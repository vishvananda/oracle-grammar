//! Simultaneous equal damage has one amount for disjoint recipient domains.
use serde_json::{Value, json};

fn same_snapshot(value: &Value) -> bool {
    if value["kind"] == "Constant" {
        return value["args"].is_i64();
    }
    let population = &value["args"];
    let zone = &population["args"];
    value["kind"] == "CountObjects"
        && population["kind"] == "Zone"
        && zone["scope"]["Shared"] == "Battlefield"
        && zone["kind"] == "Permanent"
        && matches!(
            zone["filter"]["kind"].as_str(),
            Some("Subtype" | "CardType")
        )
        && zone["filter"]["args"].is_string()
}

pub(super) fn equal_amounts(value: &mut Value) {
    let args = &value["args"];
    let Some([first, second]) = args["parts"].as_array().map(Vec::as_slice) else {
        return;
    };
    let amount = &first["amount"];
    if amount != &second["amount"] || !same_snapshot(amount) {
        return;
    }
    let (objects, players) = match (
        first["recipients"]["kind"].as_str(),
        second["recipients"]["kind"].as_str(),
    ) {
        (Some("Objects"), Some("Players")) => (&first["recipients"], &second["recipients"]),
        (Some("Players"), Some("Objects")) => (&second["recipients"], &first["recipients"]),
        _ => return,
    };
    if players["args"]["kind"] != "One" || players["args"]["args"]["kind"] != "AbilityController" {
        return;
    }
    *value = json!({"kind":"DealDamage","args":{
        "source":args["source"], "amount":amount, "preventable":args["preventable"],
        "recipients":{"kind":"Union","args":[objects,players]}
    }});
}
