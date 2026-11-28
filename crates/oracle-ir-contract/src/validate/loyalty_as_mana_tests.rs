use crate::{ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"[{"kind": "Static", "args": {"kind": "Ordinary", "affected": {"Players": {"kind": "One", "args": {"kind": "AbilityController"}}}, "changes": [{"kind": "PlayerRule", "args": {"PlayPermissionWithCounterPayment": {"permission": {"PlayPermissionWith": {"permission": {"PlayPermission": {"cards": {"kind": "Zone", "args": {"scope": {"Shared": "Exile"}, "kind": "Card", "filter": {"kind": "AllOf", "args": [{"kind": "OwnedBy", "args": {"kind": "OpponentsOf", "args": {"kind": "AbilityController"}}}, {"kind": "Counters", "args": {"kind": {"Named": "DeceitCounter"}, "comparison": "Greater", "value": {"kind": "Constant", "args": 0}}}]}}}, "origins": ["Exile"], "action": "CastSpell", "filter": {"kind": "Any"}}}, "terms": {"limit": null, "during": {"kind": "One", "args": {"kind": "AbilityController"}}, "cost": "Normal", "spend_as": null, "into_exile": false, "condition": null, "effects": [], "flash": false, "may_look": false, "then": null, "face_down": null, "cast_filter": null}}}, "sources": {"kind": "Zone", "args": {"scope": {"Shared": "Battlefield"}, "kind": "Permanent", "filter": {"kind": "AllOf", "args": [{"kind": "CardType", "args": "Planeswalker"}, {"kind": "ControlledBy", "args": {"kind": "One", "args": {"kind": "AbilityController"}}}]}}}, "counter": {"Named": "LoyaltyCounter"}, "spend_as": "AnyColor", "minimum_counter_use": null, "ordinary_mana_may_supplement": null}}}]}}, {"kind": "Activated", "args": {"origin": "Ordinary", "cost": {"parts": [{"Loyalty": {"kind": "Constant", "args": 1}}]}, "limits": {"once": false, "once_each_turn": false, "sorcery": true, "condition": null, "activators": null, "keyword": null, "modifiers": []}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "ForEachPlayer", "args": {"players": {"kind": "OpponentsOf", "args": {"kind": "AbilityController"}}, "binding": 0, "starting_with": null, "in_chosen_direction": false, "body": {"instructions": [{"kind": "Effect", "args": {"result": 0, "operation": {"kind": "Move", "args": {"objects": {"kind": "Take", "args": {"objects": {"kind": "Zone", "args": {"scope": {"Players": {"zone": "Library", "players": {"kind": "One", "args": {"kind": "Bound", "args": 0}}}}, "kind": "Card", "filter": {"kind": "Any"}}}, "count": {"kind": "Constant", "args": 2}, "from": "Top"}}, "destination": {"Exile": {"visibility": "FaceUp", "arrival": {"counters": [], "states": []}}}, "qualifiers": []}}}}]}}}, {"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "PutCounters", "args": {"placements": [{"recipients": {"kind": "Objects", "args": {"kind": "Transition", "args": {"result": 0, "phase": "After"}}}, "kind": {"Named": "DeceitCounter"}, "amount": {"kind": "Constant", "args": 1}, "selection": null}]}}}}]}}}, {"kind": "Activated", "args": {"origin": "Ordinary", "cost": {"parts": [{"Loyalty": {"kind": "Constant", "args": -3}}]}, "limits": {"once": false, "once_each_turn": false, "sorcery": true, "condition": null, "activators": null, "keyword": null, "modifiers": []}, "announcement": {"targets": [{"slot": 0, "kind": {"Object": {"kind": "Permanent", "zones": ["Battlefield"]}}, "count": {"Exactly": {"kind": "Constant", "args": 1}}, "filter": {"kind": "CardType", "args": "Creature"}, "players": null, "distribute": null, "constraints": [], "chooser": null}], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 0, "operation": {"kind": "Move", "args": {"objects": {"kind": "One", "args": {"kind": "Target", "args": {"slot": 0, "index": 0}}}, "destination": {"Exile": {"visibility": "FaceUp", "arrival": {"counters": [], "states": []}}}, "qualifiers": []}}}}, {"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "PutCounters", "args": {"placements": [{"recipients": {"kind": "Objects", "args": {"kind": "One", "args": {"kind": "Result", "args": {"result": 0, "phase": "After", "index": 0}}}}, "kind": {"Named": "DeceitCounter"}, "amount": {"kind": "Constant", "args": 1}, "selection": null}]}}}}]}}}, {"kind": "DeckRule", "args": "CanBeCommander"}]"###).unwrap()
}
fn checked(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn counter_payment_keeps_ownership_and_policies() {
    let a = source();
    for x in a.as_array().unwrap() {
        assert!(checked(x.clone()));
    }
    let p = &a[0]["args"]["changes"][0]["args"]["PlayPermissionWithCounterPayment"];
    assert_eq!(p["counter"], json!({"Named":"LoyaltyCounter"}));
    assert_eq!(p["spend_as"], "AnyColor");
    assert!(p["minimum_counter_use"].is_null());
    assert!(p["ordinary_mana_may_supplement"].is_null());
    let r = &p["permission"]["PlayPermissionWith"];
    assert_eq!(r["terms"]["during"]["args"]["kind"], "AbilityController");
    let cards = &r["permission"]["PlayPermission"]["cards"]["args"];
    assert_eq!(cards["scope"], json!({"Shared":"Exile"}));
    let fs = cards["filter"]["args"].as_array().unwrap();
    assert!(
        fs.iter()
            .any(|f| f["kind"] == "OwnedBy" && f["args"]["kind"] == "OpponentsOf")
    );
    assert!(
        fs.iter()
            .any(|f| f["kind"] == "Counters"
                && f["args"]["kind"] == json!({"Named":"DeceitCounter"}))
    );
}
#[test]
fn counter_payment_rejects_land_permission() {
    let mut a = source()[0].clone();
    *a.pointer_mut("/args/changes/0/args/PlayPermissionWithCounterPayment/permission/PlayPermissionWith/permission/PlayPermission/action").unwrap()=json!("PlayLand");
    assert!(!checked(a));
}
#[test]
fn counter_placements_follow_actual_exile_receipts() {
    let a = source();
    let plus = &a[1]["args"]["body"]["instructions"];
    assert_eq!(plus[0]["args"]["players"]["kind"], "OpponentsOf");
    assert_eq!(
        plus[1]["args"]["operation"]["args"]["placements"][0]["recipients"]["args"]["kind"],
        "Transition"
    );
    let minus = &a[2]["args"]["body"]["instructions"];
    assert_eq!(
        minus[1]["args"]["operation"]["args"]["placements"][0]["recipients"]["args"]["args"]["kind"],
        "Result"
    );
}
