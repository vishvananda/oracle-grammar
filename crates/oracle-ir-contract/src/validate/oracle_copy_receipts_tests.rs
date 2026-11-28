use crate::{ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"{"kind": "Triggered", "args": {"event": {"Damage": {"source_kind": "Permanent", "sources": {"kind": "AllOf", "args": [{"kind": "CardType", "args": "Creature"}, {"kind": "ControlledBy", "args": {"kind": "One", "args": {"kind": "AbilityController"}}}, {"kind": "Not", "args": {"kind": "SameObject", "args": {"kind": "Source"}}}]}, "recipients": {"Players": {"kind": "All"}}, "kind": "Combat", "grouping": "Recipient", "subject": "Source"}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "Sacrifice", "args": {"players": {"kind": "One", "args": {"kind": "AbilityController"}}, "objects": {"kind": "Matching", "args": {"objects": {"kind": "Damage", "args": "Source"}, "filter": {"kind": "AllOf", "args": [{"kind": "InZone", "args": {"Shared": "Battlefield"}}, {"kind": "ControlledBy", "args": {"kind": "One", "args": {"kind": "AbilityController"}}}]}}}, "choose": {"AsManyAsCan": {"kind": "Constant", "args": 1}}}}}}, {"kind": "Effect", "args": {"result": 0, "operation": {"kind": "PlayWith", "args": {"actor": {"kind": "One", "args": {"kind": "AbilityController"}}, "action": "CastSpell", "cards": {"kind": "Zone", "args": {"scope": {"Players": {"zone": "Hand", "players": {"kind": "One", "args": {"kind": "AbilityController"}}}}, "kind": "Card", "filter": {"kind": "Any"}}}, "count": {"UpTo": {"kind": "Constant", "args": 1}}, "terms": {"limit": null, "during": null, "cost": "Free", "spend_as": null, "into_exile": false, "condition": null, "effects": [], "flash": false, "may_look": false, "then": null, "face_down": null, "cast_filter": {"kind": "AllOf", "args": [{"kind": "Color", "args": "Black"}, {"kind": "Color", "args": "Blue"}, {"kind": "Color", "args": "Red"}, {"kind": "Property", "args": {"property": "ColorCount", "test": {"kind": "Compare", "args": {"comparison": "Equal", "value": {"kind": "Constant", "args": 3}}}}}]}}, "group": null}}}}, {"kind": "If", "args": {"condition": {"kind": "Not", "args": {"kind": "CastPerformed", "args": 0}}, "then": {"instructions": [{"kind": "Effect", "args": {"result": 1, "operation": {"kind": "ChooseOracleDefinitionAtRandom", "args": {"chooser": {"kind": "AbilityController"}, "cards": {"kind": "Term", "args": {"family": "CardsInOracle", "form": "Matching", "operands": [{"kind": "Filter", "args": {"kind": "AllOf", "args": [{"kind": "Color", "args": "Black"}, {"kind": "Color", "args": "Blue"}, {"kind": "Color", "args": "Red"}, {"kind": "Property", "args": {"property": "ColorCount", "test": {"kind": "Compare", "args": {"comparison": "Equal", "value": {"kind": "Constant", "args": 3}}}}}]}}]}}}}}}, {"kind": "Effect", "args": {"result": 2, "operation": {"kind": "CopyOracleDefinition", "args": {"actor": {"kind": "AbilityController"}, "selection": 1}}}}, {"kind": "Effect", "args": {"result": 4294967294, "operation": {"kind": "PlayWith", "args": {"actor": {"kind": "One", "args": {"kind": "AbilityController"}}, "action": "CastSpell", "cards": {"kind": "OracleCopies", "args": 2}, "count": {"UpTo": {"kind": "Constant", "args": 1}}, "terms": {"limit": null, "during": null, "cost": "Free", "spend_as": null, "into_exile": false, "condition": null, "effects": [], "flash": false, "may_look": false, "then": null, "face_down": null, "cast_filter": null}, "group": null}}}}]}, "otherwise": {"instructions": []}}}]}}}"###).unwrap()
}
fn checked(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn genuine_receipts_are_valid() {
    assert!(checked(source()));
}
#[test]
fn cast_test_rejects_non_cast_result() {
    let mut a = source();
    *a.pointer_mut("/args/body/instructions/2/args/condition/args/args")
        .unwrap() = json!(4294967295_u32);
    assert!(!checked(a));
}
#[test]
fn cast_test_rejects_land_play() {
    let mut a = source();
    *a.pointer_mut("/args/body/instructions/1/args/operation/args/action")
        .unwrap() = json!("PlayLand");
    assert!(!checked(a));
}
#[test]
fn oracle_copy_rejects_cast_receipt() {
    let mut a = source();
    *a.pointer_mut(
        "/args/body/instructions/2/args/then/instructions/1/args/operation/args/selection",
    )
    .unwrap() = json!(0);
    assert!(!checked(a));
}
#[test]
fn oracle_copy_rejects_forward_reference() {
    let mut a = source();
    *a.pointer_mut(
        "/args/body/instructions/2/args/then/instructions/1/args/operation/args/selection",
    )
    .unwrap() = json!(2);
    assert!(!checked(a));
}
#[test]
fn copy_reader_rejects_selection_receipt() {
    let mut a = source();
    *a.pointer_mut(
        "/args/body/instructions/2/args/then/instructions/2/args/operation/args/cards/args",
    )
    .unwrap() = json!(1);
    assert!(!checked(a));
}
#[test]
fn definition_rejects_wrong_query_family() {
    let mut a = source();
    *a.pointer_mut(
        "/args/body/instructions/2/args/then/instructions/0/args/operation/args/cards/args/family",
    )
    .unwrap() = json!("Craftable");
    assert!(!checked(a));
}
#[test]
fn branch_result_cannot_escape_scope() {
    let mut a = source();
    let escaped = json!({"kind":"Effect","args":{"result":99,"operation":{"kind":"CopyOracleDefinition","args":{"actor":{"kind":"AbilityController"},"selection":1}}}});
    a.pointer_mut("/args/body/instructions")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(escaped);
    assert!(!checked(a));
}
