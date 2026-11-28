use super::validate_ability;
use oracle_ir::ir::Ability;
use serde_json::{Value, json};

fn ability(actions: Value, participant: Value) -> Ability {
    serde_json::from_value(json!({"kind":"Triggered","args":{
        "event":{"Spell":{"event":"Cast","players":{"kind":"One","args":{"kind":"AbilityController"}},
            "spells":{"kind":"CastPaymentParticipant","args":{"participant":participant,"actions":actions}}}},
        "intervening_if":null,"limits":{"per_turn":null,"total":null},
        "announcement":{"targets":[],"modes":null},"body":{"instructions":[]}
    }})).unwrap()
}

#[test]
fn payment_actions_require_distinct_nonempty_actions_and_scoped_participants() {
    let creature = json!({"kind":"CardType","args":"Creature"});
    for action in ["TapForCost", "TapForManaSpent", "SacrificeForCost"] {
        assert!(validate_ability(&ability(json!([action]), creature.clone())).is_ok());
    }
    assert!(
        validate_ability(&ability(
            json!(["TapForCost", "TapForManaSpent", "SacrificeForCost"]),
            creature.clone()
        ))
        .is_ok()
    );
    assert!(validate_ability(&ability(json!([]), creature.clone())).is_err());
    assert!(validate_ability(&ability(json!(["TapForCost", "TapForCost"]), creature)).is_err());
    let unbound = json!({"kind":"SameObject","args":{"kind":"Target","args":{"slot":7,"index":0}}});
    assert!(validate_ability(&ability(json!(["TapForCost"]), unbound)).is_err());
}
