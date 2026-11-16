//! A historical death is not a new name for the ability holder.
use super::{Step, symbol_name};
use serde_json::Value;

pub(super) fn unproved_source(root: &Value, path: &[Step]) -> bool {
    if root["kind"] != "Triggered" || path.len() < 9 {
        return false;
    }
    let prefix = ["args", "body", "instructions"];
    if !path[..3]
        .iter()
        .zip(prefix)
        .all(|(step, name)| matches!(step, Step::Key(key) if key == name))
        || path[3] != Step::Index(0)
    {
        return false;
    }
    if !path[4..8]
        .iter()
        .zip(["args", "operation", "args", "objects"])
        .all(|(step, name)| matches!(step, Step::Key(key) if key == name))
    {
        return false;
    }
    let first = &root["args"]["body"]["instructions"][0];
    let operation = &first["args"]["operation"];
    if first["kind"] != "Effect"
        || operation["kind"] != "Move"
        || !super::revealed_card::scalar_suffix(&path[8..], &operation["args"]["objects"])
    {
        return false;
    }
    let mut reference = root;
    for step in path {
        let next = match step {
            Step::Key(key) => reference.get(key),
            Step::Index(index) => reference.get(*index),
        };
        let Some(next) = next else { return false };
        reference = next;
    }
    if symbol_name(reference) != Some("current_named_object") {
        return false;
    }
    let condition = &root["args"]["intervening_if"];
    let history = &condition["args"]["left"];
    condition["kind"] == "Compare"
        && history["kind"] == "EventHistory"
        && history["args"]["event"]["ZoneChange"]["from"] == "Battlefield"
        && history["args"]["event"]["ZoneChange"]["to"] == "Graveyard"
}
