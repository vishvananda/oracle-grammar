use super::validate_ability;
use oracle_ir::ir::Ability;
use serde_json::json;

#[test]
fn land_play_cost_is_a_required_action_cost_not_a_spell_or_mana_ability() {
    let value = json!({"kind":"LandPlayCost","args":{"cost":{"parts":[{"Mana":[{"Generic":1},{"Color":"Blue"}]}]}}});
    let ability: Ability = serde_json::from_value(value.clone()).unwrap();
    assert!(validate_ability(&ability).is_ok());
    let mut empty = value;
    empty["args"]["cost"]["parts"] = json!([]);
    let empty: Ability = serde_json::from_value(empty).unwrap();
    assert!(validate_ability(&empty).is_err());
}
