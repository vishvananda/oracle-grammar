use crate::{ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"{"kind": "WithPresentation", "args": {"label": null, "reminder": true, "ability": {"kind": "Triggered", "args": {"event": {"AnyOf": [{"Object": {"event": "Attacks", "objects": {"kind": "SameObject", "args": {"kind": "Source"}}}}, {"ZoneChange": {"objects": {"kind": "SameObject", "args": {"kind": "Source"}}, "from": null, "to": "Battlefield"}}]}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 0, "operation": {"kind": "ChooseOracleDefinitionAtRandom", "args": {"chooser": {"kind": "AbilityController"}, "cards": {"kind": "Term", "args": {"family": "CardsInOracle", "form": "And", "operands": [{"kind": "Term", "args": {"family": "CardsInOracle", "form": "EternalLegal", "operands": []}}, {"kind": "Filter", "args": {"kind": "AllOf", "args": [{"kind": "NameMatches", "args": {"Word": {"word": "Urza", "possessive": true}}}, {"kind": "Not", "args": {"kind": "CardType", "args": "Land"}}]}}]}}}}}}, {"kind": "Optional", "args": {"decider": {"kind": "AbilityController"}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 1, "operation": {"kind": "CopyOracleDefinition", "args": {"actor": {"kind": "AbilityController"}, "selection": 0}}}}, {"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "PlayWith", "args": {"actor": {"kind": "One", "args": {"kind": "AbilityController"}}, "action": "CastSpell", "cards": {"kind": "OracleCopies", "args": 1}, "count": {"Exactly": {"kind": "Constant", "args": 1}}, "terms": {"limit": null, "during": null, "cost": "Free", "spend_as": null, "into_exile": false, "condition": null, "effects": [], "flash": false, "may_look": false, "then": null, "face_down": null, "cast_filter": null}, "group": null}}}}]}}}]}}}}}"###).unwrap()
}
fn ok(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn word_and_possessive_are_explicit() {
    let a = source();
    assert!(ok(a.clone()));
    let w = &a["args"]["ability"]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cards"]
        ["args"]["operands"][1]["args"]["args"][0]["args"]["Word"];
    assert_eq!(w["word"], "Urza");
    assert_eq!(w["possessive"], true);
}
#[test]
fn empty_or_multiple_word_query_rejected() {
    for t in ["", "Urza finder"] {
        let mut a = source();
        *a.pointer_mut("/args/ability/args/body/instructions/0/args/operation/args/cards/args/operands/1/args/args/0/args/Word/word").unwrap()=json!(t);
        assert!(!ok(a));
    }
}
