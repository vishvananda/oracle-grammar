//! `@sem` patterns: text form, elaboration against the IR schema, and the two
//! directions of use. A filled pattern is ordinary serde JSON for the IR type;
//! a hole binds a child's value, never an omitted constraint.
use crate::functions::target_scopes::unify as unify_targets;
use crate::functions::{
    agreed, applied, conjunction, continuous_parts, either_event, incremented, negated, product,
    substituted, subtype_name, unify_agree, unify_and, unify_apply, unify_continuous_parts,
    unify_either_event, unify_negate, unify_plus_one, unify_subtype_name, unify_times,
    unify_token_abilities, unify_where, unify_zone_members, with_token_abilities,
};
use crate::model::ir_schema::{Fields, Schema, Ty, TypeDef};
use serde_json::{Map, Value};
use std::borrow::Cow;

mod constraints;
mod damage_actor;
mod reference_symbols;
pub(crate) mod root_type;
use constraints::{refers_to, unconstrained};

/// The syntax written in a declaration, before types are known.
#[derive(Debug, Clone, PartialEq)]
pub enum Syntax {
    Hole(String),
    Splice(String),
    Node(String, Args),
    List(Vec<Syntax>),
    Int(i64),
    Bool(bool),
    Text(String),
    /// A reference word the binder resolves: `@target(...)`, `@it`.
    Symbol(String, Vec<Syntax>),
    /// `Type::value`, where a position gives no type of its own.
    Typed(String, Box<Syntax>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Args {
    None,
    Tuple(Vec<Syntax>),
    Named(Vec<(String, Syntax)>),
}

/// An elaborated pattern: the serialized shape of an IR value, with holes.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Value(Value),
    Hole(String, Ty),
    Object(Vec<(String, Pattern)>),
    List(Vec<Item>),
    /// `{"$sym": name, "args": [...]}` once filled; only the binder reads these.
    Symbol(String, Vec<Pattern>),
}

/// One element of a list pattern.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    One(Pattern),
    Splice(String, Ty),
}

/// What `@card_name` matched: no hole has this name.
pub const CARD_NAME: &str = "@card_name";

/// What holes hold: values borrowed from the one being matched wherever that can be, so
/// that matching copies nothing. `'p` is the pattern's life, `'v` the values'.
#[derive(Debug, Clone, Default)]
pub struct Bindings<'p, 'v>(Vec<(&'p str, Cow<'v, Value>)>);

impl<'p, 'v> Bindings<'p, 'v> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        let found = self.0.iter().find(|(hole, _)| *hole == name);
        found.map(|(_, value)| value.as_ref())
    }

    /// Set a hole, replacing what it held.
    pub fn insert(&mut self, name: &'p str, value: Cow<'v, Value>) {
        match self.0.iter_mut().find(|(hole, _)| *hole == name) {
            Some(held) => held.1 = value,
            None => self.0.push((name, value)),
        }
    }

    /// The same bindings, holding their own values, for a match that outlives what it read.
    fn owned<'o>(self) -> Bindings<'p, 'o> {
        let held = self.0.into_iter();
        Bindings(
            held.map(|(name, value)| (name, Cow::Owned(value.into_owned())))
                .collect(),
        )
    }
}

pub fn parse(text: &str) -> Result<(Option<String>, Syntax), String> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    reader.space();
    let mut root = None;
    // Structural annotations are explicit; ordinary nested Type:: patterns keep
    // using the existing value parser and Named::list remains legacy sugar.
    if let Some((prefix, _)) = text.split_once("::") {
        let trimmed = prefix.trim();
        if trimmed.starts_with('(')
            || ["Vec", "Option"].iter().any(|name| {
                trimmed
                    .strip_prefix(name)
                    .is_some_and(|rest| rest.trim_start().starts_with('<'))
            })
        {
            root_type::parse(trimmed)?;
            root = Some(trimmed.into());
            reader.at = prefix.len() + 2;
        }
    }
    let start = reader.at;
    if root.is_none()
        && let Ok(name) = reader.ident()
        && reader.eat("::")
    {
        root = Some(name);
    } else if root.is_none() {
        reader.at = start;
    }
    let value = reader.value(0)?;
    reader.space();
    if reader.at != reader.bytes.len() {
        return Err(format!("trailing pattern text at {}", reader.at));
    }
    Ok((root, value))
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn space(&mut self) {
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }
    fn eat(&mut self, token: &str) -> bool {
        self.space();
        let found = self.bytes[self.at..].starts_with(token.as_bytes());
        if found {
            self.at += token.len();
        }
        found
    }
    fn next_is(&mut self, token: &str) -> bool {
        self.space();
        self.bytes[self.at..].starts_with(token.as_bytes())
    }
    fn word(&mut self) -> &str {
        let start = self.at;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            self.at += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or_default()
    }
    fn ident(&mut self) -> Result<String, String> {
        self.space();
        let word = self.word();
        if word.is_empty() {
            Err("expected a name".into())
        } else {
            Ok(word.to_string())
        }
    }
    /// A quoted string, already past its opening quote. `''` is one apostrophe.
    fn text(&mut self) -> Result<String, String> {
        let mut bytes = Vec::new();
        while let Some(byte) = self.bytes.get(self.at).copied() {
            self.at += 1;
            if byte != b'\'' {
                bytes.push(byte);
            } else if self.bytes.get(self.at) == Some(&b'\'') {
                self.at += 1;
                bytes.push(byte);
            } else {
                break;
            }
        }
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
    fn list(&mut self, close: &str, depth: usize) -> Result<Vec<Syntax>, String> {
        let mut items = Vec::new();
        loop {
            if self.eat(close) {
                return Ok(items);
            }
            items.push(self.value(depth + 1)?);
            if !self.eat(",") && !self.next_is(close) {
                return Err("expected , or a closing bracket".into());
            }
        }
    }
    fn named(&mut self, depth: usize) -> Result<Vec<(String, Syntax)>, String> {
        let mut items = Vec::new();
        loop {
            if self.eat("}") {
                return Ok(items);
            }
            let name = self.ident()?;
            if !self.eat(":") {
                return Err(format!("expected : after {name}"));
            }
            items.push((name, self.value(depth + 1)?));
            if !self.eat(",") && !self.next_is("}") {
                return Err("expected , or }".into());
            }
        }
    }
    fn value(&mut self, depth: usize) -> Result<Syntax, String> {
        if depth > 64 {
            return Err("pattern nesting limit".into());
        }
        if self.eat("...$") {
            return Ok(Syntax::Splice(self.ident()?));
        }
        if self.eat("$") {
            return Ok(Syntax::Hole(self.ident()?));
        }
        if self.eat("@") {
            let name = self.ident()?;
            let args = if self.eat("(") {
                self.list(")", depth)?
            } else {
                Vec::new()
            };
            return Ok(Syntax::Symbol(name, args));
        }
        if self.eat("[") {
            return Ok(Syntax::List(self.list("]", depth)?));
        }
        if self.eat("'") {
            return Ok(Syntax::Text(self.text()?));
        }
        self.space();
        if self
            .bytes
            .get(self.at)
            .is_some_and(|b| b.is_ascii_digit() || *b == b'-')
        {
            let start = self.at;
            self.at += 1;
            let _ = self.word();
            let digits = std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or_default();
            return digits
                .parse()
                .map(Syntax::Int)
                .map_err(|_| format!("bad number {digits}"));
        }
        let name = self.ident()?;
        if name == "true" || name == "false" {
            return Ok(Syntax::Bool(name == "true"));
        }
        if self.eat("::") {
            return Ok(Syntax::Typed(name, Box::new(self.value(depth + 1)?)));
        }
        let args = if self.eat("(") {
            Args::Tuple(self.list(")", depth)?)
        } else if self.eat("{") {
            Args::Named(self.named(depth)?)
        } else {
            Args::None
        };
        Ok(Syntax::Node(name, args))
    }
}

pub fn elaborate(syntax: &Syntax, ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    if let Syntax::Symbol(name, args) = syntax
        && let Some(pattern) = crate::functions::checked_call(name, args, ty, schema)?
    {
        return Ok(pattern);
    }
    if let (Syntax::Symbol(name, _), Ty::Named(reference)) = (syntax, ty)
        && name == "target"
        && reference != "TargetSlot"
        && schema.types.contains_key(reference)
    {
        return Err(format!("@target fills a slot number, not a {reference}"));
    }
    match (syntax, ty) {
        (Syntax::Hole(name), _) => Ok(Pattern::Hole(name.clone(), ty.clone())),
        (Syntax::Typed(name, inner), _) => elaborate(inner, &Ty::Named(name.clone()), schema),
        (Syntax::Symbol(name, args), _) => Ok(Pattern::Symbol(
            name.clone(),
            args.iter()
                .map(|a| elaborate_any(a, schema))
                .collect::<Result<_, _>>()?,
        )),
        (_, Ty::Option(inner)) => match syntax {
            Syntax::Node(name, Args::None) if name == "None" => Ok(Pattern::Value(Value::Null)),
            Syntax::Node(name, Args::Tuple(items)) if name == "Some" && items.len() == 1 => {
                elaborate(&items[0], inner, schema)
            }
            _ => elaborate(syntax, inner, schema),
        },
        (Syntax::List(items), Ty::Vec(inner)) => list(items, inner, schema),
        (Syntax::List(items), Ty::Tuple(parts)) if items.len() == parts.len() => {
            let items: Vec<Pattern> = items
                .iter()
                .zip(parts)
                .map(|(item, part)| elaborate(item, part, schema))
                .collect::<Result<_, _>>()?;
            Ok(Pattern::List(items.into_iter().map(Item::One).collect()))
        }
        (Syntax::Int(value), _) => Ok(Pattern::Value(Value::from(*value))),
        (Syntax::Bool(value), _) => Ok(Pattern::Value(Value::Bool(*value))),
        (Syntax::Text(value), _) => Ok(Pattern::Value(Value::from(value.clone()))),
        (Syntax::Node(name, args), Ty::Named(ty)) => node(name, args, ty, schema),
        (Syntax::Node(name, args), Ty::Opaque) => {
            if matches!(args, Args::None) {
                Ok(Pattern::Value(Value::from(name.clone())))
            } else {
                Err(format!("{name} has arguments in an unchecked position"))
            }
        }
        _ => Err(format!("pattern {syntax:?} does not fit {ty:?}")),
    }
}

/// Binder symbols take arguments of any type, so each names its own.
fn elaborate_any(syntax: &Syntax, schema: &Schema) -> Result<Pattern, String> {
    match syntax {
        Syntax::Hole(name) => Ok(Pattern::Hole(name.clone(), Ty::Opaque)),
        Syntax::Typed(..) | Syntax::Symbol(..) | Syntax::Int(_) | Syntax::Text(_) => {
            elaborate(syntax, &Ty::Opaque, schema)
        }
        other => Err(format!("a symbol argument needs Type:: form: {other:?}")),
    }
}

fn list(items: &[Syntax], inner: &Ty, schema: &Schema) -> Result<Pattern, String> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(match item {
            Syntax::Splice(name) => Item::Splice(name.clone(), Ty::Vec(Box::new(inner.clone()))),
            other => Item::One(elaborate(other, inner, schema)?),
        });
    }
    Ok(Pattern::List(out))
}

fn node(name: &str, args: &Args, ty: &str, schema: &Schema) -> Result<Pattern, String> {
    match schema.types.get(ty) {
        Some(TypeDef::Enum { .. }) => {
            let (adjacent, fields) = schema
                .variant(ty, name)
                .ok_or_else(|| format!("{ty} has no variant {name}"))?;
            let content = content(args, fields, schema, &format!("{ty}::{name}"))?;
            Ok(wrap(name, adjacent, content))
        }
        Some(TypeDef::Struct(fields)) => {
            // `_{...}` is a record of whatever type the position holds.
            if name != ty && name != "_" {
                return Err(format!("expected a {ty} record, found {name}"));
            }
            content(args, fields, schema, ty)?.ok_or_else(|| format!("{ty} needs fields"))
        }
        None => {
            if matches!(args, Args::None) {
                Ok(Pattern::Value(Value::from(name.to_string())))
            } else {
                Ok(wrap(
                    name,
                    false,
                    Some(match args {
                        Args::Tuple(items) if items.len() == 1 => {
                            elaborate(&items[0], &Ty::Opaque, schema)?
                        }
                        _ => return Err(format!("{ty}::{name}: unsupported opaque payload")),
                    }),
                ))
            }
        }
    }
}

fn wrap(name: &str, adjacent: bool, content: Option<Pattern>) -> Pattern {
    match (adjacent, content) {
        (false, None) => Pattern::Value(Value::from(name.to_string())),
        (false, Some(content)) => Pattern::Object(vec![(name.to_string(), content)]),
        (true, None) => Pattern::Object(vec![("kind".into(), Pattern::Value(name.into()))]),
        (true, Some(content)) => Pattern::Object(vec![
            ("kind".into(), Pattern::Value(name.into())),
            ("args".into(), content),
        ]),
    }
}

fn content(
    args: &Args,
    fields: &Fields,
    schema: &Schema,
    label: &str,
) -> Result<Option<Pattern>, String> {
    match (args, fields) {
        (Args::None, Fields::Unit) => Ok(None),
        // `Variant{...}` for a variant that holds one record.
        (Args::Named(_), Fields::Tuple(types)) if types.len() == 1 => {
            let record = Syntax::Node("_".into(), args.clone());
            Ok(Some(
                elaborate(&record, &types[0], schema).map_err(|e| format!("{label}: {e}"))?,
            ))
        }
        // `Variant([a, b])` for a tuple variant: serialized alike, so the miner cannot tell.
        (Args::Tuple(items), Fields::Tuple(types)) if items.len() == 1 && types.len() > 1 => {
            match items.as_slice() {
                [Syntax::List(spread)] => {
                    content(&Args::Tuple(spread.clone()), fields, schema, label)
                }
                _ => Err(format!("{label}: arguments do not fit its fields")),
            }
        }
        // `Variant(_{...})` for a variant with fields of its own: the miner cannot tell them apart.
        (Args::Tuple(items), Fields::Named(_)) => match items.as_slice() {
            [Syntax::Node(name, args)] if name == "_" => content(args, fields, schema, label),
            _ => Err(format!("{label}: arguments do not fit its fields")),
        },
        (Args::Tuple(items), Fields::Tuple(types)) if items.len() == types.len() => {
            let mut parts = items
                .iter()
                .zip(types)
                .map(|(item, ty)| elaborate(item, ty, schema))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("{label}: {e}"))?;
            if parts.len() == 1 {
                Ok(parts.pop())
            } else {
                Ok(Some(Pattern::List(
                    parts.into_iter().map(Item::One).collect(),
                )))
            }
        }
        (Args::Named(items), Fields::Named(types)) => {
            let mut object = Vec::new();
            for (name, ty) in types {
                let item = items.iter().find(|(field, _)| field == name);
                let value = match (item, ty) {
                    (Some((_, syntax)), _) => {
                        elaborate(syntax, ty, schema).map_err(|e| format!("{label}.{name}: {e}"))?
                    }
                    (None, Ty::Option(_)) => Pattern::Value(Value::Null),
                    (None, Ty::Vec(_)) => Pattern::List(Vec::new()),
                    (None, Ty::Named(kind)) if kind == "bool" => Pattern::Value(Value::Bool(false)),
                    (None, _) => return Err(format!("{label} is missing field {name}")),
                };
                object.push((name.clone(), value));
            }
            if let Some((unknown, _)) = items
                .iter()
                .find(|(name, _)| !types.iter().any(|(field, _)| field == name))
            {
                return Err(format!("{label} has no field {unknown}"));
            }
            Ok(Some(Pattern::Object(object)))
        }
        _ => Err(format!("{label}: arguments do not fit its fields")),
    }
}

impl Pattern {
    /// Forward: replace each hole with the child's value. `at` is the token the
    /// production started on, which orders the binder's work as the text reads.
    pub fn fill(&self, bindings: &Bindings, at: usize) -> Result<Value, String> {
        match self {
            Self::Value(value) => Ok(value.clone()),
            Self::Hole(name, _) => bindings
                .get(name)
                .cloned()
                .ok_or_else(|| format!("unbound hole {name}")),
            Self::Object(fields) => {
                let mut object = Map::with_capacity(fields.len());
                for (name, pattern) in fields {
                    object.insert(name.clone(), pattern.fill(bindings, at)?);
                }
                Ok(Value::Object(object))
            }
            Self::List(pieces) => {
                let mut items = Vec::new();
                for piece in pieces {
                    match piece {
                        Item::One(pattern) => items.push(pattern.fill(bindings, at)?),
                        Item::Splice(name, _) => match bindings.get(name) {
                            Some(Value::Array(middle)) => items.extend(middle.iter().cloned()),
                            _ => return Err(format!("unbound splice {name}")),
                        },
                    }
                }
                Ok(Value::Array(items))
            }
            Self::Symbol(name, args) => {
                let args = args
                    .iter()
                    .map(|a| a.fill(bindings, at))
                    .collect::<Result<Vec<_>, _>>()?;
                match (name.as_str(), args.as_slice()) {
                    ("single_player", [players]) => {
                        if players["kind"] != "One" {
                            return Err("one player is required".into());
                        }
                        return Ok(players["args"].clone());
                    }
                    ("implicit_card_set", [objects]) => {
                        return Ok(crate::bind::exile_followups::lexical(objects, at));
                    }
                    ("returned_conditional" | "returned_conditional_literal", [instructions]) => {
                        return crate::bind::returned_conditionals::annotate(
                            instructions,
                            name == "returned_conditional_literal",
                        );
                    }
                    ("implicit_return_grant" | "return_grant", [operation]) => {
                        return crate::bind::return_grants::annotate(
                            operation,
                            at,
                            name == "implicit_return_grant",
                        );
                    }
                    ("enchanted_creature_operand", [reference]) => {
                        return crate::bind::attached_counter_other::literal(reference, at);
                    }
                    ("departure_controller", [event, filter]) => {
                        return crate::bind::departure_controller::annotate(event, filter, at);
                    }
                    ("entering_controller", [ability]) => {
                        return crate::functions::entering::bind(ability);
                    }
                    ("class_levels", [rows]) => return crate::functions::class_levels::lower(rows),
                    ("and_sequence", [head, tail]) => {
                        return crate::functions::optional_scope::sequence(head, tail);
                    }
                    ("then_sequence" | "optional_then", [head, tail]) => {
                        return crate::functions::optional_scope::then_sequence(
                            head,
                            tail,
                            name == "optional_then",
                        );
                    }
                    ("single_reflexive_base", [actions]) => {
                        return crate::functions::optional_scope::single_base(actions);
                    }
                    ("offered_action", [actions]) => {
                        return crate::functions::optional_scope::offered_action(actions);
                    }
                    ("optional_action_program", [program]) => {
                        return crate::functions::optional_scope::action_program(program);
                    }
                    ("optional_program", [program]) => {
                        return crate::functions::optional_scope::program(program);
                    }
                    ("captured_targets" | "announced_targets", [actions]) => {
                        return crate::functions::target_scopes::guard(
                            actions,
                            name == "announced_targets",
                        );
                    }
                    ("unordered", [items]) => {
                        if !items.is_array() {
                            return Err("@unordered requires an array value".into());
                        }
                        return args
                            .into_iter()
                            .next()
                            .ok_or_else(|| "missing @unordered argument".into());
                    }
                    ("and", _) => return Ok(conjunction(args)),
                    ("event_free_once", [ability]) => {
                        return crate::functions::trigger_batches::mark(ability);
                    }
                    ("either_event", [left, right]) => return Ok(either_event(left, right)),
                    ("agree", [first, repeated]) => return agreed(first, repeated),
                    ("mana_options", [colors]) => return crate::functions::mana_options(colors),
                    ("singular_count" | "plural_count" | "plural_amount", [count]) => {
                        return crate::functions::counts::guard(name, count);
                    }
                    ("fixed_mana" | "variable_mana", [cost]) => {
                        return crate::functions::guard_cost(name, cost);
                    }
                    ("inline_counter_price" | "captured_counter_price", [expression]) => {
                        return crate::functions::guard_counter_price(name, expression);
                    }
                    ("keyword_abilities", [keywords]) => {
                        return crate::functions::keyword_list(keywords, "Keyword");
                    }
                    ("keyword_changes", [keywords]) => {
                        return crate::functions::keyword_list(keywords, "GrantKeyword");
                    }
                    ("continuous_parts", [template]) => return continuous_parts(template),
                    (name @ ("defining_static" | "defining_changes"), [value]) => {
                        return crate::functions::definitions::mark(name, value);
                    }
                    ("retain_animation_types", [changes]) => {
                        return crate::functions::retain_animation_types(changes);
                    }
                    ("retain_animation", [operation]) => {
                        return crate::functions::retain_animation(operation);
                    }
                    ("arrival_changes", [operation, changes]) => {
                        return crate::functions::arrival::changes(operation, changes);
                    }
                    ("times", [factor, count]) => return Ok(product(factor, count)),
                    ("repeated_damage_actor", [instruction]) => {
                        crate::functions::branch_bounds(instruction)?;
                        return crate::functions::repeated_damage_actor::capture(
                            instruction.clone(),
                        );
                    }
                    ("apply", [subject, body]) => return Ok(applied(subject, body.clone())),
                    ("computed_condition", [condition]) => {
                        return crate::functions::computed_condition::guard(condition).cloned();
                    }
                    ("where", [body, value]) => {
                        if crate::functions::value_snapshot::required(body, value) {
                            return crate::functions::value_snapshot::snapshot(body, value, at);
                        }
                        return Ok(substituted(body.clone(), value));
                    }
                    ("where_snapshot", [body, value]) => {
                        return crate::functions::value_snapshot::snapshot(body, value, at);
                    }
                    ("capture_amount", [body, value]) => {
                        return crate::functions::capture_amount::lower(body, value, at);
                    }
                    ("negate", [value]) => return Ok(negated(value)),
                    ("in_filter", [value]) => return Ok(crate::bind::filter_operand(value)),
                    ("gap", [value]) => {
                        return Ok(serde_json::json!({"kind": "Gap", "args": value}));
                    }
                    ("subtype_name", [value]) => return Ok(subtype_name(value)),
                    ("plus_one", [value]) => return incremented(value),
                    ("token_abilities", [base, abilities]) => {
                        return with_token_abilities(base, abilities);
                    }
                    ("length", [Value::Array(items)]) => {
                        return Ok(serde_json::json!({"kind": "Constant", "args": items.len()}));
                    }
                    _ => {}
                }
                Ok(serde_json::json!({"$sym": name, "args": args, "at": at}))
            }
        }
    }

    /// Backward: every way this pattern matches a value, each with what its
    /// holes must render. Most patterns match once; `@and` can split many ways.
    pub fn unify<'p, 'v>(
        &'p self,
        value: &'v Value,
        bindings: Bindings<'p, 'v>,
    ) -> Vec<Bindings<'p, 'v>> {
        match self {
            Self::Value(expected) => {
                if expected == value {
                    vec![bindings]
                } else {
                    Vec::new()
                }
            }
            Self::Hole(name, _) => bind(name, Cow::Borrowed(value), bindings)
                .into_iter()
                .collect(),
            Self::Object(fields) => {
                let Some(object) = value.as_object() else {
                    return Vec::new();
                };
                if object.len() != fields.len() {
                    return Vec::new();
                }
                let mut solutions = vec![bindings];
                for (name, pattern) in fields {
                    let Some(field) = object.get(name) else {
                        return Vec::new();
                    };
                    solutions = solutions
                        .into_iter()
                        .flat_map(|b| pattern.unify(field, b))
                        .collect();
                }
                solutions
            }
            Self::List(pieces) => match value.as_array() {
                Some(items) => unify_items(pieces, items, bindings),
                None => Vec::new(),
            },
            Self::Symbol(name, args) => self.unify_symbol(name, args, value, bindings),
        }
    }

    /// Match a value made along the way, which the bindings cannot go on borrowing.
    pub(crate) fn unify_made<'p, 'v>(
        &'p self,
        made: &Value,
        bindings: &Bindings<'p, 'v>,
    ) -> Vec<Bindings<'p, 'v>> {
        let found = self.unify(made, bindings.clone());
        found.into_iter().map(Bindings::owned).collect()
    }

    fn unify_symbol<'p, 'v>(
        &'p self,
        name: &str,
        args: &'p [Self],
        value: &'v Value,
        bindings: Bindings<'p, 'v>,
    ) -> Vec<Bindings<'p, 'v>> {
        let name = crate::bind::damage_relative::ordinary_name(name);
        if name == "card_name" {
            // Only the printer knows the card it prints for: it holds this text to that name.
            let mut bindings = bindings;
            bindings.insert(CARD_NAME, Cow::Borrowed(value));
            return vec![bindings];
        }
        if unconstrained(name) || refers_to(name, value) {
            return vec![bindings];
        }
        if matches!(name, "group_reference" | "that_much" | "that_many") {
            return reference_symbols::unify(name, value, bindings);
        }
        match name {
            "single_player" => {
                return args[0]
                    .unify_made(&serde_json::json!({"kind":"One","args":value}), &bindings);
            }
            "selection_id" | "reflexive_performed" => {
                return if value.as_u64().is_some_and(|id| id <= u64::from(u32::MAX)) {
                    vec![bindings]
                } else {
                    Vec::new()
                };
            }
            "return_ref" => return reference_symbols::return_ref(args, value, bindings),
            "selection" => {
                if value["$sym"] != "selection" {
                    return Vec::new();
                }
                return match (args.first(), value["args"].get(0)) {
                    (Some(pattern), Some(step)) => pattern.unify(step, bindings),
                    _ => Vec::new(),
                };
            }
            "target_contrast" if value.get("$sym").is_none() => {
                return match args {
                    [fallback, _domain] => fallback.unify(value, bindings),
                    _ => Vec::new(),
                };
            }
            "activation_creature_property" if value.get("$sym").is_none() => {
                return crate::bind::activation_property::unify(args, value, bindings);
            }
            "sacrificed_damage_source" => {
                return reference_symbols::sacrificed_damage_source(args, value, bindings);
            }
            "last_exiled" | "last_exiled_creature" => {
                if value["kind"] != "Earlier"
                    || value["args"]["what"] != "LastExiled"
                    || !value["args"]["by"].is_null()
                {
                    return Vec::new();
                }
                return args[0].unify_made(&serde_json::json!({"kind":"Source"}), &bindings);
            }
            "qualified_members" => {
                return reference_symbols::qualified_members(args, value, bindings);
            }
            "qualified_object" if constraints::qualified_reference(args, value) => {
                return vec![bindings];
            }
            "created_reference"
            | "created_reference_single"
            | "created_batch_reference"
            | "created_named_reference"
                if value["kind"] == "Created" && value["args"].is_u64() =>
            {
                return vec![bindings];
            }
            "counter_reference" | "counter_reference_single"
                if value["kind"] == "Affected" && value["args"].is_u64() =>
            {
                return vec![bindings];
            }
            "singular_count" | "plural_count" | "plural_amount" => {
                return crate::functions::counts::unify(name, args, value, bindings);
            }
            "kicker_branch" => return crate::functions::unify_kicker_branch(args, value, bindings),
            "conditional_ability" => {
                return crate::functions::conditional::unify(args, value, bindings);
            }
            "other_reference" if value["$sym"] == "attached_counter_other_view" => {
                return crate::bind::attached_counter_other::unify_other(args, value, bindings);
            }
            "departure_controller" => {
                return crate::bind::departure_controller::unify(args, value, bindings);
            }
            "entering_controller" => {
                return crate::functions::entering::unify(args, value, bindings);
            }
            "arrival_changes" => return crate::functions::arrival::unify(args, value, bindings),
            "continuous_parts" => return unify_continuous_parts(args, value, bindings),
            "defining_static"
            | "defining_changes"
            | "retain_animation_types"
            | "retain_animation" => {
                return crate::functions::unify_continuous(name, args, value, bindings);
            }
            "zone_members" => return unify_zone_members(args, value, bindings),
            // A chance selection is itself the moved set.
            "implicit_card_set" | "exile_followup"
                if value.get("$sym").is_none_or(|sym| sym == "selection") =>
            {
                return crate::bind::exile_followups::unify(args, value, bindings);
            }
            "returned_conditional" | "returned_conditional_literal" => {
                return unify_returned(name, args, value, bindings);
            }
            "implicit_return_grant" | "return_grant" if value.get("$sym").is_none() => {
                return crate::bind::return_grants::unify(args, value, bindings);
            }
            "class_levels" => return crate::functions::class_levels::unify(args, value, bindings),
            "and_sequence"
            | "then_sequence"
            | "optional_then"
            | "single_reflexive_base"
            | "offered_action"
            | "optional_program"
            | "optional_action_program" => {
                return crate::functions::optional_scope::unify(name, args, value, bindings);
            }
            "captured_targets" | "announced_targets" => {
                return unify_targets(args, value, bindings);
            }
            "unordered" => return crate::functions::unordered::unify(args, value, bindings),
            "and" => return unify_and(args, value, bindings),
            "either_event" => return unify_either_event(args, value, bindings),
            "agree" => return unify_agree(args, value, bindings),
            "mana_options" => return crate::functions::unify_mana_options(args, value, bindings),
            "fixed_mana" | "variable_mana" => {
                return crate::functions::unify_cost_guard(name, args, value, bindings);
            }
            "inline_counter_price" | "captured_counter_price" => {
                return crate::functions::unify_counter_price(name, args, value, bindings);
            }
            "payment_cost" => return crate::functions::payment::unify(args, value, bindings),
            "announced_cost" => {
                return crate::functions::payment::unify_announced(args, value, bindings);
            }
            "keyword_abilities" => {
                return crate::functions::unify_keyword_list(args, value, bindings, "Keyword");
            }
            "keyword_changes" => {
                return crate::functions::unify_keyword_list(args, value, bindings, "GrantKeyword");
            }
            "times" => return unify_times(args, value, bindings),
            "repeated_damage_actor" => {
                return crate::functions::repeated_damage_actor::unify(args, value, bindings);
            }
            "apply" => return unify_apply(args, value, bindings),
            "computed_condition" => {
                return crate::functions::computed_condition::unify(args, value, bindings);
            }
            "where" => return unify_where(args, value, bindings),
            "where_snapshot" => {
                return crate::functions::value_snapshot::unify(args, value, bindings);
            }
            "capture_amount" => {
                return crate::functions::capture_amount::unify(args, value, bindings);
            }
            "token_abilities" => return unify_token_abilities(args, value, bindings),
            "negate" => return unify_negate(args, value, bindings),
            "subtype_name" => return unify_subtype_name(args, value, bindings),
            "reflexive" => return unify_reflexive(args, value, bindings),
            "member_count_it" if value.get("$sym").is_none() => {
                return crate::bind::member_focus::unify(args, value, bindings);
            }
            "distributive_pump_subject" | "pump_other_ref" | "pump_name_ref"
                if value.get("$sym").is_none() =>
            {
                return crate::bind::pump_counts::unify(name, args, value, bindings);
            }
            "literal_damage_itself" if crate::bind::damage_reflexive::inverse_candidate(value) => {
                return crate::bind::damage_reflexive::unify(args, value, bindings);
            }
            "damage_actor_value" if value.get("$sym").is_none() => {
                return damage_actor::leading(args, value, bindings);
            }
            "its_stat" if value.get("$sym").is_none() || damage_actor::is_view(value) => {
                return damage_actor::literal(args, value, bindings);
            }
            "recipient_value" if value.get("$sym").is_none() => {
                return args
                    .get(1)
                    .map_or_else(Vec::new, |inner| inner.unify(value, bindings));
            }
            "remember_object" | "remember_operand" if value["$sym"] == "qualified_object" => {
                return args
                    .first()
                    .map_or_else(Vec::new, |inner| inner.unify(value, bindings));
            }
            // A scope leaves no mark in the IR, so a bound value is its own inside.
            _ if transparent_symbol(name) && value.get("$sym").is_none() => {
                return args
                    .first()
                    .map_or_else(Vec::new, |inner| inner.unify(value, bindings));
            }
            "plus_one" => return unify_plus_one(args, value, bindings),
            _ => {}
        }
        unify_symbol_arguments(name, args, value, bindings)
    }

    pub fn holes(&self, out: &mut Vec<(String, Ty)>) {
        match self {
            Self::Value(_) => {}
            Self::Hole(name, ty) => out.push((name.clone(), ty.clone())),
            Self::Object(fields) => fields.iter().for_each(|(_, p)| p.holes(out)),
            Self::List(pieces) => {
                for piece in pieces {
                    match piece {
                        Item::One(pattern) => pattern.holes(out),
                        Item::Splice(name, ty) => out.push((name.clone(), ty.clone())),
                    }
                }
            }
            Self::Symbol(_, args) => args.iter().for_each(|p| p.holes(out)),
        }
    }
}

/// Structural fallback for a symbol not handled by the contextual dispatch.
/// Keep the name, arity and argument matching separate from those special cases.
fn unify_symbol_arguments<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    if object.get("$sym").and_then(Value::as_str) != Some(name) {
        return Vec::new();
    }
    let Some(items) = object.get("args").and_then(Value::as_array) else {
        return Vec::new();
    };
    if items.len() != args.len() {
        return Vec::new();
    }
    let mut solutions = vec![bindings];
    for (pattern, item) in args.iter().zip(items) {
        solutions = solutions
            .into_iter()
            .flat_map(|b| pattern.unify(item, b))
            .collect();
    }
    solutions
}

fn unify_returned<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    crate::bind::returned_conditionals::unify(
        args,
        value,
        bindings,
        name == "returned_conditional_literal",
    )
}

fn transparent_symbol(name: &str) -> bool {
    matches!(
        name,
        "look_inline"
            | "indefinite_opponent_controller"
            | "linked_return_literal"
            | "literal_damage_itself_objects"
            | "event_free_once"
            | "scope"
            | "holder"
            | "in_filter"
            | "remember_object"
            | "fight_prior_counterpart"
            | "reflexive_moved_fight_object"
            | "move_after_current_type"
            | "move_after_actor"
            | "move_after_property"
            | "move_after_other"
            | "remember_operand"
            | "owner_return"
            | "enchanted_operand"
            | "enchanted_creature_operand"
            | "counter_choice"
            | "manifest_result"
            | "created_result"
            | "selected_result"
            | "drafted_result"
            | "exiled_card_result"
            | "search_move_result"
            | "seek_move_result"
            | "resolution"
            | "leading_if"
            | "announced_x"
            | "sacrificed_result"
            | "measured_sacrifice"
    )
}

fn unify_reflexive<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    // Modal surface forms use the ordinary scope wrapper. A reflexive
    // mode shares result allocation but has the same target surface.
    let value = if value["$sym"] == "scope" {
        let Some([mode]) = value["args"].as_array().map(Vec::as_slice) else {
            return Vec::new();
        };
        if crate::traverse::boundary(mode) != crate::traverse::Boundary::Mode
            || mode.get("body").is_none()
            || mode.get("kind").is_some()
        {
            return Vec::new();
        }
        mode
    } else {
        value
    };
    args.first()
        .map_or_else(Vec::new, |inner| inner.unify(value, bindings))
}

fn bind<'p, 'v>(
    name: &'p str,
    value: Cow<'v, Value>,
    mut bindings: Bindings<'p, 'v>,
) -> Option<Bindings<'p, 'v>> {
    match bindings.get(name) {
        Some(previous) => (*previous == *value).then_some(bindings),
        None => {
            bindings.0.push((name, value));
            Some(bindings)
        }
    }
}

fn unify_items<'p, 'v>(
    pieces: &'p [Item],
    items: &'v [Value],
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    match pieces.split_first() {
        None => {
            if items.is_empty() {
                vec![bindings]
            } else {
                Vec::new()
            }
        }
        Some((Item::One(pattern), rest)) => match items.split_first() {
            None => Vec::new(),
            Some((first, others)) => pattern
                .unify(first, bindings)
                .into_iter()
                .flat_map(|b| unify_items(rest, others, b))
                .collect(),
        },
        Some((Item::Splice(name, _), rest)) => {
            // With no other splice after it, the items left to match fix where this ends.
            let fixed = !rest.iter().any(|piece| matches!(piece, Item::Splice(..)));
            let splits = if fixed {
                let Some(split) = items.len().checked_sub(rest.len()) else {
                    return Vec::new();
                };
                split..=split
            } else {
                0..=items.len()
            };
            let mut solutions = Vec::new();
            for split in splits {
                let taken = Value::Array(items[..split].to_vec());
                if let Some(b) = bind(name, Cow::Owned(taken), bindings.clone()) {
                    solutions.extend(unify_items(rest, &items[split..], b));
                }
            }
            solutions
        }
    }
}

#[cfg(test)]
mod reflexive_mode_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reflexive_inverse_accepts_only_one_scoped_mode() {
        let mode = json!({"announcement":{"targets":[],"modes":null},"body":{"instructions":[]},"weight":1});
        let pattern = Pattern::Symbol("reflexive".into(), vec![Pattern::Value(mode.clone())]);
        let wrapped = json!({"$sym":"scope","args":[mode.clone()],"at":0});
        assert_eq!(pattern.unify(&mode, Bindings::new()).len(), 1);
        assert_eq!(pattern.unify(&wrapped, Bindings::new()).len(), 1);
        for value in [
            json!({"$sym":"scope","args":[]}),
            json!({"$sym":"scope","args":[mode.clone(),mode.clone()]}),
            json!({"$sym":"holder","args":[mode.clone()]}),
            json!({"$sym":"scope","args":[{"kind":"Spell","args":{"announcement":{},"body":{}}}]}),
            json!({"$sym":"scope","args":[{"announcement":{},"weight":1}]}),
        ] {
            assert!(pattern.unify(&value, Bindings::new()).is_empty());
        }
    }
}
