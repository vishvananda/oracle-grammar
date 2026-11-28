use super::validate_ability;
use oracle_ir::ir::Ability;
use serde_json::{Value, json};

fn spell(producer: Value, wrap_optional: bool) -> Ability {
    let effect = json!({"kind":"Effect","args":{"result":0,"operation":producer}});
    let first = if wrap_optional {
        json!({"kind":"Optional","args":{"decider":{"kind":"AbilityController"},"body":{"instructions":[effect]}}})
    } else {
        effect
    };
    serde_json::from_value(json!({"kind":"Spell","args":{
        "announcement":{"targets":[],"modes":null},
        "body":{"instructions":[first,{"kind":"If","args":{
            "condition":{"kind":"MovedToOwnersLibrary","args":{"result":0,"end":"Top"}},
            "then":{"instructions":[]},"otherwise":{"instructions":[]}
        }}]}
    }}))
    .unwrap()
}

fn movement(objects: Value) -> Value {
    json!({"kind":"Move","args":{"objects":objects,"destination":{
        "Library":{"position":"Top","order":"DestinationDefault","arrival":{"counters":[],"states":[]}}},"qualifiers":[]}})
}

#[test]
fn library_placement_receipt_requires_a_visible_singular_move() {
    let one = json!({"kind":"One","args":{"kind":"Source"}});
    validate_ability(&spell(movement(one.clone()), false)).unwrap();
    validate_ability(&spell(movement(one.clone()), true)).unwrap();
    let by_player = json!({"kind":"EachPlayer","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"action":movement(one.clone())}});
    validate_ability(&spell(by_player, true)).unwrap();
    let by_all =
        json!({"kind":"EachPlayer","args":{"players":{"kind":"All"},"action":movement(one)}});
    assert!(validate_ability(&spell(by_all, true)).is_err());
    let batch = json!({"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"Any"}}});
    assert!(validate_ability(&spell(movement(batch), false)).is_err());
    let draw = json!({"kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}}});
    assert!(validate_ability(&spell(draw, false)).is_err());
}

#[test]
fn a_receipt_cannot_be_read_before_its_move() {
    let ability = spell(
        movement(json!({"kind":"One","args":{"kind":"Source"}})),
        false,
    );
    let mut value = serde_json::to_value(ability).unwrap();
    value["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    let reordered: Ability = serde_json::from_value(value).unwrap();
    assert!(validate_ability(&reordered).is_err());
}
