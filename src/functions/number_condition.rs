//! Numeric X conditions require an actual choice or an applied definition.
//! Private derivation marks distinguish an explicit where-definition from an
//! unrelated numeric effect with the same native shape; none survive binding.
use crate::model::ir_schema::{Schema, Ty};
use crate::pattern::{Bindings, Pattern, Syntax, UnifyCtx, elaborate};
use serde_json::{Value, json};

const DEFINITION: &str = "$where_x_definition";
const CONDITION: &str = "$number_condition";
const WRITTEN_AMOUNT: &str = "$written_x_amount";

pub(super) fn checked_written_amount(
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Pattern, String> {
    match args {
        [amount] if *ty == Ty::Named("Expr".into()) => Ok(Pattern::Symbol(
            "written_x".into(),
            vec![elaborate(amount, ty, schema)?],
        )),
        _ => Err("@written_x annotates one Expr".into()),
    }
}

pub(crate) fn unify_definition<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
    ctx: Option<UnifyCtx<'p>>,
) -> Vec<Bindings<'p, 'v>> {
    match (name, args) {
        ("where", _) => super::unify_where(args, value, bindings),
        ("where_snapshot", _) => super::value_snapshot::unify(args, value, bindings),
        ("capture_amount", _) => super::capture_amount::unify(args, value, bindings),
        ("written_x", [amount]) => amount.unify_with(value, bindings, ctx),
        _ => Vec::new(),
    }
}

/// The operative expression may have replaced a literal X before a reminder
/// is checked. Keep the spelling witness private to the derivation.
pub(crate) fn written_amount(mut amount: Value) -> Value {
    if let Some(fields) = amount.as_object_mut() {
        fields.insert(WRITTEN_AMOUNT.into(), Value::Bool(true));
    }
    amount
}

pub(super) fn was_written_x(amount: &Value) -> bool {
    amount[WRITTEN_AMOUNT] == true
}

pub(super) fn same_unmarked_amount(amount: &Value, expected: &Value) -> bool {
    amount.as_object().is_some_and(|actual| {
        expected.as_object().is_some_and(|expected| {
            actual.len() == expected.len() + 1
                && expected
                    .iter()
                    .all(|(name, value)| actual.get(name) == Some(value))
        })
    })
}

pub(crate) fn mark_keyword_amounts(body: &mut Value) {
    use crate::traverse::{Visit, rewrite};
    rewrite(body, &mut |node| {
        if matches!(node["kind"].as_str(), Some("Keyword" | "KeywordEach"))
            && let Some((name, action)) = node
                .pointer_mut("/args/action")
                .and_then(Value::as_object_mut)
                .and_then(|actions| actions.iter_mut().next())
            && matches!(
                name.as_str(),
                "Earthbend" | "Amass" | "Bolster" | "Connive" | "Monstrosity"
            )
            && let Some(amount) = action.get_mut("amount")
            && super::is_x(amount)
        {
            *amount = written_amount(amount.take());
        }
        Visit::Children
    });
}

pub(crate) fn define_where(body: &Value, amount: &Value, at: usize) -> Result<Value, String> {
    let mut written = body.clone();
    mark_keyword_amounts(&mut written);
    if super::value_snapshot::required(body, amount)? {
        return super::value_snapshot::snapshot(&written, amount, at).map(definition);
    }
    let substituted = super::substituted(written, amount);
    if unresolved_direct_payment(&substituted) {
        return Err("a where-definition must supply the resolution-time X payment".into());
    }
    Ok(definition(substituted))
}

// The printed definition governs any direct payment in this same program.
// A cost that has a price already, or owns a separate scope, retains it.
fn unresolved_direct_payment(program: &Value) -> bool {
    let instructions = program["instructions"]
        .as_array()
        .or_else(|| program.as_array());
    instructions.is_some_and(|items| {
        items.iter().any(|item| {
            let operation = &item["args"]["operation"];
            item["kind"] == "Effect"
                && operation["kind"] == "PayCost"
                && operation["args"]["cost"]["parts"]
                    .as_array()
                    .is_some_and(|parts| {
                        parts.iter().any(|part| {
                            part["Mana"]
                                .as_array()
                                .is_some_and(|symbols| symbols.iter().any(|symbol| symbol == "X"))
                        })
                    })
        })
    })
}

pub(crate) fn definition(mut value: Value) -> Value {
    let node = if value.get("instructions").is_some() {
        value["instructions"]
            .as_array_mut()
            .and_then(|items| items.first_mut())
    } else if value.is_array() {
        value.as_array_mut().and_then(|items| items.first_mut())
    } else {
        Some(&mut value)
    };
    if let Some(fields) = node.and_then(Value::as_object_mut) {
        fields.insert(DEFINITION.into(), json!(true));
    }
    value
}

pub(crate) fn annotate(mut condition: Value) -> Result<Value, String> {
    super::branch_bounds(&condition)?;
    if condition["kind"] != "Compare" {
        return Err("a numeric condition needs its comparison".into());
    }
    condition[CONDITION] = json!(true);
    Ok(condition)
}

fn boundary(value: &Value) -> bool {
    matches!(
        crate::traverse::boundary(value),
        crate::traverse::Boundary::Ability | crate::traverse::Boundary::Mode
    ) || matches!(
        value["kind"].as_str(),
        Some("CreateDelayedTrigger" | "CreateTrigger" | "ReflexiveTrigger" | "CreateReplacement")
    )
}

fn defined(value: &Value, root: bool) -> bool {
    if !root && boundary(value) {
        return false;
    }
    if value[DEFINITION] == true {
        return true;
    }
    match value {
        Value::Object(fields) => fields.iter().any(|(key, child)| {
            !matches!(key.as_str(), "ability" | "abilities" | "cost") && defined(child, false)
        }),
        Value::Array(items) => items.iter().any(|child| defined(child, false)),
        _ => false,
    }
}

fn announced(value: &Value) -> bool {
    value["kind"] == "AnnouncedX"
        || match value {
            Value::Object(fields) => fields.values().any(announced),
            Value::Array(items) => items.iter().any(announced),
            _ => false,
        }
}

fn casting(value: &Value) -> bool {
    value["kind"] == "Casting"
        || value["kind"] == "Conditional"
            && value["args"]["then"]
                .as_array()
                .is_some_and(|abilities| !abilities.is_empty() && abilities.iter().all(casting))
}

pub(crate) fn resolve(value: &mut Value) -> Result<(), String> {
    super::branch_bounds(value)?;
    region(value, false, false, 0, &mut 65_536)
}

fn region(
    value: &mut Value,
    inherited: bool,
    inherited_definition: bool,
    depth: usize,
    work: &mut usize,
) -> Result<(), String> {
    if depth > 128 || *work == 0 {
        return Err("numeric definition scope bound".into());
    }
    let owned = match value["kind"].as_str() {
        Some("Spell" | "Casting") => true,
        Some("Activated") => crate::bind::payments::declared(&value["args"]["cost"]),
        Some("Triggered" | "Replacement" | "Static") => false,
        _ => inherited || casting(value),
    };
    let has_definition = inherited_definition || defined(value, true);
    walk(value, owned, has_definition, depth, work)
}

fn walk(
    value: &mut Value,
    owned: bool,
    has_definition: bool,
    depth: usize,
    work: &mut usize,
) -> Result<(), String> {
    if depth > 128 || *work == 0 {
        return Err("numeric definition scope bound".into());
    }
    *work -= 1;
    if value[CONDITION] == true && announced(value) && (!owned || has_definition) {
        return Err("numeric X condition needs its own choice or complete where-definition".into());
    }
    match value {
        Value::Object(fields) => {
            fields.remove(DEFINITION);
            fields.remove(CONDITION);
            fields.remove(WRITTEN_AMOUNT);
            for (key, child) in fields {
                if boundary(child) || matches!(key.as_str(), "ability" | "abilities" | "cost") {
                    region(
                        child,
                        if key == "cost" { false } else { owned },
                        if matches!(key.as_str(), "ability" | "abilities" | "cost")
                            || crate::traverse::boundary(child)
                                == crate::traverse::Boundary::Ability
                        {
                            false
                        } else {
                            has_definition
                        },
                        depth + 1,
                        work,
                    )?;
                } else {
                    walk(child, owned, has_definition, depth + 1, work)?;
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                if boundary(child) {
                    let definition = crate::traverse::boundary(child)
                        != crate::traverse::Boundary::Ability
                        && has_definition;
                    region(child, owned, definition, depth + 1, work)?;
                } else {
                    walk(child, owned, has_definition, depth + 1, work)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
