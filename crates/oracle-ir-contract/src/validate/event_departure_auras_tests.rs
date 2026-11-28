use super::validate_ability;
use oracle_ir::ir::Ability;
use serde_json::{Value, json};

fn ability(event: Value) -> Ability {
    serde_json::from_value(json!({"kind":"Triggered","args":{
        "event":event,
        "intervening_if":null,
        "limits":{"per_turn":null,"total":null},
        "announcement":{"targets":[],"modes":null},
        "body":{"instructions":[{"kind":"Effect","args":{
            "result":4294967295u32,
            "operation":{"kind":"Draw","args":{
                "players":{"kind":"One","args":{"kind":"AbilityController"}},
                "amount":{"kind":"Aggregate","args":{
                    "objects":{"kind":"EventDepartureAuras"},
                    "property":"ManaValue","aggregation":"Maximum"
                }}
            }}
        }}]}
    }}))
    .unwrap()
}

#[test]
fn aura_snapshot_requires_completed_battlefield_departure() {
    let dies =
        json!({"ZoneChange":{"objects":{"kind":"Any"},"from":"Battlefield","to":"Graveyard"}});
    let leaves = json!({"ZoneChange":{"objects":{"kind":"Any"},"from":"Battlefield","to":"Exile"}});
    let arrives =
        json!({"ZoneChange":{"objects":{"kind":"Any"},"from":"Graveyard","to":"Battlefield"}});
    let attack = json!({"Object":{"event":"Attacks","objects":{"kind":"Any"}}});
    assert!(validate_ability(&ability(dies.clone())).is_ok());
    assert!(validate_ability(&ability(leaves)).is_ok());
    assert!(validate_ability(&ability(arrives)).is_err());
    assert!(validate_ability(&ability(attack.clone())).is_err());
    assert!(validate_ability(&ability(json!({"FirstEachTurn":dies.clone()}))).is_ok());
    assert!(validate_ability(&ability(json!({"AnyOf":[dies, attack]}))).is_err());
}
