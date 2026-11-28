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
                    "objects":{"kind":"EventGroupDeparture"},
                    "property":"ManaValue","aggregation":"Maximum"
                }}
            }}
        }}]}
    }}))
    .unwrap()
}

fn batch(event: Value, per: &str) -> Value {
    json!({"Batch":{"event":event,"per":per}})
}

#[test]
fn departure_snapshot_requires_a_grouped_zone_change() {
    let leaves = json!({"Object":{"event":"LeavesGraveyard","objects":{"kind":"Any"}}});
    let enters = json!({"Object":{"event":"Attacks","objects":{"kind":"Any"}}});
    let moved = json!({"ZoneChange":{"objects":{"kind":"Any"},"from":"Graveyard","to":"Exile"}});
    assert!(validate_ability(&ability(batch(leaves.clone(), "Whole"))).is_ok());
    assert!(validate_ability(&ability(batch(moved, "Whole"))).is_ok());
    assert!(validate_ability(&ability(leaves.clone())).is_err());
    assert!(validate_ability(&ability(batch(leaves, "Subject"))).is_err());
    assert!(validate_ability(&ability(batch(enters.clone(), "Whole"))).is_err());
    let grouped_departure = batch(
        json!({"Object":{"event":"LeavesGraveyard","objects":{"kind":"Any"}}}),
        "Whole",
    );
    assert!(validate_ability(&ability(json!({"FirstEachTurn":grouped_departure.clone()}))).is_ok());
    assert!(
        validate_ability(&ability(
            json!({"AnyOf":[grouped_departure, batch(enters, "Whole")]})
        ))
        .is_err()
    );
}
