use super::validate_ability;
use crate::semantics::event_bindings;
use oracle_ir::ir::{Ability, EventSpec};
use serde_json::{Value, json};

fn event() -> Value {
    json!({"OptionalCostUnpaid":{"players":{"kind":"All"},"cost":{"parts":[{"Mana":[{"Generic":1}]}]}}})
}
fn ability(event: Value) -> Ability {
    serde_json::from_value(json!({"kind":"Triggered","args":{
        "event":event,"limits":{"per_turn":null,"total":null},"announcement":{"targets":[],"modes":null},
        "body":{"instructions":[{"kind":"Effect","args":{"result":0,"operation":{"kind":"Draw","args":{
            "players":{"kind":"One","args":{"kind":"EventSubject"}},"amount":{"kind":"Constant","args":1}
        }}}}]}
    }})).unwrap()
}
#[test]
fn actual_optional_unpaid_event_binds_exact_player() {
    let event: EventSpec = serde_json::from_value(event()).unwrap();
    let bindings = event_bindings(&event);
    assert!(bindings.player);
    assert!(!bindings.object && !bindings.spell && !bindings.amount);
    assert!(validate_ability(&ability(serde_json::to_value(event).unwrap())).is_ok());
}
#[test]
fn event_player_is_not_available_while_filtering_the_event() {
    let mut invalid = event();
    invalid["OptionalCostUnpaid"]["players"] = json!({"kind":"One","args":{"kind":"EventSubject"}});
    assert!(validate_ability(&ability(invalid)).is_err());
}
