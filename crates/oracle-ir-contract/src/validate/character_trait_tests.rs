use super::validate_ability;
use oracle_ir::ir::Ability;
use serde_json::{Value, json};

fn commander_filter(filter: Value) -> Ability {
    serde_json::from_value(json!({"kind":"CommanderDeckRule","args":{
        "kind":"Term","args":{"family":"DeckConstruction","form":"CanHaveAnyCards",
            "operands":[{"kind":"Filter","args":filter}]}
    }}))
    .unwrap()
}
#[test]
fn transgender_character_is_a_catalog_trait_not_a_name_or_unbound_reference() {
    let trait_rule =
        commander_filter(json!({"kind":"RepresentsCharacterTrait","args":"Transgender"}));
    let named_rule = commander_filter(json!({"kind":"RepresentsCharacter","args":"Alesha"}));
    assert!(validate_ability(&trait_rule).is_ok());
    assert!(validate_ability(&named_rule).is_ok());
    assert_ne!(
        serde_json::to_value(trait_rule).unwrap(),
        serde_json::to_value(named_rule).unwrap()
    );
    let invalid = commander_filter(json!({"kind":"SameObject","args":{"kind":"EventSubject"}}));
    assert!(validate_ability(&invalid).is_err());
}
