use super::validate_ability;
use oracle_ir::ir::Ability;
use serde_json::json;

#[test]
fn deck_restriction_keeps_printing_roles_and_independent_exceptions() {
    let value = json!({"kind":"DeckOnlyCards","args":{
        "roles":"CommanderOrCompanion",
        "allowed":{"kind":"AnyOf","args":[
            {"kind":"PrintingHas","args":"MysteryBoosterSet"},
            {"kind":"PrintingHas","args":"PlaytestCard"}
        ]},
        "basic_lands":true,"any_color_identity":true
    }});
    let ability: Ability = serde_json::from_value(value.clone()).unwrap();
    assert!(validate_ability(&ability).is_ok());
    for field in ["basic_lands", "any_color_identity"] {
        let mut changed = value.clone();
        changed["args"][field] = json!(false);
        let changed: Ability = serde_json::from_value(changed.clone()).unwrap();
        assert!(validate_ability(&changed).is_ok());
        assert_ne!(
            serde_json::to_value(&ability).unwrap(),
            serde_json::to_value(changed).unwrap()
        );
    }
    let mut invalid = value;
    invalid["args"]["allowed"] = json!({"kind":"SameObject","args":{"kind":"EventSubject"}});
    let invalid: Ability = serde_json::from_value(invalid).unwrap();
    assert!(validate_ability(&invalid).is_err());
}
