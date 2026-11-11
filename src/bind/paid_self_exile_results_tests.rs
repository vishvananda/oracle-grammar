use super::*;

fn entry_counter(value: &mut Value, amount: Value) {
    ret_mut(value)["destination"]["Battlefield"]["arrival"]["counters"] =
        json!([[{"PowerToughness":[1,1]},amount]]);
}
fn property(result: u64) -> Value {
    json!({"kind":"ObjectProperty","args":{"object":{"kind":"Result","args":{"result":result,"phase":"After","index":0}},"property":"Power","information":"CurrentOrLastKnown"}})
}
fn no_new_view(mut value: Value) {
    assert!(prepare(&value).unwrap().is_some());
    let before = value.clone();
    apply(&mut value);
    assert_eq!(value, before);
}

#[test]
fn return_entry_reads_of_the_deleted_move_leave_the_input_unchanged() {
    for amount in [
        json!({"kind":"Performed","args":24}),
        property(24),
        json!({"kind":"CountObjects","args":{"kind":"Transition","args":{"result":24,"phase":"After"}}}),
        json!({"kind":"Add","args":[{"kind":"Constant","args":1},property(24)]}),
    ] {
        let mut value = raw(true);
        entry_counter(&mut value, amount);
        no_new_view(value);
    }
}

#[test]
fn a_valid_exported_move_property_cannot_dangle_after_conversion() {
    let mut value = raw(true);
    value["kind"] = json!("Triggered");
    value["args"]["event"] = fixture()["native"][1]["args"]["event"].clone();
    value["args"]["limits"] = json!({"per_turn":null,"total":null});
    value["args"]["intervening_if"] = Value::Null;
    entry_counter(&mut value, property(24));
    let mut ordinary = value.clone();
    ret_mut(&mut ordinary)["objects"] = json!({"kind":"One","args":{"kind":"EventSubject"}});
    validate_abilities(&json!([ordinary])).unwrap();
    no_new_view(value);
}

#[test]
fn prefix_result_reads_are_not_reinterpreted_as_unrelated_integer_operands() {
    for expr in [json!({"kind":"BoundValue","args":24}), property(24)] {
        let mut value = raw(true);
        value["args"]["body"]["instructions"].as_array_mut().unwrap().insert(0,
            json!({"kind":"Effect","args":{"result":5,"operation":{"kind":"BindValue","args":{"value":expr}}}}));
        no_new_view(value);
    }
}

#[test]
fn the_same_integer_in_constants_and_counter_kinds_is_not_a_result_use() {
    let mut value = raw(true);
    entry_counter(&mut value, json!({"kind":"Constant","args":24}));
    ret_mut(&mut value)["destination"]["Battlefield"]["arrival"]["counters"][0][0] =
        json!({"PowerToughness":[24,24]});
    apply(&mut value);
    assert!(
        value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"][0]
            .get("Move")
            .is_some()
    );
    assert_eq!(
        ret_mut(&mut value)["destination"]["Battlefield"]["arrival"]["counters"],
        json!([[{"PowerToughness":[24,24]},{"kind":"Constant","args":24}]])
    );
    validate_abilities(&json!([value])).unwrap();
}

#[test]
fn unrelated_live_parent_result_references_are_preserved() {
    let mut value = raw(true);
    entry_counter(&mut value, json!({"kind":"Performed","args":17}));
    apply(&mut value);
    assert!(
        value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"][0]
            .get("Move")
            .is_some()
    );
    assert_eq!(
        ret_mut(&mut value)["destination"]["Battlefield"]["arrival"]["counters"][0][1],
        json!({"kind":"Performed","args":17})
    );
}
