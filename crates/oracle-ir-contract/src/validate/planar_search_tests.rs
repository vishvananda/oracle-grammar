use crate::{ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"{"kind": "WithPresentation", "args": {"label": null, "reminder": true, "ability": {"kind": "Triggered", "args": {"event": {"Player": {"event": "EncountersPhenomenon", "players": {"kind": "One", "args": {"kind": "AbilityController"}}, "objects": {"kind": "SameObject", "args": {"kind": "Source"}}}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 0, "operation": {"kind": "SearchPlanarDeck", "args": {"searcher": {"kind": "AbilityController"}, "deck": {"ApplicableTo": {"kind": "AbilityController"}}, "eligibility": {"NeverPlaneswalkedTo": {"traveler": {"kind": "AbilityController"}, "history": "Lifetime"}}}}}}, {"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "ShufflePlanarDeck", "args": {"deck": {"ApplicableTo": {"kind": "AbilityController"}}}}}}, {"kind": "Effect", "args": {"result": 4294967294, "operation": {"kind": "PlaneswalkToSearchedPlane", "args": {"player": {"kind": "AbilityController"}, "search": 0}}}}]}}}}}"###).unwrap()
}
fn checked(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn lifetime_search_receipt_survives_shuffle() {
    let a = source();
    assert!(checked(a.clone()));
    let ins = &a["args"]["ability"]["args"]["body"]["instructions"];
    assert_eq!(
        ins[0]["args"]["operation"]["args"]["eligibility"]["NeverPlaneswalkedTo"]["history"],
        "Lifetime"
    );
    assert_eq!(ins[1]["args"]["operation"]["kind"], "ShufflePlanarDeck");
    assert_eq!(ins[2]["args"]["operation"]["args"]["search"], 0);
}
#[test]
fn wrong_producer_rejected() {
    let mut a = source();
    *a.pointer_mut("/args/ability/args/body/instructions/2/args/operation/args/search")
        .unwrap() = json!(4294967295_u32);
    assert!(!checked(a));
}
#[test]
fn forward_reference_rejected() {
    let mut a = source();
    a.pointer_mut("/args/ability/args/body/instructions")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .swap(0, 2);
    assert!(!checked(a));
}
#[test]
fn unbound_history_traveler_rejected() {
    let mut a = source();
    *a.pointer_mut("/args/ability/args/body/instructions/0/args/operation/args/eligibility/NeverPlaneswalkedTo/traveler").unwrap()=json!({"kind":"Bound","args":9});
    assert!(!checked(a));
}
#[test]
fn unbound_deck_player_rejected() {
    let mut a = source();
    *a.pointer_mut(
        "/args/ability/args/body/instructions/1/args/operation/args/deck/ApplicableTo",
    )
    .unwrap() = json!({"kind":"Bound","args":9});
    assert!(!checked(a));
}
