use super::*;

fn eager_pair() -> Value {
    json!({"instructions":[
        {"kind":"Effect","args":{"result":0,"operation":{"kind":"LoseLife","args":{
            "players":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}},
            "amount":{"kind":"AnnouncedX"}}}}},
        {"kind":"Effect","args":{"result":1,"operation":{"kind":"GainLife","args":{
            "players":{"kind":"One","args":{"kind":"AbilityController"}},
            "amount":{"kind":"AnnouncedX"}}}}}
    ]})
}

fn mana_value(object: Value) -> Value {
    json!({"kind":"ObjectProperty","args":{
        "object":object,"property":"ManaValue","information":"CurrentOrLastKnown"}})
}

#[test]
fn shared_party_mutation_and_event_mana_values_have_one_generated_definition() {
    let definitions = [
        json!({"kind":"PartySize","args":{"kind":"AbilityController"}}),
        json!({"kind":"TimesMutated","args":{"kind":"Source"}}),
        mana_value(json!({"kind":"EventSubject"})),
        mana_value(json!({"$sym":"qualified_object","args":["Permanent",{
            "kind":"Subtype","args":"Saga"}],"at":20})),
    ];
    for value in definitions {
        assert!(shared_definition(&value));
        let captured = snapshot(&eager_pair(), &value, 40).unwrap();
        let instructions = captured["instructions"].as_array().unwrap();
        assert_eq!(instructions.len(), 3);
        assert_eq!(instructions[0]["args"]["result"]["$sym"], "snapshot_next");
        assert_eq!(
            instructions[0]["args"]["operation"]["args"]["value"]["kind"],
            value["kind"]
        );
        for instruction in &instructions[1..] {
            assert_eq!(
                instruction["args"]["operation"]["args"]["amount"],
                json!({"$sym":"snapshot_ref","args":[40],"at":40})
            );
        }
    }
}

#[test]
fn other_object_properties_do_not_borrow_the_event_mana_contract() {
    let mut power = mana_value(json!({"kind":"EventSubject"}));
    power["args"]["property"] = json!("Power");
    for value in [
        power,
        mana_value(json!({"kind":"Source"})),
        mana_value(json!({"$sym":"qualified_object","args":["Permanent",{
            "kind":"CardType","args":"Creature"}],"at":20})),
    ] {
        assert!(!shared_definition(&value));
    }
}

#[test]
fn eager_definition_extension_does_not_expand_the_paired_pump_contract() {
    let instructions = [0, 1].map(|_| {
        json!({"kind":"Effect","args":{
        "operation":{"kind":"CreateContinuousEffect","args":{
            "evaluation":"Snapshot","duration":"EndOfTurn","changes":[{
                "kind":"Pump","args":{"power":{"kind":"AnnouncedX"},
                    "toughness":{"kind":"Constant","args":0}}}]}}}})
    });
    let body = json!({"instructions":instructions});
    assert!(paired_pumps(&body));
    for value in [
        json!({"kind":"PartySize","args":{"kind":"AbilityController"}}),
        json!({"kind":"TimesMutated","args":{"kind":"Source"}}),
        mana_value(json!({"kind":"EventSubject"})),
    ] {
        assert!(snapshot(&body, &value, 40).is_err());
    }
    assert!(snapshot(&body, &json!({"kind":"Constant","args":3}), 40).is_ok());
}
