use crate::{ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"{"kind": "WithPresentation", "args": {"label": null, "reminder": true, "ability": {"kind": "Triggered", "args": {"event": {"Player": {"event": "EncountersPhenomenon", "players": {"kind": "One", "args": {"kind": "AbilityController"}}, "objects": {"kind": "SameObject", "args": {"kind": "Source"}}}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "ForEachPlayer", "args": {"players": {"kind": "All"}, "binding": 0, "starting_with": null, "in_chosen_direction": false, "body": {"instructions": [{"kind": "Effect", "args": {"result": 0, "operation": {"kind": "ChooseObjects", "args": {"player": {"kind": "Bound", "args": 0}, "objects": {"kind": "Zone", "args": {"scope": {"Shared": "Battlefield"}, "kind": "Permanent", "filter": {"kind": "CardType", "args": "Creature"}}}, "count": {"UpTo": {"kind": "PlayerNumber", "args": {"player": {"kind": "Bound", "args": 0}, "number": "SuperHeroMediaConsumedLastMonth"}}}, "constraints": []}}}}]}}}, {"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "Destroy", "args": {"objects": {"kind": "Difference", "args": {"objects": {"kind": "Zone", "args": {"scope": {"Shared": "Battlefield"}, "kind": "Permanent", "filter": {"kind": "CardType", "args": "Creature"}}}, "excluding": {"kind": "Selected", "args": 0}}}, "regeneration": true}}}}]}}}}}"###).unwrap()
}
fn checked(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn per_player_count_and_one_destroy_union() {
    let a = source();
    assert!(checked(a.clone()));
    let p = &a["args"]["ability"]["args"]["body"]["instructions"];
    assert_eq!(p.as_array().unwrap().len(), 2);
    let run = &p[0]["args"];
    assert_eq!(run["players"]["kind"], "All");
    let choice = &run["body"]["instructions"][0]["args"]["operation"]["args"];
    assert_eq!(choice["player"], json!({"kind":"Bound","args":0}));
    assert_eq!(choice["count"]["UpTo"]["args"]["player"], choice["player"]);
    assert_eq!(
        choice["count"]["UpTo"]["args"]["number"],
        "SuperHeroMediaConsumedLastMonth"
    );
    assert_eq!(
        choice["objects"]["args"]["filter"],
        json!({"kind":"CardType","args":"Creature"})
    );
    let destroy = &p[1]["args"]["operation"];
    assert_eq!(destroy["kind"], "Destroy");
    assert_eq!(
        destroy["args"]["objects"]["args"]["excluding"],
        json!({"kind":"Selected","args":0})
    );
    assert_eq!(
        destroy["args"]["objects"]["args"]["objects"],
        choice["objects"]
    );
}
#[test]
fn missing_choice_receipt_is_rejected() {
    let mut a = source();
    *a.pointer_mut(
        "/args/ability/args/body/instructions/1/args/operation/args/objects/args/excluding/args",
    )
    .unwrap() = json!(12);
    assert!(!checked(a));
}
#[test]
fn bound_player_cannot_escape_loop() {
    let mut a = source();
    *a.pointer_mut(
        "/args/ability/args/body/instructions/1/args/operation/args/objects/args/excluding",
    )
    .unwrap() = json!({"kind":"Zone","args":{"scope":{"Players":{"zone":"Battlefield","players":{"kind":"One","args":{"kind":"Bound","args":0}}}},"kind":"Permanent","filter":{"kind":"Any"}}});
    assert!(!checked(a));
}
