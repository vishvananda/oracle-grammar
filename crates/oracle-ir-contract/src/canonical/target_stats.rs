//! A snapshot's single object is also its affected-object property owner.
use super::{trace, walk::lossless};
use crate::ir::Operation;
use serde_json::{Value, json};

const STATS: [&str; 2] = ["power", "toughness"];

fn snapshot(operation: &Value) -> Option<(&Value, &[Value])> {
    let args = &operation["args"];
    let objects = &args["affected"]["Objects"];
    if operation["kind"] != "CreateContinuousEffect"
        || args["evaluation"] != "Snapshot"
        || args["duration"] != "EndOfTurn"
        || objects["kind"] != "One"
        || !matches!(
            objects["args"]["kind"].as_str(),
            Some("Target" | "EventSubject")
        )
    {
        return None;
    }
    let changes = args["changes"].as_array().filter(|changes| {
        (1..=4).contains(&changes.len())
            && changes
                .iter()
                .all(|change| matches!(change["kind"].as_str(), Some("Pump" | "GrantKeyword")))
    })?;
    Some((&objects["args"], changes))
}

fn scalar_owner(expr: &Value) -> Option<&Value> {
    let args = &expr["args"];
    (expr["kind"] == "ObjectProperty"
        && args["information"] == "CurrentOrLastKnown"
        && matches!(
            args["property"].as_str(),
            Some("Power" | "Toughness" | "ManaValue")
        ))
    .then_some(&args["object"])
}

/// Signed reductions of a single stat use the same snapshot as a direct stat.
fn property_node(expr: &Value) -> Option<&Value> {
    if scalar_owner(expr).is_some() {
        return Some(expr);
    }
    let [sign, difference] = expr["args"].as_array()?.as_slice() else {
        return None;
    };
    if expr["kind"] != "Multiply"
        || *sign != json!({"kind":"Constant","args":-1})
        || difference["kind"] != "Subtract"
    {
        return None;
    }
    let [property, offset] = difference["args"].as_array()?.as_slice() else {
        return None;
    };
    (offset["kind"] == "Constant" && scalar_owner(property).is_some()).then_some(property)
}

fn matches_owner(expr: &Value, owner: &Value, tracing: bool) -> bool {
    property_node(expr)
        .and_then(scalar_owner)
        .is_some_and(|read| trace::same(read, owner, tracing))
}

fn replace_owner(expr: &mut Value, owner: &Value) {
    let property = if expr["kind"] == "ObjectProperty" {
        expr
    } else {
        &mut expr["args"][1]["args"][0]
    };
    property["args"]["object"] = owner.clone();
}

fn changes_owner(changes: &[Value], owner: &Value, tracing: bool) -> bool {
    changes.iter().any(|change| {
        change["kind"] == "Pump"
            && STATS
                .iter()
                .any(|stat| matches_owner(&change["args"][stat], owner, tracing))
    })
}

pub(super) fn may_print(operation: &Value) -> bool {
    snapshot(operation).is_some_and(|(target, changes)| changes_owner(changes, target, false))
}

fn rewrite(operation: &mut Value, affected: bool, tracing: bool) -> bool {
    let Some((target, changes)) = snapshot(operation) else {
        return false;
    };
    let implicit = json!({"kind":"Affected"});
    let (from, to) = if affected {
        (target, &implicit)
    } else {
        (&implicit, target)
    };
    if !changes_owner(changes, from, tracing) || lossless::<Operation>(operation, tracing).is_none()
    {
        return false;
    }
    let (from, to) = (from.clone(), to.clone());
    let changes = operation["args"]["changes"]
        .as_array_mut()
        .expect("checked snapshot changes");
    for change in changes.iter_mut().filter(|c| c["kind"] == "Pump") {
        for stat in STATS {
            let expr = &mut change["args"][stat];
            if matches_owner(expr, &from, tracing) {
                replace_owner(expr, &to);
            }
        }
    }
    true
}

pub(super) fn normalize(operation: &mut Value, tracing: bool) {
    rewrite(operation, false, tracing);
}

pub(super) fn print(operation: &mut Value) -> bool {
    rewrite(operation, true, false)
}
