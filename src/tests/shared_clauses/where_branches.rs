//! A shared X in a paid branch is captured there, after paying and before either use.
use super::*;

const TEXT: &str = "Whenever this creature attacks, you may exile a creature card from your graveyard. If you do, each opponent loses X life and you gain X life, where X is the number of creature cards exiled with this creature.";

#[test]
fn linked_count_is_captured_inside_the_paid_branch_for_both_life_amounts() {
    let readings = engine().parse("Context example", TEXT).unwrap().readings;
    assert_eq!(
        readings.len(),
        2,
        "native move and offered-action payment views"
    );
    for reading in readings {
        validate_abilities(&reading).unwrap();
        let outer = reading[0]["args"]["body"]["instructions"]
            .as_array()
            .unwrap();
        assert_eq!(outer.len(), 2);
        assert_eq!(outer[0]["args"]["operation"]["kind"], "PayCost");
        assert_eq!(outer[1]["kind"], "If");
        assert_eq!(outer[1]["args"]["condition"]["kind"], "PaymentChosen");
        assert_eq!(
            outer[1]["args"]["condition"]["args"],
            outer[0]["args"]["result"]
        );
        let then = outer[1]["args"]["then"]["instructions"].as_array().unwrap();
        assert_eq!(then.len(), 3);
        assert_eq!(then[0]["args"]["operation"]["kind"], "BindValue");
        let definition = &then[0]["args"]["operation"]["args"]["value"];
        assert_eq!(definition["kind"], "CountObjects");
        let mut types = Vec::new();
        visit(definition, "CardType", &mut types);
        assert_eq!(types, [json!("Creature")]);
        for instruction in &then[1..] {
            assert_eq!(
                instruction["args"]["operation"]["args"]["amount"],
                json!({
                "kind":"BoundValue","args":then[0]["args"]["result"]})
            );
        }
        printed(&reading);
    }
}

#[test]
fn editing_a_paid_linked_definition_keeps_the_payment_and_uses_in_place() {
    let readings = engine().parse("Context example", TEXT).unwrap().readings;
    let mut reading = readings
        .into_iter()
        .find(|r| {
            r[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"][0]
                .get("Move")
                .is_some()
        })
        .unwrap();
    let payment = reading[0]["args"]["body"]["instructions"][0].clone();
    let definition = &mut reading[0]["args"]["body"]["instructions"][1]["args"]["then"]["instructions"]
        [0]["args"]["operation"]["args"]["value"];
    for item in definition["args"]["args"]["filter"]["args"]
        .as_array_mut()
        .unwrap()
    {
        if item["kind"] == "CardType" {
            item["args"] = json!("Artifact");
        }
    }
    let text = printed(&reading);
    assert!(text.contains("artifact cards exiled with"), "{text}");
    assert_eq!(reading[0]["args"]["body"]["instructions"][0], payment);
}

fn pair() -> Value {
    json!({"instructions":[
        {"kind":"Effect","args":{"result":0,"operation":{"kind":"LoseLife","args":{
            "players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"AnnouncedX"}}}}},
        {"kind":"Effect","args":{"result":1,"operation":{"kind":"GainLife","args":{
            "players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"AnnouncedX"}}}}}
    ]})
}

#[test]
fn outer_where_cannot_inline_an_inner_shared_value_or_capture_it_too_early() {
    use crate::functions::value_snapshot::{required, snapshot};
    let amount = json!({"kind":"CountObjects","args":{"kind":"Zone","args":{
        "scope":{"Shared":"Exile"},"kind":"Card","filter":{"kind":"Any"}}}});
    let outer = json!({"instructions":[{"kind":"If","args":{
        "condition":{"kind":"Compare","args":{"left":{"kind":"Constant","args":1},"test":{"kind":"Compare","args":{"comparison":"Equal","value":{"kind":"Constant","args":1}}}}},"then":pair(),"otherwise":{"instructions":[]}}}]});
    assert!(required(&pair(), &amount).unwrap());
    assert!(required(&outer, &amount).unwrap());
    assert!(snapshot(&outer, &amount, 0).is_err());
    assert!(snapshot(&pair(), &amount, 0).is_ok());
    for nested in [
        json!({"kind":"Triggered","args":{"body":pair()}}),
        json!({"$sym":"holder","args":[pair()]}),
    ] {
        let outer = json!({"instructions":[nested]});
        assert!(!required(&outer, &amount).unwrap());
    }
}

#[test]
fn shared_scope_detection_has_explicit_work_and_depth_bounds() {
    let amount = json!({"kind":"Constant","args":1});
    let wide = Value::Array(vec![Value::Null; 131_073]);
    assert!(crate::functions::value_snapshot::required(&wide, &amount).is_err());
    let mut deep = Value::Null;
    for _ in 0..130 {
        deep = json!([deep]);
    }
    assert!(crate::functions::value_snapshot::required(&deep, &amount).is_err());
}
