//! "A creature destroyed this way can't be regenerated" forbids regenerating each
//! object the sentence before it destroys. The noun names what was destroyed and
//! its number agrees with it: "a creature" for one object, "creatures" for a set.
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

/// Destroy operations in a sentence, bounded like any branch payload.
fn destroys(value: &Value, depth: usize, found: &mut Vec<Vec<String>>, path: &mut Vec<String>) {
    if depth > 128 || found.len() > 8 {
        return;
    }
    if value["kind"] == "Destroy" && value["args"]["regeneration"].is_boolean() {
        found.push(path.clone());
    }
    match value {
        Value::Object(fields) => fields.iter().for_each(|(key, field)| {
            path.push(key.clone());
            destroys(field, depth + 1, found, path);
            path.pop();
        }),
        Value::Array(items) => items.iter().enumerate().for_each(|(index, item)| {
            path.push(index.to_string());
            destroys(item, depth + 1, found, path);
            path.pop();
        }),
        _ => {}
    }
}

fn pointer(path: &[String]) -> String {
    path.iter().map(|key| format!("/{key}")).collect()
}

/// Equal apart from source positions.
fn same(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            let keys = |o: &serde_json::Map<String, Value>| o.keys().filter(|k| *k != "at").count();
            keys(a) == keys(b)
                && a.iter()
                    .filter(|(k, _)| *k != "at")
                    .all(|(k, v)| b.get(k).is_some_and(|w| same(v, w)))
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(v, w)| same(v, w))
        }
        _ => left == right,
    }
}

fn mentions(value: &Value, noun: &Value, depth: usize) -> bool {
    depth <= 128
        && (same(value, noun)
            || match value {
                Value::Object(fields) => fields.values().any(|v| mentions(v, noun, depth + 1)),
                Value::Array(items) => items.iter().any(|v| mentions(v, noun, depth + 1)),
                _ => false,
            })
}

/// One object for "a creature", a set for "creatures". A set may still be a symbol
/// its operand hook fills (`zone_card_members`).
fn agrees(objects: &Value, one: bool) -> bool {
    (objects["kind"] == "One") == one
}

/// Forbid regenerating every object the sentence destroys. Each destroyed object
/// must be the named kind in the stated number; a sentence that destroys nothing,
/// or destroys something else too, is not what "destroyed this way" names.
pub(crate) fn capture(mut value: Value, noun: &Value, one: bool) -> Result<Value, String> {
    super::branch_bounds(&value)?;
    let mut found = Vec::new();
    destroys(&value, 0, &mut found, &mut Vec::new());
    if found.is_empty() || found.len() > 8 {
        return Err("\"destroyed this way\" names what its sentence destroys".into());
    }
    for path in found {
        let destroy = value
            .pointer_mut(&pointer(&path))
            .ok_or("destroyed this way path")?;
        let objects = &destroy["args"]["objects"];
        if destroy["args"]["regeneration"] != true
            || !agrees(objects, one)
            || !mentions(objects, noun, 0)
        {
            return Err("\"destroyed this way\" agrees with what is destroyed".into());
        }
        destroy["args"]["regeneration"] = Value::Bool(false);
    }
    Ok(value)
}

/// Kinds a destroyed set may be named by.
fn nouns(value: &Value, depth: usize, found: &mut Vec<Value>) {
    if depth > 128 || found.len() > 8 {
        return;
    }
    if matches!(value["kind"].as_str(), Some("CardType" | "Subtype"))
        && !found.iter().any(|seen| same(seen, value))
    {
        found.push(value.clone());
    }
    match value {
        Value::Object(fields) => fields.values().for_each(|v| nouns(v, depth + 1, found)),
        Value::Array(items) => items.iter().for_each(|v| nouns(v, depth + 1, found)),
        _ => {}
    }
}

/// Card types a destroyed reference may be named by when its value has no noun.
const REFERENCE_KINDS: [&str; 5] = [
    "Creature",
    "Artifact",
    "Enchantment",
    "Land",
    "Planeswalker",
];

/// The inverse restores regeneration on each destroyed object and offers each kind
/// every destroyed object shares as the noun; reparse verifies the projection.
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
    one: bool,
) -> Vec<Bindings<'p, 'v>> {
    let [clause, noun] = args else {
        return Vec::new();
    };
    if super::branch_bounds(value).is_err() {
        return Vec::new();
    }
    let mut found = Vec::new();
    destroys(value, 0, &mut found, &mut Vec::new());
    if found.is_empty() || found.len() > 8 {
        return Vec::new();
    }
    let mut projected = value.clone();
    let mut kinds: Option<Vec<Value>> = None;
    for path in &found {
        let Some(destroy) = projected.pointer_mut(&pointer(path)) else {
            return Vec::new();
        };
        let objects = &destroy["args"]["objects"];
        if destroy["args"]["regeneration"] != false || !agrees(objects, one) {
            return Vec::new();
        }
        let mut named = Vec::new();
        nouns(objects, 0, &mut named);
        kinds = Some(match kinds {
            None => named,
            Some(kinds) => kinds
                .into_iter()
                .filter(|kind| named.iter().any(|n| same(n, kind)))
                .collect(),
        });
        destroy["args"]["regeneration"] = Value::Bool(true);
    }
    // A destroyed reference ("destroy that creature") carries no noun of its own;
    // offer each card type and let reparse keep the one the reference names.
    let kinds = match kinds {
        Some(kinds) if !kinds.is_empty() => kinds,
        _ => REFERENCE_KINDS
            .iter()
            .map(|kind| serde_json::json!({"kind": "CardType", "args": kind}))
            .collect(),
    };
    let mut out = Vec::new();
    for kind in kinds {
        for bound in clause.unify_made(&projected, &bindings) {
            out.extend(noun.unify_made(&kind, &bound));
        }
    }
    out
}
