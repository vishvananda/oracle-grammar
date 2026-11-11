//! Loads `.ebnf` source with its `@sem` declarations into dense productions.
//! `tabnas-ebnf` skips comments, so a pre-pass attaches each declaration to the
//! production after it.
use crate::model::ir_schema::{Schema, Ty};
use crate::pattern::{self, Pattern};
use crate::reader;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::ops::Range;
use tabnas_ebnf::{Element, Kind, Production};

pub type RuleId = u32;

/// One token class of `lexical.ebnf`; the tokenizer produces exactly these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Class {
    Number,
    Word,
    Capitalized,
    SelfName,
    Newline,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Sym {
    Word(String),
    Mark(char),
    Class(Class),
    Rule(RuleId),
}

#[derive(Debug, Clone)]
pub struct Alternative {
    pub syms: Vec<Sym>,
    /// For each `sym`, the hole it fills, if any.
    pub holes: Vec<Option<String>>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub alternatives: Vec<Alternative>,
    /// `None` for a pass-through rule, whose alternatives each hold one reference.
    pub sem: Option<Pattern>,
    /// The IR type this rule produces, when it is known.
    pub ty: Option<Ty>,
    pub prefer: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Grammar {
    pub rules: Vec<Rule>,
    pub names: HashMap<String, RuleId>,
    /// Every word a terminal can match, for sentence-initial case folding.
    pub vocabulary: HashSet<String>,
    pub fingerprint: String,
    /// Which rules can print a value.
    pub net: crate::heads::Net,
    /// Which alternatives can start at a token, for reading.
    pub starts: crate::starts::Starts,
    /// Only declared, literal presentation labels may be stripped from rules text.
    pub presentation_labels: Vec<String>,
}

/// A small validated catalogue, shared by built-in and hot-loaded normalization.
pub(crate) fn presentation_labels(source: &str) -> Result<Vec<String>, String> {
    let (notes, _) = declarations(source)?;
    let productions = tabnas_ebnf::parse_ebnf(source)
        .map_err(|error| error.to_string())?
        .productions;
    let by_name: HashMap<_, _> = productions.iter().map(|p| (p.name.as_str(), p)).collect();
    let mut pending = vec!["ability_label"];
    if by_name.contains_key("rules_flavor_label") {
        pending.push("rules_flavor_label");
    }
    let mut seen = HashSet::default();
    let mut labels = Vec::new();
    while let Some(name) = pending.pop() {
        if !seen.insert(name) {
            continue;
        }
        if seen.len() > 1024 {
            return Err("presentation catalogue rule limit".into());
        }
        let Some(rule) = by_name.get(name) else {
            if name == "ability_label" {
                return Ok(Vec::new());
            }
            return Err(format!("unknown presentation label rule {name}"));
        };
        if let Some(sem) = notes.get(name).and_then(|n| n.sem.as_deref()) {
            let (ty, syntax) = pattern::parse(sem)?;
            let pattern::Syntax::Text(label) = syntax else {
                return Err(format!(
                    "{name}: presentation label must be a literal String"
                ));
            };
            if ty.as_deref() != Some("String") {
                return Err(format!("{name}: presentation label must be a String"));
            }
            for alt in &rule.alts {
                let words: Option<Vec<_>> = alt
                    .iter()
                    .map(|e| match &e.kind {
                        Kind::Term { literal, .. } => Some(literal.as_str()),
                        _ => None,
                    })
                    .collect();
                if !words.is_some_and(|words| words.join(" ").eq_ignore_ascii_case(&label)) {
                    return Err(format!(
                        "{name}: presentation label disagrees with its terminals"
                    ));
                }
            }
            labels.push(label);
        } else {
            for alt in &rule.alts {
                let [
                    Element {
                        kind: Kind::Ref { name, .. },
                        ..
                    },
                ] = alt.as_slice()
                else {
                    return Err(format!("{name}: presentation aliases must name one rule"));
                };
                pending.push(name);
            }
        }
    }
    labels.sort();
    labels.dedup();
    Ok(labels)
}

/// One declaration attached to the production after it.
#[derive(Debug, Default, Clone)]
struct Notes {
    sem: Option<String>,
    prefer: Vec<String>,
}

/// What a pre-pass finds: the declarations, and where each production is.
type Declared = (HashMap<String, Notes>, Vec<Range<usize>>);

/// Find `/* @... */` comments and the production each precedes, honoring string
/// literals so a quoted `/*` is not a comment. Also gives where each production's text
/// is, without the comments between productions, which are most of the source.
fn declarations(source: &str) -> Result<Declared, String> {
    let bytes = source.as_bytes();
    let mut notes: HashMap<String, Notes> = HashMap::default();
    let mut spans = Vec::new();
    let mut start = None;
    let mut code_end = 0;
    let mut pending = Notes::default();
    let mut has_pending = false;
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'"' | b'\'' => {
                let quote = bytes[at];
                at += 1;
                while at < bytes.len() && bytes[at] != quote {
                    at += 1;
                }
                at += 1;
                code_end = at.min(bytes.len());
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let end = source[at..]
                    .find("*/")
                    .ok_or("unterminated comment in grammar")?;
                let body = source[at + 2..at + end].trim();
                if let Some(sem) = body.strip_prefix("@sem ") {
                    if pending.sem.is_some() {
                        return Err(format!("two @sem declarations in a row: {sem}"));
                    }
                    pending.sem = Some(sem.trim().to_string());
                    has_pending = true;
                } else if let Some(prefer) = body.strip_prefix("@prefer ") {
                    pending.prefer.push(prefer.trim().to_string());
                    has_pending = true;
                }
                at += end + 2;
                continue;
            }
            b'[' => {
                while at < bytes.len() && bytes[at] != b']' {
                    at += 1;
                }
                code_end = (at + 1).min(bytes.len());
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let from = at;
                while bytes
                    .get(at)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                {
                    at += 1;
                }
                let name = &source[from..at];
                let rest = source[at..].trim_start();
                if rest.starts_with("::=") {
                    if let Some(begun) = start.replace(from) {
                        spans.push(begun..code_end);
                    }
                    if has_pending {
                        notes.insert(name.to_string(), std::mem::take(&mut pending));
                        has_pending = false;
                    }
                } else if has_pending {
                    return Err(format!("a declaration has no production, found {name}"));
                }
                code_end = at;
                continue;
            }
            byte if byte.is_ascii_whitespace() => {}
            _ => code_end = at + 1,
        }
        at += 1;
    }
    if has_pending {
        return Err("a declaration at the end of the grammar has no production".into());
    }
    if let Some(begun) = start {
        spans.push(begun..code_end);
    }
    Ok((notes, spans))
}

/// Read the productions at these spans in grammar order: `tabnas-ebnf` takes every one
/// the plain reader hands back.
fn read_productions(source: &str, spans: &[Range<usize>]) -> Result<Vec<Production>, String> {
    let mut read: Vec<Option<Production>> = spans
        .iter()
        .map(|span| reader::plain_production(&source[span.clone()]))
        .collect();
    let left: Vec<usize> = (0..spans.len()).filter(|at| read[*at].is_none()).collect();
    let text: Vec<&str> = left.iter().map(|at| &source[spans[*at].clone()]).collect();
    let general = tabnas_ebnf::parse_ebnf(&text.join("\n")).map_err(|e| e.to_string())?;
    if general.productions.len() != left.len() {
        return Err("a production was lost reading the grammar".into());
    }
    for (at, production) in left.into_iter().zip(general.productions) {
        read[at] = Some(production);
    }
    Ok(read.into_iter().flatten().collect())
}

/// What the plain reader must agree on: the name, and each alternative's symbols.
fn plain_shape(production: &Production) -> Vec<Vec<String>> {
    let symbol = |element: &Element| match &element.kind {
        Kind::Term { literal, .. } => format!("t{literal}"),
        Kind::Ref { name, .. } => format!("r{name}"),
        other => format!("{other:?}"),
    };
    let mut shape = vec![vec![production.name.clone()]];
    shape.extend(
        production
            .alts
            .iter()
            .map(|alt| alt.iter().map(symbol).collect()),
    );
    shape
}

/// How many productions the plain reader took, having checked each against `tabnas-ebnf`.
pub fn plain_reader_agrees(lexical: &str, syntax: &[(&str, &str)]) -> Result<usize, String> {
    let mut source = String::from(lexical);
    for (_, text) in syntax {
        source.push('\n');
        source.push_str(text);
    }
    let (_, spans) = declarations(&source)?;
    let text: Vec<&str> = spans.iter().map(|span| &source[span.clone()]).collect();
    let general = tabnas_ebnf::parse_ebnf(&text.join("\n")).map_err(|e| e.to_string())?;
    if general.productions.len() != spans.len() {
        return Err("the spans are not the productions".into());
    }
    let mut taken = 0;
    for (text, production) in text.iter().zip(&general.productions) {
        if let Some(plain) = reader::plain_production(text) {
            taken += 1;
            if plain_shape(&plain) != plain_shape(production) {
                return Err(format!("the plain reader misread {}", production.name));
            }
        }
    }
    Ok(taken)
}

fn class_of(name: &str) -> Option<Class> {
    match name {
        "NUMBER" => Some(Class::Number),
        "WORD" => Some(Class::Word),
        "CAPITALIZED" => Some(Class::Capitalized),
        "SELF_NAME" => Some(Class::SelfName),
        "NEWLINE" => Some(Class::Newline),
        _ => None,
    }
}

struct Builder {
    grammar: Grammar,
    production_names: HashSet<String>,
    /// The production being built, and how many groups it has had so far. A group is named
    /// for its production, so a key stays right when other rules change.
    building: String,
    anonymous: usize,
}

/// `lexical` declares token classes. Their rules are checked for existence and
/// never compiled, because the tokenizer is the lexical level.
pub fn load(lexical: &str, syntax: &[(&str, &str)], schema: &Schema) -> Result<Grammar, String> {
    let mut sources = String::from(lexical);
    for (name, text) in syntax {
        sources.push_str("\n/* file: ");
        sources.push_str(name);
        sources.push_str(" */\n");
        sources.push_str(text);
    }
    let (notes, spans) = declarations(&sources)?;
    let lexical_names: HashSet<String> = tabnas_ebnf::parse_ebnf(lexical)
        .map_err(|e| e.to_string())?
        .productions
        .iter()
        .map(|p| p.name.clone())
        .collect();
    let productions = read_productions(&sources, &spans)?;
    let mut builder = Builder {
        grammar: Grammar::default(),
        production_names: productions.iter().map(|p| p.name.clone()).collect(),
        building: String::new(),
        anonymous: 0,
    };
    let semantic: Vec<_> = productions
        .iter()
        .filter(|p| !lexical_names.contains(&p.name))
        .collect();
    for (id, production) in semantic.iter().enumerate() {
        let id = RuleId::try_from(id).map_err(|e| e.to_string())?;
        builder.grammar.names.insert(production.name.clone(), id);
        builder.grammar.rules.push(Rule {
            name: production.name.clone(),
            alternatives: Vec::new(),
            sem: None,
            ty: None,
            prefer: Vec::new(),
        });
    }
    let mut refused = Vec::new();
    for production in &semantic {
        if let Err(e) = build_rule(&mut builder, production, &notes, schema) {
            refused.push(format!("{}: {e}", production.name));
        }
    }
    if !refused.is_empty() {
        return Err(refused.join("\n"));
    }
    let mut grammar = builder.grammar;
    // EBNF has no empty rule, so a module with nothing at a level names a placeholder.
    // It stands for nothing and must read and print nothing.
    for rule in &mut grammar.rules {
        if rule.name.starts_with("never_") {
            rule.alternatives.clear();
        }
    }
    for rule in &grammar.rules {
        // These validated literals classify presentation omission only. They
        // cannot change tokenization of unrelated rules or proper names.
        if rule.name.starts_with("flavor_word_") {
            continue;
        }
        for alternative in &rule.alternatives {
            for sym in &alternative.syms {
                if let Sym::Word(word) = sym {
                    grammar.vocabulary.insert(word.clone());
                }
            }
        }
    }
    finish(&mut grammar)?;
    grammar.fingerprint = fingerprint(&sources);
    grammar.net = crate::heads::Net::build(&grammar.rules);
    if let Some((_, source)) = syntax.iter().find(|(name, _)| *name == "labels") {
        grammar.presentation_labels = presentation_labels(source)?;
    }
    Ok(grammar)
}

fn build_rule(
    builder: &mut Builder,
    production: &tabnas_ebnf::Production,
    notes: &HashMap<String, Notes>,
    schema: &Schema,
) -> Result<(), String> {
    let id = builder.grammar.names[&production.name] as usize;
    builder.building.clone_from(&production.name);
    builder.anonymous = 0;
    let note = notes.get(&production.name).cloned().unwrap_or_default();
    let (sem, ty) = match &note.sem {
        Some(text) => declared(text, schema)?,
        None => (None, None),
    };
    let mut alternatives = Vec::with_capacity(production.alts.len());
    for sequence in &production.alts {
        alternatives.push(builder.alternative(sequence, sem.as_ref())?);
    }
    if sem.is_none()
        && alternatives
            .iter()
            .any(|a| !matches!(a.syms.as_slice(), [Sym::Rule(_) | Sym::Class(_)]))
    {
        return Err("an alternative without @sem must be one reference".into());
    }
    let rule = &mut builder.grammar.rules[id];
    rule.alternatives = alternatives;
    rule.sem = sem;
    rule.ty = ty;
    rule.prefer = note.prefer;
    Ok(())
}

fn fingerprint(source: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn declared(text: &str, schema: &Schema) -> Result<(Option<Pattern>, Option<Ty>), String> {
    if text == "()" {
        return Ok((Some(Pattern::Value(serde_json::Value::Null)), None));
    }
    let (root, syntax) = pattern::parse(text)?;
    let inferred = match &syntax {
        pattern::Syntax::Symbol(name, _) if name == "retain_animation_types" => {
            Some(Ty::Vec(Box::new(Ty::Named("Change".into()))))
        }
        pattern::Syntax::Symbol(name, _) => crate::functions::keyword_list_type(name),
        _ => None,
    };
    let (ty, legacy) = match root {
        Some(root) => pattern::root_type::declared(&root, schema)?,
        None => (inferred.unwrap_or(Ty::Opaque), false),
    };
    if matches!(ty, Ty::Opaque)
        && !matches!(
            syntax,
            pattern::Syntax::Hole(_) | pattern::Syntax::Symbol(..)
        )
    {
        return Err("a pattern needs a Type:: root".into());
    }
    let ty = match (&syntax, ty, legacy) {
        (pattern::Syntax::List(_), Ty::Named(name), true) => Ty::Vec(Box::new(Ty::Named(name))),
        (_, ty, _) => ty,
    };
    let pattern = pattern::elaborate(&syntax, &ty, schema)?;
    Ok((Some(pattern), Some(ty)))
}

impl Builder {
    fn alternative(
        &mut self,
        sequence: &[Element],
        sem: Option<&Pattern>,
    ) -> Result<Alternative, String> {
        let mut syms = Vec::new();
        let mut holes = Vec::new();
        let mut wanted = Vec::new();
        if let Some(pattern) = sem {
            pattern.holes(&mut wanted);
        }
        let mut seen: HashMap<String, usize> = HashMap::default();
        for element in sequence {
            let (sym, name) =
                if let Some((id, name)) = self.optional_capture(element, &wanted, &seen)? {
                    (vec![Sym::Rule(id)], Some(name))
                } else {
                    self.symbols(element, 0)?
                };
            for sym in sym {
                let hole = name.clone().and_then(|name| {
                    let count = seen.entry(name.clone()).or_insert(0);
                    *count += 1;
                    let label = if *count == 1 {
                        name
                    } else {
                        format!("{name}_{count}")
                    };
                    wanted
                        .iter()
                        .any(|(hole, _)| *hole == label)
                        .then_some(label)
                });
                syms.push(sym);
                holes.push(hole);
            }
        }
        for (hole, _) in &wanted {
            if !holes.iter().flatten().any(|h| h == hole) {
                return Err(format!("hole ${hole} matches nothing in this alternative"));
            }
        }
        Ok(Alternative { syms, holes })
    }

    /// An optional captured reference preserves its value when present. Its
    /// absence is the empty collection or None required by the declared hole,
    /// so both parser and printer use ordinary typed rules in either case.
    fn optional_capture(
        &mut self,
        element: &Element,
        wanted: &[(String, Ty)],
        seen: &HashMap<String, usize>,
    ) -> Result<Option<(RuleId, String)>, String> {
        let Kind::Opt { inner } = &element.kind else {
            return Ok(None);
        };
        let Kind::Ref { name, .. } = &inner.kind else {
            return Ok(None);
        };
        let name = if class_of(name).is_some() {
            name.to_lowercase()
        } else {
            name.clone()
        };
        let count = seen.get(&name).copied().unwrap_or(0) + 1;
        let label = if count == 1 {
            name.clone()
        } else {
            format!("{name}_{count}")
        };
        let Some((_, ty)) = wanted.iter().find(|(hole, _)| *hole == label) else {
            return Ok(None);
        };
        let absent = match ty {
            Ty::Vec(_) => serde_json::Value::Array(Vec::new()),
            Ty::Option(_) => serde_json::Value::Null,
            _ => {
                return Err(format!(
                    "optional ${label} needs a collection or Option type"
                ));
            }
        };
        let body = self.flatten(inner, 1)?;
        let empty = self.push_anonymous(vec![Vec::new()]);
        self.grammar.rules[empty as usize].sem = Some(Pattern::Value(absent));
        self.grammar.rules[empty as usize].ty = Some(ty.clone());
        let optional = self.push_anonymous(vec![vec![Sym::Rule(empty)], body]);
        self.grammar.rules[optional as usize].sem = None;
        self.grammar.rules[optional as usize].ty = Some(ty.clone());
        Ok(Some((optional, name)))
    }

    /// The symbols for one element, and the name a hole would use for it.
    fn symbols(
        &mut self,
        element: &Element,
        depth: usize,
    ) -> Result<(Vec<Sym>, Option<String>), String> {
        if depth > 32 {
            return Err("group nesting limit".into());
        }
        match &element.kind {
            Kind::Term { literal, .. } => Ok((terminal(literal)?, None)),
            Kind::Ref { name, .. } => {
                if let Some(class) = class_of(name) {
                    return Ok((vec![Sym::Class(class)], Some(name.to_lowercase())));
                }
                if !self.production_names.contains(name) {
                    return Err(format!("unknown rule {name}"));
                }
                let id = *self
                    .grammar
                    .names
                    .get(name)
                    .ok_or_else(|| format!("{name} is not a syntactic rule"))?;
                Ok((vec![Sym::Rule(id)], Some(name.clone())))
            }
            Kind::Group { alts } => {
                let single = (alts.len() == 1 && alts[0].len() == 1).then(|| &alts[0][0]);
                if let Some(element) = single {
                    return self.symbols(element, depth + 1);
                }
                let id = self.anonymous_rule(alts)?;
                Ok((vec![Sym::Rule(id)], None))
            }
            Kind::Opt { inner } => {
                let body = self.flatten(inner, depth + 1)?;
                let id = self.push_anonymous(vec![Vec::new(), body]);
                Ok((vec![Sym::Rule(id)], None))
            }
            other => Err(format!(
                "unsupported element in a syntactic rule: {other:?}"
            )),
        }
    }

    fn flatten(&mut self, element: &Element, depth: usize) -> Result<Vec<Sym>, String> {
        Ok(self.symbols(element, depth)?.0)
    }

    fn anonymous_rule(&mut self, alts: &[Vec<Element>]) -> Result<RuleId, String> {
        let mut bodies = Vec::with_capacity(alts.len());
        for sequence in alts {
            let mut body = Vec::new();
            for element in sequence {
                body.extend(self.flatten(element, 1)?);
            }
            bodies.push(body);
        }
        Ok(self.push_anonymous(bodies))
    }

    fn push_anonymous(&mut self, bodies: Vec<Vec<Sym>>) -> RuleId {
        self.anonymous += 1;
        let id = self.grammar.rules.len() as RuleId;
        let alternatives = bodies
            .into_iter()
            .map(|syms| Alternative {
                holes: vec![None; syms.len()],
                syms,
            })
            .collect();
        let name = format!("{}#{}", self.building, self.anonymous);
        self.grammar.names.insert(name.clone(), id);
        self.grammar.rules.push(Rule {
            name,
            alternatives,
            sem: Some(Pattern::Value(serde_json::Value::Null)),
            ty: None,
            prefer: Vec::new(),
        });
        id
    }
}

/// A terminal is one token, or a run of marks and words written together for
/// readability, such as `"{T}"`.
fn terminal(literal: &str) -> Result<Vec<Sym>, String> {
    // A hyphen joins words and an apostrophe sits in them; alone they are marks.
    if literal == "-" || literal == "'" {
        return Ok(literal.chars().map(Sym::Mark).collect());
    }
    let mut syms = Vec::new();
    let mut word = String::new();
    for c in literal.chars() {
        if c.is_alphanumeric() || c == '\'' || c == '\u{2019}' || c == '-' {
            word.push(c);
        } else {
            if !word.is_empty() {
                syms.push(Sym::Word(std::mem::take(&mut word)));
            }
            if !c.is_whitespace() {
                syms.push(Sym::Mark(c));
            }
        }
    }
    if !word.is_empty() {
        syms.push(Sym::Word(word));
    }
    if syms.is_empty() {
        return Err("empty terminal".into());
    }
    Ok(syms)
}

/// Which rules can derive nothing, a refusal of left recursion, which the memoized
/// recognizer cannot enter, and the tables that follow from both.
fn finish(grammar: &mut Grammar) -> Result<(), String> {
    let mut nullable = vec![false; grammar.rules.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for (id, rule) in grammar.rules.iter().enumerate() {
            let empty = |alternative: &Alternative| {
                alternative.syms.iter().all(|sym| match sym {
                    Sym::Rule(rule) => nullable[*rule as usize],
                    _ => false,
                })
            };
            if !nullable[id] && rule.alternatives.iter().any(empty) {
                nullable[id] = true;
                changed = true;
            }
        }
    }
    let mut state = vec![0u8; grammar.rules.len()];
    for id in 0..grammar.rules.len() {
        left_recursion(grammar, id, &nullable, &mut state)?;
    }
    grammar.starts = crate::starts::Starts::build(&grammar.rules, &nullable);
    Ok(())
}

fn left_recursion(
    grammar: &Grammar,
    id: usize,
    nullable: &[bool],
    state: &mut [u8],
) -> Result<(), String> {
    match state[id] {
        1 => return Err(format!("left recursion through {}", grammar.rules[id].name)),
        2 => return Ok(()),
        _ => {}
    }
    state[id] = 1;
    for alternative in &grammar.rules[id].alternatives {
        for sym in &alternative.syms {
            let Sym::Rule(rule) = sym else { break };
            left_recursion(grammar, *rule as usize, nullable, state)?;
            if !nullable[*rule as usize] {
                break;
            }
        }
    }
    state[id] = 2;
    Ok(())
}
