//! An unqualified graveyard introduces no player; a lexical controller may.
//! Evidence is evaluated before binding and never duplicates target producers.
use super::{Scope, controller_role, presentation, symbol_name};
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

fn synthetic_owner() -> Value {
    json!({"kind":"OwnedBy","args":{"kind":"All"}})
}
fn owns(value: &Value) -> bool {
    if value["kind"] == "OwnedBy" {
        return true;
    }
    match value {
        Value::Object(fields) => fields.values().any(owns),
        Value::Array(items) => items.iter().any(owns),
        _ => false,
    }
}
fn proven(filter: &Value) -> Result<bool, String> {
    crate::functions::branch_bounds(filter)?;
    fn markers(value: &Value) -> bool {
        if super::symbol_name(value) == Some("indefinite_opponent_controller")
            && value["args"].as_array().is_none_or(|args| args.len() != 1)
        {
            return false;
        }
        match value {
            Value::Object(fields) => fields.values().all(markers),
            Value::Array(items) => items.iter().all(markers),
            _ => true,
        }
    }
    if !markers(filter) {
        return Err("departure controller filter annotation arity".into());
    }
    Ok(!owns(filter) && controller_role::proven(filter, &mut 16_384, 0)?)
}
fn native(filter: &Value) -> Value {
    json!({"ZoneChange":{"objects":crate::functions::conjunction(vec![filter.clone(), synthetic_owner()]),"from":"Battlefield","to":"Graveyard"}})
}
/// Both arguments are evaluated without context binding. Only the native event
/// survives, so auxiliary object/target symbols are never introduced twice.
pub(crate) fn annotate(event: &Value, filter: &Value, at: usize) -> Result<Value, String> {
    crate::functions::branch_bounds(event)?;
    crate::functions::branch_bounds(filter)?;
    if *event != native(filter) {
        return Err("departure controller event/filter mismatch".into());
    }
    if proven(filter)? {
        Ok(json!({"$sym":"departure_controller_event","args":[event],"at":at}))
    } else {
        Ok(event.clone())
    }
}
fn original(event: &Value) -> Option<Value> {
    let z = event.get("ZoneChange")?;
    if z["from"] != "Battlefield" || z["to"] != "Graveyard" {
        return None;
    }
    let filter = &z["objects"];
    let parts = if filter["kind"] == "AllOf" {
        filter["args"].as_array()?.clone()
    } else {
        vec![filter.clone()]
    };
    let mut kept = Vec::new();
    let mut removed = false;
    for part in parts {
        if !removed && part == synthetic_owner() {
            removed = true;
        } else {
            kept.push(part);
        }
    }
    removed.then(|| crate::functions::conjunction(kept))
}
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [event, filter] = args else {
        return Vec::new();
    };
    if crate::functions::branch_bounds(value).is_err() {
        return Vec::new();
    }
    let Some(original) = original(value) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for bound in event.unify(value, bindings) {
        out.extend(filter.unify_made(&original, &bound));
    }
    out
}
/// Strip only event constructors; never descend into a predicate or ability.
fn take_event(event: &mut Value, depth: usize, work: &mut usize) -> Result<bool, String> {
    if depth > 64 || *work == 0 {
        return Err("departure event provenance bound".into());
    }
    *work -= 1;
    if symbol_name(event) == Some("departure_controller_event") {
        let args = event["args"]
            .as_array_mut()
            .ok_or("departure event annotation args")?;
        if args.len() != 1 {
            return Err("departure event annotation arity".into());
        }
        let inner = args[0].take();
        let original = original(&inner).ok_or("departure event annotation kind")?;
        if !proven(&original)? {
            return Err("departure event annotation lacks controller proof".into());
        }
        *event = inner;
        return Ok(true);
    }
    if let Some(cases) = event.get_mut("AnyOf").and_then(Value::as_array_mut) {
        if cases.is_empty() || cases.len() > 16 {
            return Err("departure event alternative bound".into());
        }
        let mut all = true;
        for case in cases {
            all &= take_event(case, depth + 1, work)?;
        }
        return Ok(all);
    }
    for key in ["FirstEachTurn", "FirstForObjectEachTurn", "FirstEachGame"] {
        if let Some(inner) = event.get_mut(key) {
            return take_event(inner, depth + 1, work);
        }
    }
    for key in ["Filtered", "NthEachTurn", "Caused", "WhileActivating"] {
        if let Some(inner) = event.get_mut(key).and_then(|v| v.get_mut("event")) {
            return take_event(inner, depth + 1, work);
        }
    }
    // Strip unsupported batch annotations too, but never seed a scalar role.
    if let Some(inner) = event.get_mut("Batch").and_then(|v| v.get_mut("event")) {
        take_event(inner, depth + 1, work)?;
    }
    Ok(false)
}
/// Called at each actual event-owning scope, before target capture/initialization.
pub(super) fn prepare(ability: &mut Value) -> Result<bool, String> {
    let ability = presentation::core_mut(ability);
    if !matches!(
        ability["kind"].as_str(),
        Some("Triggered" | "CreateTrigger")
    ) {
        return Ok(false);
    }
    crate::functions::branch_bounds(ability)?;
    take_event(&mut ability["args"]["event"], 0, &mut 16_384)
}
pub(super) fn seed(scope: &mut Scope, proven: bool) {
    if proven
        && scope.players.last() == Some(&json!({"kind":"OwnerOf","args":{"kind":"EventSubject"}}))
    {
        *scope.players.last_mut().expect("checked player") =
            json!({"kind":"ControllerOf","args":{"kind":"EventSubject"}});
    }
}

#[cfg(test)]
#[path = "departure_controller_tests.rs"]
mod tests;
