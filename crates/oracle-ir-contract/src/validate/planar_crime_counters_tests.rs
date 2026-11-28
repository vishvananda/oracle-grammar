use crate::{ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"[{"kind": "WithPresentation", "args": {"label": null, "reminder": true, "ability": {"kind": "Triggered", "args": {"event": {"Player": {"event": "Planeswalks", "players": {"kind": "One", "args": {"kind": "AbilityController"}}, "objects": {"kind": "SameObject", "args": {"kind": "Source"}}}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 0, "operation": {"kind": "Move", "args": {"objects": {"kind": "Zone", "args": {"scope": {"Shared": "Battlefield"}, "kind": "Permanent", "filter": {"kind": "AnyOf", "args": [{"kind": "Subtype", "args": "Assassin"}, {"kind": "Subtype", "args": "Mercenary"}, {"kind": "Subtype", "args": "Pirate"}, {"kind": "Subtype", "args": "Rogue"}, {"kind": "Subtype", "args": "Warlock"}]}}}, "destination": {"Exile": {"visibility": "FaceUp", "arrival": {"counters": [], "states": []}}}, "qualifiers": []}}}}, {"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "ReturnAfter", "args": {"objects": {"kind": "Transition", "args": {"result": 0, "phase": "After"}}, "duration": {"WhileFaceUpPlane": {"kind": "Source"}}}}}}]}}}}}, {"kind": "WithPresentation", "args": {"label": null, "reminder": true, "ability": {"kind": "Static", "args": {"kind": "Ordinary", "affected": {"Players": {"kind": "All"}}, "changes": [{"kind": "PlayerRule", "args": {"Prohibit": "CommitCrime"}}]}}}}, {"kind": "Triggered", "args": {"event": {"Player": {"event": "ChaosEnsues", "players": {"kind": "All"}, "objects": null}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 0, "operation": {"kind": "PutCounters", "args": {"placements": [{"recipients": {"kind": "Objects", "args": {"kind": "Zone", "args": {"scope": {"Shared": "Battlefield"}, "kind": "Permanent", "filter": {"kind": "AllOf", "args": [{"kind": "CardType", "args": "Creature"}, {"kind": "ControlledBy", "args": {"kind": "One", "args": {"kind": "AbilityController"}}}]}}}}, "kind": {"PowerToughness": [1, 1]}, "amount": {"kind": "Constant", "args": 1}, "selection": null}]}}}}, {"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "PutCounters", "args": {"placements": [{"recipients": {"kind": "Objects", "args": {"kind": "CounterRecipients", "args": 0}}, "kind": {"PowerToughness": [1, 1]}, "amount": {"kind": "CountObjects", "args": {"kind": "Zone", "args": {"scope": {"Shared": "Exile"}, "kind": "Card", "filter": {"kind": "AllOf", "args": [{"kind": "AnyOf", "args": [{"kind": "Subtype", "args": "Assassin"}, {"kind": "Subtype", "args": "Mercenary"}, {"kind": "Subtype", "args": "Pirate"}, {"kind": "Subtype", "args": "Rogue"}, {"kind": "Subtype", "args": "Warlock"}]}, {"kind": "ExiledWith", "args": {"kind": "Source"}}]}}}}, "selection": null}]}}}}]}}}]"###).unwrap()
}
fn checked(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn complete_abilities_validate() {
    for a in source().as_array().unwrap() {
        assert!(checked(a.clone()));
    }
}
#[test]
fn wrong_counter_producer_rejected() {
    let mut a = source()[2].clone();
    *a.pointer_mut("/args/body/instructions/0/args/operation")
        .unwrap() = json!({"kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}}});
    assert!(!checked(a));
}
#[test]
fn forward_counter_reference_rejected() {
    let mut a = source()[2].clone();
    *a.pointer_mut(
        "/args/body/instructions/1/args/operation/args/placements/0/recipients/args/args",
    )
    .unwrap() = json!(4294967295_u32);
    assert!(!checked(a));
}
#[test]
fn unbound_plane_reference_rejected() {
    let mut a = source()[0].clone();
    *a.pointer_mut(
        "/args/ability/args/body/instructions/1/args/operation/args/duration/WhileFaceUpPlane",
    )
    .unwrap() = json!({"kind":"Bound","args":4});
    assert!(!checked(a));
}
#[test]
fn crime_prohibition_is_actor_relative() {
    let a = source();
    assert_eq!(
        a[1]["args"]["ability"]["args"]["changes"][0]["args"]["Prohibit"],
        "CommitCrime"
    );
}
