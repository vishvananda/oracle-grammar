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

// At most one player holds the monarch designation. Matching that player's
// current state is the same defender set as reading the current holder.
fn monarch_defender(value: &mut Value, tracing: bool) {
    use serde_json::json;
    let defenders = &value["Attack"]["defenders"]["Players"];
    let args = &defenders["args"];
    let test = &args["condition"];
    let actor = &test["args"]["player"];
    if defenders["kind"] == "Matching"
        && test["kind"] == "PlayerState"
        && test["args"]["state"] == "Monarch"
        && actor["kind"] == "Bound"
        && super::trace::same(&actor["args"], &args["binding"], tracing)
        && super::walk::lossless::<crate::ir::EventSpec>(value, tracing).is_some()
    {
        value["Attack"]["defenders"]["Players"] =
            json!({"kind":"One","args":{"kind":"Holder","args":"Monarch"}});
    }
}

// A created event whose attack match fixes one target supplies that exact
// attacker as its subject. A snapshot grant can retain the captured target.
pub(super) fn fixed_attacker_grant(value: &mut Value, tracing: bool) {
    use serde_json::json;
    let args = &value["args"];
    let attack = &args["event"]["Attack"];
    let attacker = &attack["attackers"];
    let target = &attacker["args"];
    let Some([effect]) = args["body"]["instructions"].as_array().map(Vec::as_slice) else {
        return;
    };
    let grant = &effect["args"]["operation"];
    if attacker["kind"] != "SameObject"
        || target["kind"] != "Target"
        || effect["kind"] != "Effect"
        || grant["kind"] != "CreateContinuousEffect"
        || value["kind"] != "CreateTrigger"
        || grant["args"]["evaluation"] != "Snapshot"
        || !(grant["args"]["affected"]
            == json!({"Objects":{"kind":"One","args":{"kind":"EventSubject"}}})
            || grant["args"]["affected"] == json!({"Objects":{"kind":"One","args":target}}))
        || grant["args"]["changes"].as_array().is_none_or(|changes| {
            changes.is_empty()
                || changes.len() > 8
                || changes.iter().any(|c| c["kind"] != "GrantKeyword")
        })
        || super::walk::lossless::<crate::ir::Operation>(value, tracing).is_none()
    {
        return;
    }
    let target = target.clone();
    monarch_defender(&mut value["args"]["event"], tracing);
    value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["affected"]["Objects"]
        ["args"] = target;
}

/// A face-down permanent's flip event and controller are independent of
/// its front-face type. Preserve all permanent types for that event (708.7).
pub(super) fn controlled_face_up(value: &mut Value, tracing: bool) {
    use serde_json::json;
    let args = &value["Object"];
    let objects = &args["objects"];
    let face_down = json!({"kind":"State","args":"FaceDown"});
    let controlled =
        json!({"kind":"ControlledBy","args":{"kind":"One","args":{"kind":"AbilityController"}}});
    if args["event"] != "TurnsFaceUp"
        || objects["kind"] != "AllOf"
        || !objects["args"].as_array().is_some_and(|parts| {
            parts.len() == 2 && parts.contains(&face_down) && parts.contains(&controlled)
        })
        || super::walk::lossless::<crate::ir::EventSpec>(value, tracing).is_none()
    {
        return;
    }
    let types = [
        "Artifact",
        "Battle",
        "Creature",
        "Enchantment",
        "Land",
        "Planeswalker",
    ]
    .map(|kind| json!({"kind":"CardType","args":kind}));
    value["Object"]["objects"]["args"] =
        json!([face_down,{"kind":"AnyOf","args":types},controlled]);
}

// The two disjoint face-up alternatives describe one object matcher. Rerender
// the combined lexical description while preserving the other event arms.
pub(super) fn join_fixed_face_up(events: &mut Vec<Value>, tracing: bool) {
    use serde_json::json;
    let state = json!({"kind":"State","args":"FaceDown"});
    if events.len() != 3
        || !events.iter().any(|event| {
            let entry = &event["Player"];
            let filter = &entry["objects"];
            entry["event"] == "EntersUnderControl"
                && filter["kind"] == "AllOf"
                && filter["args"]
                    .as_array()
                    .is_some_and(|parts| parts.contains(&state))
        })
    {
        return;
    }
    let source = json!({"kind":"SameObject","args":{"kind":"Source"}});
    let other = json!({"kind":"Not","args":source});
    let controlled =
        json!({"kind":"ControlledBy","args":{"kind":"One","args":{"kind":"AbilityController"}}});
    let mut self_turn = None;
    let mut controlled_turn = None;
    for (index, event) in events.iter().enumerate() {
        let object = &event["Object"];
        let filter = &object["objects"];
        if object["event"] != "TurnsFaceUp" {
            continue;
        }
        if *filter == source && self_turn.replace(index).is_some() {
            return;
        }
        if filter["kind"] == "AllOf"
            && filter["args"].as_array().is_some_and(|parts| {
                parts.len() == 2 && parts.contains(&other) && parts.contains(&controlled)
            })
            && controlled_turn.replace(index).is_some()
        {
            return;
        }
    }
    if let (Some(own), Some(controlled)) = (self_turn, controlled_turn) {
        let mut predicates = [source, events[controlled]["Object"]["objects"].clone()];
        predicates.sort_by_cached_key(|value| super::trace::key(value, tracing));
        events[controlled]["Object"]["objects"] = json!({"kind":"AnyOf","args":predicates});
        events.remove(own);
    }
}
