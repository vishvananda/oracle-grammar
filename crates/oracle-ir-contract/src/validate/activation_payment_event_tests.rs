use super::validate_ability;
use oracle_ir::ir::Ability;
use serde_json::{Value, json};

fn ability(event: Value, expression: Value) -> Ability {
    serde_json::from_value(json!({"kind":"Triggered","args":{
        "event":event,
        "intervening_if":{"kind":"Compare","args":{"left":expression,
            "test":{"kind":"Compare","args":{"comparison":"GreaterOrEqual","value":{"kind":"Constant","args":2}}}}},
        "limits":{},"announcement":{"targets":[],"modes":null},"body":{"instructions":[]}
    }})).unwrap()
}

fn activation() -> Value {
    json!({"Activated":{"players":{"kind":"One","args":{"kind":"AbilityController"}},
        "abilities":{"kind":"OnStack","args":"Loyalty"}}})
}
fn amount() -> Value {
    json!({"kind":"EventActivationCountersRemoved","args":{"kind":{"Named":"LoyaltyCounter"}}})
}
#[test]
fn activation_payment_fact_is_scoped_to_the_matched_activation() {
    validate_ability(&ability(activation(), amount())).unwrap();
    let entry = json!({"ZoneChange":{"objects":{"kind":"Any"},"from":null,"to":"Battlefield"}});
    assert!(validate_ability(&ability(entry.clone(), amount())).is_err());
    assert!(validate_ability(&ability(json!({"AnyOf":[activation(),entry]}), amount())).is_err());
    let own_payment =
        json!({"kind":"PaidCountersRemoved","args":{"payment":"Activation","component":[0]}});
    assert!(validate_ability(&ability(activation(), own_payment)).is_err());
}
