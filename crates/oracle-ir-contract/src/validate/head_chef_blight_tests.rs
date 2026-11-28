use crate::{ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"[{"kind": "Keyword", "args": {"kind": "Menace"}}, {"kind": "WithPresentation", "args": {"label": null, "reminder": true, "ability": {"kind": "Triggered", "args": {"event": {"ZoneChange": {"objects": {"kind": "SameObject", "args": {"kind": "Source"}}, "from": null, "to": "Battlefield"}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "BecomeHeadChef", "args": {"player": {"kind": "AbilityController"}}}}}]}}}}}, {"kind": "Triggered", "args": {"event": {"Spell": {"event": "Cast", "players": {"kind": "One", "args": {"kind": "AbilityController"}}, "spells": {"kind": "Any"}}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "ForEachPlayer", "args": {"players": {"kind": "OpponentsOf", "args": {"kind": "AbilityController"}}, "binding": 0, "starting_with": null, "in_chosen_direction": false, "body": {"instructions": [{"kind": "Effect", "args": {"result": 0, "operation": {"kind": "Keyword", "args": {"actor": {"kind": "One", "args": {"kind": "Bound", "args": 0}}, "action": {"Blight": {"amount": {"kind": "Constant", "args": 1}}}}}}}, {"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "CreateContinuousEffect", "args": {"source": {"kind": "Source"}, "affected": {"Objects": {"kind": "Blighted", "args": 0}}, "changes": [{"kind": "LosesAbility", "args": {"kind": "Term", "args": {"family": "CheckHasable", "form": "Flying", "operands": []}}}], "duration": "EndOfTurn", "evaluation": "Snapshot"}}}}]}}}, {"kind": "If", "args": {"condition": {"kind": "AllOf", "args": [{"kind": "ObjectMatches", "args": {"object": {"kind": "EventSubject"}, "filter": {"kind": "NthCast", "args": {"ordinal": {"kind": "Constant", "args": 2}, "spells": {"kind": "Any"}, "by": {"kind": "One", "args": {"kind": "AbilityController"}}, "scope": "Turn"}}}}, {"kind": "PlayerState", "args": {"player": {"kind": "AbilityController"}, "state": "HeadChef"}}]}, "then": {"instructions": [{"kind": "Effect", "args": {"result": 4294967294, "operation": {"kind": "PutCounters", "args": {"placements": [{"recipients": {"kind": "Players", "args": {"kind": "OpponentsOf", "args": {"kind": "AbilityController"}}}, "kind": {"Named": "PoisonCounter"}, "amount": {"kind": "Constant", "args": 1}, "selection": null}]}}}}]}, "otherwise": {"instructions": []}}}]}}}]"###).unwrap()
}
fn checked(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn full_abilities_validate() {
    for a in source().as_array().unwrap() {
        assert!(checked(a.clone()));
    }
}
#[test]
fn designation_and_monarch_are_distinct() {
    let a = source();
    let p = &a[1]["args"]["ability"]["args"]["body"]["instructions"][0]["args"]["operation"];
    assert_eq!(p["kind"], "BecomeHeadChef");
    let cond = &a[2]["args"]["body"]["instructions"][1]["args"]["condition"]["args"];
    assert!(
        cond.as_array()
            .unwrap()
            .iter()
            .any(|c| c["kind"] == "PlayerState" && c["args"]["state"] == "HeadChef")
    );
}
#[test]
fn wrong_blight_producer_rejected() {
    let mut a = source()[2].clone();
    *a.pointer_mut("/args/body/instructions/0/args/body/instructions/0/args/operation")
        .unwrap() = json!({"kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"Bound","args":0}},"amount":{"kind":"Constant","args":1}}});
    assert!(!checked(a));
}
#[test]
fn blighted_receipt_cannot_escape_player_scope() {
    let mut a = source()[2].clone();
    let op = a["args"]["body"]["instructions"][0]["args"]["body"]["instructions"][1].clone();
    a.pointer_mut("/args/body/instructions")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(op);
    assert!(!checked(a));
}
#[test]
fn conditional_uses_event_cast_ordinal() {
    let a = source();
    let t = &a[2]["args"];
    assert!(t["intervening_if"].is_null());
    assert_eq!(t["event"]["Spell"]["event"], "Cast");
    let c = &t["body"]["instructions"][1]["args"]["condition"]["args"][0];
    assert_eq!(c["args"]["object"]["kind"], "EventSubject");
    assert_eq!(c["args"]["filter"]["args"]["ordinal"]["args"], 2);
}
