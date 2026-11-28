use super::validate_ability;
use oracle_ir::ir::{Ability, EventSpec};
use serde_json::{Value, json};

fn ability(event: Value) -> Ability {
    serde_json::from_value(json!({"kind":"Triggered","args":{
        "event":event,"intervening_if":null,"limits":{"per_turn":null,"total":null},
        "announcement":{"targets":[],"modes":null},"body":{"instructions":[]}
    }}))
    .unwrap()
}

#[test]
fn untap_actor_event_requires_a_permanent_and_binds_actor_and_object() {
    let actor = json!({"Player":{"event":"UntapsPermanent",
        "players":{"kind":"One","args":{"kind":"AbilityController"}},"objects":{"kind":"Any"}}});
    let event: EventSpec = serde_json::from_value(actor.clone()).unwrap();
    let bindings = crate::semantics::event_bindings(&event);
    assert!(bindings.player && bindings.object && bindings.permanent);
    assert!(validate_ability(&ability(actor.clone())).is_ok());
    let mut missing = actor.clone();
    missing["Player"]["objects"] = Value::Null;
    assert!(validate_ability(&ability(missing)).is_err());
    let mut opponents = actor;
    opponents["Player"]["players"] =
        json!({"kind":"OpponentsOf","args":{"kind":"AbilityController"}});
    assert!(validate_ability(&ability(opponents)).is_ok());
}
