//! Numeric X conditions require an actual choice or an applied definition.
//! Private derivation marks distinguish an explicit where-definition from an
//! unrelated numeric effect with the same native shape; none survive binding.
use serde_json::{Value, json};

const DEFINITION: &str = "$where_x_definition";
const CONDITION: &str = "$number_condition";

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
