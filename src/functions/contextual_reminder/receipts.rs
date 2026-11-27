//! Extract receipts and ownership evidence from one bound program.
use super::evidence::step_operation;
use super::object_notes::{filter_proves, is_controller};
use serde_json::Value;

pub(super) fn instructions(program: &Value) -> Option<&[Value]> {
    let steps = program["instructions"].as_array()?;
    (steps.len() <= 64).then_some(steps)
}

pub(super) fn producer<'a>(steps: &'a [Value], result: &Value) -> Option<&'a Value> {
    steps.iter().find_map(|step| {
        if step["args"]["result"] == *result {
            step_operation(step)
        } else {
            None
        }
    })
}

pub(super) fn subject_query(subject: &Value) -> Option<&Value> {
    let query = &subject["args"];
    (subject["kind"] == "One" && query["kind"] == "Query" && query["args"]["index"] == 0)
        .then(|| &query["args"]["objects"])
}

pub(super) fn nonland(filter: &Value) -> bool {
    filter["kind"] == "Not"
        && filter["args"]["kind"] == "CardType"
        && filter["args"]["args"] == "Land"
}

pub(super) fn own_zone(filter: &Value, zone: &str) -> bool {
    filter["zone"] == zone && is_controller(&filter["players"])
}

pub(super) fn owned_nonland_hand_exile(cost: &Value, index: usize) -> bool {
    let movement = &cost["parts"][index]["Move"];
    let Some([selection]) = movement["selections"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let objects = &selection["objects"];
    movement["destination"]["Exile"].is_object()
        && objects["kind"] == "Zone"
        && own_zone(&objects["args"]["scope"]["Players"], "Hand")
        && filter_proves(&objects["args"]["filter"], &nonland, 0, &mut 256)
}
