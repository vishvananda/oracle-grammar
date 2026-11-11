//! Explicit reference introductions and final action-operand shaping.
use super::{Scope, Step, at_path, filters, selections, symbol_name};
use crate::model::ir::{KeywordAction, Operation, PlayerSet};
use serde_json::Value;

pub(super) fn counter_choice(
    node: &Value,
    scope: &mut Scope,
    result: u64,
) -> Result<Value, String> {
    let wrapped = node["args"]
        .get(0)
        .ok_or("a counter choice needs an operation")?;
    let operation = selections::action(wrapped);
    let placements = operation["args"]["placements"]
        .as_array()
        .ok_or("counter placements")?;
    if operation["kind"] != "PutCounters"
        || placements.len() != 1
        || placements[0]["selection"].is_null()
        || !closed(operation)
    {
        return Err("counter choice needs one bound placement with a selection".into());
    }
    let count = &placements[0]["selection"]["count"];
    let one = |value: &Value| value == &serde_json::json!({"kind":"Constant","args":1});
    let single = count.get("AsManyAsCan").is_some_and(one) || count.get("UpTo").is_some_and(one);
    scope.counter_choice = Some((result, single));
    let reference = serde_json::json!({"kind":"Query","args":{
        "objects":{"kind":"Affected","args":result},"index":0
    }});
    scope.objects.push(reference.clone());
    scope.named_object = Some(reference);
    scope.others.clear();
    scope.pronoun = None;
    Ok(wrapped.clone())
}

/// Record the new permanent without selecting a member of an unordered group.
pub(super) fn manifest_result(node: &Value, scope: &mut Scope, result: u64) -> Result<(), String> {
    let wrapped = node["args"]
        .get(0)
        .ok_or("manifest result needs its operation")?;
    let operation = selections::action(wrapped);
    // A single manifest-dread action also yields one new face-down creature.
    // Reuse the result contract only for this exact typed action: other keyword
    // actions and repeated/parameterized forms cannot establish this focus.
    let dread = operation["kind"] == "Keyword";
    if dread {
        let typed: Operation = serde_json::from_value(operation.clone())
            .map_err(|why| format!("manifest-dread result operation: {why}"))?;
        if !matches!(
            typed,
            Operation::Keyword {
                actor: PlayerSet::One(_),
                action: KeywordAction::ManifestDread { times: None },
            }
        ) {
            return Err("manifest result needs one unparameterized manifest-dread action".into());
        }
    } else if operation["kind"] != "Manifest" {
        return Err("manifest result needs a Manifest operation or manifest dread".into());
    }
    let bounded = |count: &Value| {
        count["kind"] == "Constant" && count["args"].as_u64().is_some_and(|n| n <= 1)
    };
    let source = &operation["args"]["source"];
    let cards = &source["Cards"];
    let single_source = bounded(&source["TopOfLibrary"]["count"])
        || cards["kind"] == "One"
        || selections::singular(wrapped, cards);
    let single = dread || (operation["args"]["actor"]["kind"] == "One" && single_source);
    scope.manifest_result = Some((result, single));
    let reference = serde_json::json!({"kind":"Result","args":{
        "result":result,"phase":"After","index":0
    }});
    scope.objects.push(reference.clone());
    scope.named_object = Some(reference);
    scope.others.clear();
    scope.pronoun = None;
    Ok(())
}

pub(super) fn counter_reference(scope: &Scope, single: bool) -> Result<Value, String> {
    let reference = scope.object().ok_or("no recent counter recipient")?;
    if reference["kind"] != "Query" || reference["args"]["objects"]["kind"] != "Affected" {
        return Err("no recent counter recipient".into());
    }
    let result = reference["args"]["objects"]["args"].as_u64();
    if scope.counter_choice.map(|(id, _)| id) != result {
        return Err("no recorded counter choice supplies this reference".into());
    }
    if single
        && scope.counter_choice
            != reference["args"]["objects"]["args"]
                .as_u64()
                .map(|id| (id, true))
    {
        return Err("the counter choice does not establish a singular recipient".into());
    }
    Ok(reference["args"]["objects"].clone())
}

pub(super) fn check_scalar_reference(
    name: &str,
    value: &Value,
    scope: &Scope,
) -> Result<(), String> {
    if let Some((result, false)) = scope.manifest_result
        && value["kind"] == "Result"
        && value["args"]["result"] == result
        && value["args"]["phase"] == "After"
    {
        return Err("a plural manifest/cloak result is not one object".into());
    }
    if matches!(
        name,
        "it" | "current_it"
            | "that_object"
            | "named_object"
            | "current_named_object"
            | "defined_it"
            | "defined_object"
            | "defined_named_object"
    ) && let Some((result, false)) = scope.counter_choice
        && value["kind"] == "Query"
        && value["args"]["objects"] == serde_json::json!({"kind":"Affected","args":result})
    {
        return Err("a multiple-recipient counter choice is not one object".into());
    }
    Ok(())
}

fn closed(value: &Value) -> bool {
    if let Some(symbol) = symbol_name(value) {
        return symbol == "selection"
            && value["args"]
                .as_array()
                .is_some_and(|args| args.iter().all(closed));
    }
    match value {
        Value::Object(fields) => fields.values().all(closed),
        Value::Array(items) => items.iter().all(closed),
        _ => true,
    }
}

pub(super) fn remember(
    node: &Value,
    scope: &mut Scope,
    focus: bool,
    path: &[Step],
    introductions: &[(Vec<Step>, usize)],
) -> Result<Value, String> {
    let value = node["args"]
        .get(0)
        .ok_or("a remembered object needs its value")?;
    let mut projected = value.clone();
    let mut operand = path.to_vec();
    operand.extend([Step::Key("args".into()), Step::Index(0)]);
    let mut deferred = false;
    for (target, slot) in introductions {
        if let Some(relative) = target.strip_prefix(operand.as_slice())
            && let Some(value) = at_path(&mut projected, relative)
        {
            *value = serde_json::json!(scope.target_base + slot);
            deferred = true;
        }
    }
    if deferred {
        filters::strip(&mut projected);
    }
    if !closed(&projected) {
        return Err("a remembered object must already be bound".into());
    }
    if focus {
        scope.objects.push(projected.clone());
        scope.named_object = Some(projected);
        scope.others.clear();
        scope.pronoun = None;
    }
    // Keep introduction paths stable until finish_scope moves their specs.
    Ok(if deferred {
        node.clone()
    } else {
        value.clone()
    })
}

pub(super) fn resolve_shapes(value: &mut Value) -> Result<(), String> {
    match value {
        Value::Object(fields) => {
            for child in fields.values_mut() {
                resolve_shapes(child)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                resolve_shapes(child)?;
            }
        }
        _ => {}
    }
    if symbol_name(value) == Some("zone_members") {
        let object = value["args"]
            .get(0)
            .ok_or("a zone member needs its object")?;
        if symbol_name(object) == Some("exile_followup") {
            let inner = object["args"]
                .get(0)
                .ok_or("exile followup needs one ObjectSet")?;
            if !closed(inner) {
                return Err("a zone member contains an unbound object".into());
            }
            let mut marker = object.clone();
            marker["args"][0] = crate::functions::zone_members(inner);
            *value = marker;
        } else {
            if !closed(object) {
                return Err("a zone member contains an unbound object".into());
            }
            *value = crate::functions::zone_members(object);
        }
    }
    Ok(())
}
