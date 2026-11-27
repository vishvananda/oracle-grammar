//! A singular description of the attached permanent keeps the Aura's host.
use super::{Attachment, symbol_name};
use serde_json::{Value, json};

fn named_enchanted_set(set: &Value, subtype: &str) -> bool {
    let set = if symbol_name(set) == Some("distributive_pump_subject") {
        &set["args"][0]
    } else {
        set
    };
    let zone = &set["args"];
    let filter = &zone["filter"];
    set["kind"] == "Zone"
        && zone["scope"]["Shared"] == "Battlefield"
        && zone["kind"] == "Permanent"
        && filter["kind"] == "AllOf"
        && filter["args"].as_array().is_some_and(|parts| {
            parts.len() == 2
                && parts.contains(&json!({"kind":"Subtype","args":subtype}))
                && parts.contains(&json!({"kind":"State","args":"Enchanted"}))
        })
}

/// Unqualified "enchanted Equipment" and "enchanted <basic land type>"
/// in these single-host instructions denote this Aura's actual attachment.
pub(super) fn global_host(ability: &Value, attachment: Attachment) -> bool {
    if !matches!(attachment, Attachment::Aura | Attachment::OtherAura) {
        return false;
    }
    let args = &ability["args"];
    if ability["kind"] == "Static" {
        return named_enchanted_set(&args["affected"]["Objects"], "Equipment")
            && args["changes"]
                == json!([{"kind":"SetController","args":{"kind":"AbilityController"}}]);
    }
    if ability["kind"] != "Activated" {
        return false;
    }
    let Some([effect]) = args["body"]["instructions"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let operation = &effect["args"]["operation"];
    let operands = &operation["args"];
    effect["kind"] == "Effect"
        && operation["kind"] == "CreateContinuousEffect"
        && operands["duration"] == "EndOfTurn"
        && operands["evaluation"] == "Snapshot"
        && ["Plains", "Island", "Swamp", "Mountain", "Forest"]
            .iter()
            .any(|subtype| named_enchanted_set(&operands["affected"]["Objects"], subtype))
        && operands["changes"].as_array().is_some_and(|changes| {
            changes.len() <= 8
                && changes.iter().any(|c| c["kind"] == "SetBasePowerToughness")
                && changes
                    .iter()
                    .any(|c| c["kind"] == "AddCardType" && c["args"] == "Creature")
        })
}

/// The damage source in this death trigger is the creature enchanted at
/// trigger time. Putting a counter on that creature uses the captured host.
pub(super) fn uncaptured_damage_source(ability: &Value, attachment: Attachment) -> bool {
    if attachment != Attachment::Aura || ability["kind"] != "Triggered" {
        return false;
    }
    let args = &ability["args"];
    let event = &args["event"]["ZoneChange"];
    let objects = &event["objects"];
    let Some([effect]) = args["body"]["instructions"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let operation = &effect["args"]["operation"];
    let Some([placement]) = operation["args"]["placements"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return false;
    };
    let recipients = &placement["recipients"];
    let mut reference = &recipients["args"];
    for _ in 0..4 {
        reference = match symbol_name(reference) {
            Some("qualified_members" | "remember_operand") => &reference["args"][0],
            _ if reference["kind"] == "One" => &reference["args"],
            _ => break,
        };
    }
    let relative = symbol_name(reference) == Some("counter_reference_single")
        || symbol_name(reference) == Some("qualified_object")
            && reference["args"] == json!(["Permanent",{"kind":"CardType","args":"Creature"}]);
    if !relative
        || effect["kind"] != "Effect"
        || operation["kind"] != "PutCounters"
        || recipients["kind"] != "Objects"
        || objects["kind"] != "AllOf"
        || event["from"] != "Battlefield"
        || event["to"] != "Graveyard"
    {
        return false;
    }
    objects["args"].as_array().is_some_and(|parts| {
        parts.len() == 2
            && parts.iter().any(|part| {
                let history = &part["args"]["args"]["left"];
                let damage = &history["args"]["event"]["Damage"];
                let source = &damage["sources"]["args"];
                part["kind"] == "Condition"
                    && history["kind"] == "EventHistory"
                    && damage["sources"]["kind"] == "SameObject"
                    && symbol_name(source) == Some("enchanted_operand")
                    && source["args"][0] == json!({"kind":"AttachedTo","args":{"kind":"Source"}})
                    && damage["recipients"]
                        == json!({"Objects":{"kind":"SameObject","args":{"kind":"Candidate"}}})
            })
    })
}
