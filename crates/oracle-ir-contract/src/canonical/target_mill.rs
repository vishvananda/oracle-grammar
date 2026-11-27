//! One announced player supplies both a mill and its private-zone count.
use super::{trace, walk::lossless};
use crate::ir::Operation;
use serde_json::{Value, json};

pub(super) fn normalize(ability: &mut Value, tracing: bool) {
    if !matches!(ability["kind"].as_str(), Some("Activated" | "Triggered"))
        || !ability["args"]["announcement"]["modes"].is_null()
    {
        return;
    }
    let Some([spec]) = ability["args"]["announcement"]["targets"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return;
    };
    if spec["kind"] != "Player" || spec["count"] != json!({"Exactly":{"kind":"Constant","args":1}})
    {
        return;
    }
    let Some(slot) = trace::number(&spec["slot"]) else {
        return;
    };
    let Some([effect]) = ability["args"]["body"]["instructions"]
        .as_array_mut()
        .map(Vec::as_mut_slice)
    else {
        return;
    };
    if effect["kind"] != "Effect" {
        return;
    }
    let operation = &mut effect["args"]["operation"];
    let player = &operation["args"]["players"]["args"];
    if operation["kind"] != "Mill"
        || operation["args"]["players"]["kind"] != "One"
        || player["kind"] != "Target"
        || trace::number(&player["args"]["slot"]) != Some(slot)
        || player["args"]["index"] != 0
        || lossless::<Operation>(operation, tracing).is_none()
    {
        return;
    }
    let target = player.clone();
    let amount = &mut operation["args"]["amount"];
    let count = if amount["kind"] == "Divide"
        && amount["args"]["denominator"] == json!({"kind":"Constant","args":2})
    {
        &mut amount["args"]["numerator"]
    } else {
        amount
    };
    if count["kind"] != "CountObjects" || count["args"]["kind"] != "Zone" {
        return;
    }
    let zone = &mut count["args"]["args"];
    if zone["kind"] != "Card" || zone["filter"] != json!({"kind":"Any"}) {
        return;
    }
    let Some(scope) = zone["scope"].get_mut("Players") else {
        return;
    };
    if matches!(scope["zone"].as_str(), Some("Library" | "Graveyard"))
        && scope["players"] == json!({"kind":"One","args":{"kind":"Prior","args":"That"}})
    {
        scope["players"]["args"] = target;
    }
}
