//! Overload changes "target" in a spell's text to "each" (CR 702.96b): the paid spell
//! acts on every object or player one of its targets could have been, and announces none.
use serde_json::{Value, json};

/// The overloaded spell for an ordinary one: each single object or player target
/// becomes the collection its spec describes, in the zone it would be chosen from.
/// The spell may be bound, announcing its targets, or in the printer's surface form,
/// where each target names its spec in place (`@target`).
pub(crate) fn paid(unpaid: &Value) -> Result<Value, String> {
    let targets = unpaid["args"]["announcement"]["targets"]
        .as_array()
        .ok_or("overload targets")?;
    if targets.len() > 8 {
        return Err("an overloaded spell names at most eight targets".into());
    }
    let zones = targets
        .iter()
        .map(|spec| Ok((spec["slot"].clone(), collection(spec)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let mut paid = unpaid.clone();
    paid["args"]["announcement"]["targets"] = json!([]);
    let mut changed = 0;
    each(&mut paid["args"]["body"], &zones, &mut changed, 0)?;
    if changed == 0 {
        return Err("an overloaded spell names its targets".into());
    }
    if names_target(&paid["args"]["body"]) {
        return Err("an overloaded target is named outside its set".into());
    }
    Ok(paid)
}

/// `target creature you don't control` as `each creature you don't control`, and
/// `target player` as `each player`.
fn collection(spec: &Value) -> Result<Value, String> {
    let one = spec["count"] == json!({"Exactly":{"kind":"Constant","args":1}})
        && spec["distribute"].is_null()
        && spec["chooser"].is_null()
        && spec["constraints"] == json!([]);
    if spec["kind"] == "Player" {
        let any =
            spec["players"] == json!({"kind":"All"}) && spec["filter"] == json!({"kind":"Any"});
        return if one && any {
            Ok(json!({"kind":"All"}))
        } else {
            Err("an overloaded player target is any one player".into())
        };
    }
    let object = &spec["kind"]["Object"];
    let zone = match object["zones"].as_array().map(Vec::as_slice) {
        Some([zone]) => zone,
        _ => return Err("an overloaded target is chosen from one zone".into()),
    };
    if !one || !spec["players"].is_null() || !object["kind"].is_string() {
        return Err("an overloaded target is one object".into());
    }
    Ok(json!({"kind":"Zone","args":{
        "scope":{"Shared":zone}, "kind":object["kind"], "filter":spec["filter"]
    }}))
}

fn each(
    value: &mut Value,
    zones: &[(Value, Value)],
    changed: &mut usize,
    depth: usize,
) -> Result<(), String> {
    if depth > 128 {
        return Err("overload depth bound".into());
    }
    let target = &value["args"];
    if value["kind"] == "One" && target["kind"] == "Target" && target["args"]["index"] == 0 {
        let slot = &target["args"]["slot"];
        let zone = if slot["$sym"] == "target" {
            Some(collection(&slot["args"][0])?)
        } else {
            zones
                .iter()
                .find(|(announced, _)| slot == announced)
                .map(|(_, zone)| zone.clone())
        };
        if let Some(zone) = zone {
            *value = zone;
            *changed += 1;
            return Ok(());
        }
    }
    match value {
        Value::Object(fields) => fields
            .values_mut()
            .try_for_each(|v| each(v, zones, changed, depth + 1)),
        Value::Array(items) => items
            .iter_mut()
            .try_for_each(|v| each(v, zones, changed, depth + 1)),
        _ => Ok(()),
    }
}

fn names_target(value: &Value) -> bool {
    value["kind"] == "Target"
        || value["$sym"] == "target"
        || match value {
            Value::Object(fields) => fields.values().any(names_target),
            Value::Array(items) => items.iter().any(names_target),
            _ => false,
        }
}
