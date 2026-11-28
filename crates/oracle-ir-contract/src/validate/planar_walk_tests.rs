use crate::{ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"{"kind": "WithPresentation", "args": {"label": null, "reminder": true, "ability": {"kind": "Triggered", "args": {"event": {"Player": {"event": "EncountersPhenomenon", "players": {"kind": "One", "args": {"kind": "AbilityController"}}, "objects": {"kind": "SameObject", "args": {"kind": "Source"}}}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "PlanarRevealAndPlaneswalk", "args": {"player": {"kind": "AbilityController"}, "planes": {"kind": "Constant", "args": 2}, "return_order": "Random", "destination_trigger_rule": "TriggersAgain"}}}}]}}}}}"###).unwrap()
}
fn checked(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn original_is_valid() {
    assert!(checked(source()));
}
#[test]
fn unbound_planar_deck_owner_is_rejected() {
    let mut a = source();
    *a.pointer_mut("/args/ability/args/body/instructions/0/args/operation/args/player")
        .unwrap() = json!({"kind":"Bound","args":42});
    assert!(!checked(a));
}
#[test]
fn unrelated_event_amount_is_rejected() {
    let mut a = source();
    *a.pointer_mut("/args/ability/args/body/instructions/0/args/operation/args/planes")
        .unwrap() = json!({"kind":"EventAmount"});
    assert!(!checked(a));
}
