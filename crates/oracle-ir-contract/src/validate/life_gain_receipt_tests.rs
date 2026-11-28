use super::validate_ability;
use oracle_ir::ir::Ability;
use serde_json::{Value, json};

fn gain(id: u32) -> Value {
    json!({"kind":"Effect","args":{"result":id,"operation":{"kind":"GainLife","args":{
        "players":{"kind":"One","args":{"kind":"AbilityController"}},
        "amount":{"kind":"Constant","args":1}}}}})
}
fn read(id: u32) -> Value {
    json!({"kind":"If","args":{"condition":{"kind":"Compare","args":{
        "left":{"kind":"CountPlayers","args":{"kind":"GainedLifeBy","args":id}},
        "test":{"kind":"Compare","args":{"comparison":"Greater","value":{"kind":"Constant","args":0}}}}},
        "then":{"instructions":[]},"otherwise":{"instructions":[]}}})
}
fn ability(instructions: Vec<Value>) -> Ability {
    serde_json::from_value(json!({"kind":"Spell","args":{
        "announcement":{"targets":[],"modes":null},"body":{"instructions":instructions}}}))
    .unwrap()
}
#[test]
fn actual_gain_receipt_requires_visible_direct_gain() {
    assert!(validate_ability(&ability(vec![gain(7), read(7)])).is_ok());
    let group = json!({"kind":"Group","args":{"instructions":[gain(7)]}});
    assert!(validate_ability(&ability(vec![group, read(7)])).is_ok());
    let mut wrong = gain(7);
    wrong["args"]["operation"]["kind"] = json!("LoseLife");
    assert!(validate_ability(&ability(vec![wrong, read(7)])).is_err());
    let mut wrapped = gain(7);
    let inner = wrapped["args"]["operation"].clone();
    wrapped["args"]["operation"] =
        json!({"kind":"WithSelections","args":{"steps":[],"action":inner}});
    assert!(validate_ability(&ability(vec![wrapped.clone()])).is_ok());
    assert!(validate_ability(&ability(vec![wrapped, read(7)])).is_err());
    assert!(validate_ability(&ability(vec![read(7)])).is_err());
    let optional = json!({"kind":"Optional","args":{"decider":{"kind":"AbilityController"},"body":{"instructions":[gain(7)]}}});
    assert!(validate_ability(&ability(vec![optional, read(7)])).is_err());
    let delayed = json!({"kind":"Effect","args":{"result":8,"operation":{
        "kind":"CreateDelayedTrigger","args":{
            "event":{"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}},
            "start":"Immediately","expires":null,"intervening_if":null,
            "body":{"instructions":[read(7)]}
        }
    }}});
    assert!(validate_ability(&ability(vec![gain(7), delayed])).is_ok());
    let nested = json!({"kind":"Optional","args":{"decider":{"kind":"AbilityController"},"body":{"instructions":[gain(7),read(7)]}}});
    assert!(validate_ability(&ability(vec![nested])).is_ok());
}

#[test]
fn life_gain_receipt_identity_is_alpha_normalized() {
    let normalize = |id| {
        let mut value = json!([ability(vec![gain(id), read(id)])]);
        crate::canonical::abilities(&mut value).unwrap();
        let mut twice = value.clone();
        crate::canonical::abilities(&mut twice).unwrap();
        assert_eq!(value, twice);
        value
    };
    assert_eq!(normalize(7), normalize(83));
}
