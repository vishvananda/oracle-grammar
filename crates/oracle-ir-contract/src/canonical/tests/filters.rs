use crate::{canonical, ir::Ability, validate};
use serde_json::{Value, json};

fn attacked() -> Value {
    json!({"kind":"Compare","args":{
        "left":{"kind":"EventHistory","args":{
            "event":{"Object":{"event":"Attacks",
                "objects":{"kind":"SameObject","args":{"kind":"Candidate"}}}},
            "window":"ThisCombat","measure":"Occurrences"}},
        "test":{"kind":"Compare","args":{"comparison":"GreaterOrEqual",
            "value":{"kind":"Constant","args":1}}}}})
}

/// Total War's "destroy all untapped creatures that didn't attack", with `negated`
/// as the "didn't attack" filter.
fn destroy(negated: Value) -> Value {
    let filter = json!({"kind":"AllOf","args":[
        {"kind":"Not","args":{"kind":"Tapped"}},
        {"kind":"CardType","args":"Creature"},
        negated,
    ]});
    json!([{"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},
        "body":{"instructions":[{"kind":"Effect","args":{"result":0,"operation":{
            "kind":"Destroy","args":{"objects":{"kind":"Zone","args":{
                "scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":filter}},
            "regeneration":false}}}}]}}}])
}

fn canonical(value: Value) -> Value {
    let abilities: Vec<Ability> = serde_json::from_value(value.clone()).unwrap();
    assert!(validate::abilities(&abilities).is_ok());
    let mut value = value;
    canonical::abilities(&mut value).unwrap();
    let once = value.clone();
    canonical::abilities(&mut value).unwrap();
    assert_eq!(value, once);
    value
}

#[test]
fn a_negated_condition_filter_has_one_form() {
    let outside = canonical(destroy(
        json!({"kind":"Not","args":{"kind":"Condition","args":attacked()}}),
    ));
    let inside = canonical(destroy(
        json!({"kind":"Condition","args":{"kind":"Not","args":attacked()}}),
    ));
    assert_eq!(outside, inside);
    let text = inside.to_string();
    assert!(
        text.contains(r#"{"kind":"Condition","args":{"kind":"Not","args":"#),
        "{text}"
    );
    assert!(
        text.contains(r#"{"kind":"Not","args":{"kind":"Tapped"}}"#),
        "{text}"
    );
}
