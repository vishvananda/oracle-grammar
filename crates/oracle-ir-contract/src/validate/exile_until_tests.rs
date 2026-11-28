use super::validate_ability;
use oracle_ir::ir::Ability;
use serde_json::{Value, json};

fn spell(first: Value, second_objects: Value) -> Ability {
    serde_json::from_value(json!({"kind":"Spell","args":{
        "announcement":{"targets":[],"modes":null},
        "body":{"instructions":[
            {"kind":"Effect","args":{"result":0,"operation":first}},
            {"kind":"Effect","args":{"result":1,"operation":{
                "kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},
                  "amount":{"kind":"CountObjects","args":second_objects}}
            }}}
        ]}
    }}))
    .unwrap()
}
fn exile(stop: Value) -> Value {
    json!({"kind":"ExileLibraryUntil","args":{
        "actor":{"kind":"AbilityController"},"library":{"kind":"EventSubject"},"stop":stop
    }})
}
#[test]
fn matching_receipt_rejects_other_producers_and_threshold_stop() {
    let matching = exile(
        json!({"Matching":{"filter":{"kind":"Not","args":{"kind":"CardType","args":"Land"}},"count":{"kind":"Constant","args":1}}}),
    );
    let threshold = exile(
        json!({"CumulativeAtLeast":{"property":"ManaValue","threshold":{"kind":"Constant","args":8}}}),
    );
    let receipt = json!({"kind":"ExileUntilMatches","args":0});
    let ordinary = json!({"kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}}});
    // EventSubject is not bound in this spell: the actor/library binding must be validated too.
    assert!(validate_ability(&spell(matching.clone(), receipt.clone())).is_err());
    let bind_library = |mut op: Value| {
        op["args"]["library"] = json!({"kind":"AbilityController"});
        op
    };
    let matching = bind_library(matching);
    assert!(validate_ability(&spell(matching.clone(), receipt.clone())).is_ok());
    let mut optional = serde_json::to_value(spell(matching, receipt.clone())).unwrap();
    let instructions = optional["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap();
    let first = instructions[0].take();
    instructions[0] = json!({"kind":"Optional","args":{"decider":{"kind":"AbilityController"},"body":{"instructions":[first]}}});
    let optional: Ability = serde_json::from_value(optional).unwrap();
    assert!(validate_ability(&optional).is_ok());
    let transitions = json!({"kind":"Transition","args":{"result":0,"phase":"After"}});
    assert!(validate_ability(&spell(bind_library(threshold.clone()), transitions)).is_ok());
    assert!(validate_ability(&spell(bind_library(threshold), receipt.clone())).is_err());
    let invalid_count =
        exile(json!({"Matching":{"filter":{"kind":"Any"},"count":{"kind":"Constant","args":-1}}}));
    assert!(validate_ability(&spell(bind_library(invalid_count), receipt.clone())).is_err());
    assert!(validate_ability(&spell(ordinary, receipt)).is_err());
}
