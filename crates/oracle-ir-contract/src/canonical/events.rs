//! Reference frames of independently matched created events.
use serde_json::Value;

pub(super) struct Frame {
    pub subject: super::Subject,
    pub damage: Option<Value>,
}

pub(super) fn subject(event: &Value) -> super::Subject {
    if super::source_events::names_source(event) {
        super::Subject::Source
    } else {
        super::Subject::Other
    }
}

pub(super) fn created(owner: &str, args: &Value) -> Option<Frame> {
    if matches!(
        owner,
        "EntryState::ExiledForWith" | "ExileState::ExiledForWith"
    ) {
        let event = serde_json::to_value(crate::semantics::entry_return_event())
            .expect("typed returning entry event");
        return Some(Frame {
            subject: subject(&event),
            damage: None,
        });
    }
    matches!(owner, "Operation::CreateTrigger" | "Operation::CreateBoon").then(|| Frame {
        subject: subject(&args["event"]),
        damage: super::damage_subject(&args["event"]),
    })
}

// A combat-damage assignment has a battlefield creature as its source (CR
// 510.1–2). In a prevention matcher its live battlefield membership adds no
// test. Other damage can have a departed source and retains its zone filter.
pub(super) fn combat_source(value: &mut Value, tracing: bool) {
    use crate::ir::ReplacementEvent;
    let damage = &value["Damage"];
    let sources = &damage["sources"];
    let set = &sources["args"];
    let zone = &set["args"];
    if damage["source_kind"] == "Permanent"
        && damage["kind"] == "Combat"
        && sources["kind"] == "InSet"
        && set["kind"] == "Zone"
        && zone["scope"]["Shared"] == "Battlefield"
        && zone["kind"] == "Permanent"
        && super::walk::lossless::<ReplacementEvent>(value, tracing).is_some()
    {
        value["Damage"]["sources"] = zone["filter"].clone();
    }
}

// A boon about a creature the beneficiary controls entering the battlefield
// supplies that entrant to a switch of its two characteristics. The alternate
// EntersUnderControl header adds no player or transition operand to the body.
pub(super) fn boon_entry_switch(value: &mut Value, tracing: bool) {
    use crate::ir::Operation;
    use serde_json::json;
    let args = &value["args"];
    let event = &args["event"]["Player"];
    let objects = &event["objects"];
    let program = &args["body"];
    let op = &program["args"]["instructions"][0]["args"]["operation"];
    if event["event"] != "EntersUnderControl"
        || event["players"] != json!({"kind":"All"})
        || objects["kind"] != "AllOf"
        || !objects["args"].as_array().is_some_and(|parts| {
            parts.len()==2 && parts.contains(&json!({"kind":"CardType","args":"Creature"}))
                && parts.contains(&json!({"kind":"ControlledBy","args":{"kind":"One","args":{"kind":"AbilityController"}}}))
        })
        || program["kind"] != "Program"
        || program["args"]["instructions"].as_array().map(Vec::len) != Some(1)
        || op["kind"] != "CreateContinuousEffect"
        || op["args"]["changes"] != json!([{"kind":"SwitchPowerToughness"}])
        || op["args"]["affected"] != json!({"Objects":{"kind":"One","args":{"kind":"EventSubject"}}})
        || super::walk::lossless::<Operation>(value, tracing).is_none()
    {
        return;
    }
    value["args"]["event"] = json!({"ZoneChange":{
        "objects":objects,"from":null,"to":"Battlefield"
    }});
}
