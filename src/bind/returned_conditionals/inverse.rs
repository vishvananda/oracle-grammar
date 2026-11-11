//! Only the exact two frozen partitions and flat constant actions project to If.
use super::operands::{group, retarget, scalar};
use crate::model::ir::{Instruction, TargetSpec};
use serde_json::{Value, json};
use std::collections::BTreeSet;
fn note(value: &Value) -> Option<(u32, &Value)> {
    if value["kind"] != "Effect" || value["args"]["operation"]["kind"] != "NoteObjects" {
        return None;
    }
    Some((
        u32::try_from(value["args"]["result"].as_u64()?).ok()?,
        &value["args"]["operation"]["args"]["objects"],
    ))
}
// A repeated MAX action definition is the canonical unused-result marker only
// when this exact recipe never observes MAX anywhere outside a definition.
// This conservative inverse guard does not restrict native ResultId's range.
fn max_outside_definitions(items: &[Value]) -> bool {
    let mut pending: Vec<&Value> = items.iter().collect();
    while let Some(value) = pending.pop() {
        if value["kind"] == "Effect" {
            if let Some(args) = value["args"].as_object() {
                pending.extend(
                    args.iter()
                        .filter(|(key, _)| key.as_str() != "result")
                        .map(|(_, value)| value),
                );
            }
            continue;
        }
        match value {
            Value::Number(number) if number.as_u64() == Some(u64::from(u32::MAX)) => return true,
            Value::Object(fields) => pending.extend(fields.values()),
            Value::Array(values) => pending.extend(values),
            _ => {}
        }
    }
    false
}
fn projected(value: &Value) -> Option<Value> {
    let mut items = value.as_array()?.as_slice();
    if let [only] = items
        && only["kind"] == "Group"
    {
        items = only["args"]["instructions"].as_array()?.as_slice();
    }
    if items.len() < 2 || items.len() > 8194 {
        return None;
    }
    let (yes_id, matching) = note(items.first()?)?;
    if matching["kind"] != "Matching"
        || matching["args"]["objects"]["kind"] != "Transition"
        || matching["args"]["objects"]["args"]["phase"] != "After"
    {
        return None;
    }
    let result = u32::try_from(matching["args"]["objects"]["args"]["result"].as_u64()?).ok()?;
    let mut start = 1;
    let yes = json!({"kind":"Selected","args":yes_id});
    let no = if let Some((id, difference)) = items.get(1).and_then(note) {
        if id == yes_id
            || difference
                != &json!({"kind":"Difference","args":{"objects":group(result),"excluding":yes}})
        {
            return None;
        }
        start = 2;
        Some(json!({"kind":"Selected","args":id}))
    } else {
        None
    };
    let one = json!({"kind":"One","args":scalar(result)});
    let mut yes_body = Vec::new();
    let mut no_body = Vec::new();
    let mut in_no = false;
    let mut definitions = BTreeSet::new();
    let observed_max = yes_id == u32::MAX
        || no.as_ref().is_some_and(|no| no["args"] == u32::MAX)
        || max_outside_definitions(items);
    for (index, i) in items.iter().enumerate() {
        let _: Instruction = serde_json::from_value(i.clone()).ok()?;
        if i["kind"] != "Effect" {
            return None;
        }
        let definition = u32::try_from(i["args"]["result"].as_u64()?).ok()?;
        if index >= start && definition == u32::MAX && !observed_max {
            continue;
        }
        if !definitions.insert(definition) {
            return None;
        }
    }
    for i in &items[start..] {
        let op = &i["args"]["operation"];
        if !in_no && let Some(operation) = retarget(op, &yes, &one) {
            let mut i = i.clone();
            i["args"]["operation"] = operation;
            yes_body.push(i);
            continue;
        }
        let no = no.as_ref()?;
        let operation = retarget(op, no, &one)?;
        in_no = true;
        let mut i = i.clone();
        i["args"]["operation"] = operation;
        no_body.push(i);
    }
    if yes_body.is_empty() || no.is_some() && no_body.is_empty() {
        return None;
    }
    Some(
        json!([{"kind":"If","args":{"condition":{"kind":"ObjectMatches","args":{"object":scalar(result),"filter":matching["args"]["filter"]}},"then":{"instructions":yes_body},"otherwise":{"instructions":no_body}}}]),
    )
}
pub(super) fn project(value: &Value) -> Option<Value> {
    crate::functions::branch_bounds(value).ok()?;
    projected(value)
}

/// Unbinding restores the lexical target introduction before inverse matching.
/// Only the private proof copy uses its native slot; the emitted projection
/// retains every target-spec field and introduction marker for grammar matching.
fn proof_producer(value: &Value) -> Option<Value> {
    crate::functions::branch_bounds(value).ok()?;
    let mut producer = value.clone();
    if producer["kind"] != "Effect" {
        return None;
    }
    let operation = &mut producer["args"]["operation"];
    let objects = &mut operation["args"]["objects"];
    if objects["kind"] != "One" || objects["args"]["kind"] != "Target" {
        return Some(producer);
    }
    let slot = &mut objects["args"]["args"]["slot"];
    if slot["$sym"] == "target" {
        let [spec] = slot["args"].as_array()?.as_slice() else {
            return None;
        };
        crate::functions::branch_bounds(spec).ok()?;
        let spec: TargetSpec = serde_json::from_value(spec.clone()).ok()?;
        *slot = serde_json::to_value(spec.slot).ok()?;
    }
    Some(producer)
}

/// The one fixed EBNF template owns its entire producer/return/If vector.
/// No arbitrary prefix is inferred or searched during inverse matching.
pub(super) fn project_with_prefix(value: &Value, literal: bool) -> Option<Value> {
    if !literal {
        return project(value);
    }
    crate::functions::branch_bounds(value).ok()?;
    let items = value.as_array()?;
    if items.len() < 4 || items.len() > 8196 {
        return None;
    }
    let mut proof = super::Context::default();
    proof.record(&proof_producer(&items[0])?).ok()?;
    proof.record(&items[1]).ok()?;
    let (returned, _) = proof.current_return()?;
    let suffix = Value::Array(items[2..].to_vec());
    let projected = project(&suffix)?;
    if projected[0]["args"]["condition"]["args"]["object"] != scalar(returned) {
        return None;
    }
    let mut output = items[..2].to_vec();
    output.extend(projected.as_array()?.iter().cloned());
    Some(Value::Array(output))
}
