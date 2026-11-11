use super::*;

#[test]
fn local_optional_exile_uses_its_receipt_instead_of_a_linked_query() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/shared-clauses-18.json"
    ))
    .unwrap();
    let case = &cases[0];
    let engine = crate::test_support::engine();
    let actual = engine
        .parse(
            case["name"].as_str().unwrap(),
            case["text"].as_str().unwrap(),
        )
        .unwrap();
    assert_eq!(actual.readings.len(), 1);
    assert_eq!(
        actual.readings[0],
        crate::engine::canonical_abilities(&case["expected"]).unwrap()
    );
}

fn native(index: usize) -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/linked-card-action-native.json"
    ))
    .unwrap();
    fixture["cases"][index]["expected"].clone()
}
fn site(index: usize) -> Site {
    if index == 0 {
        Site {
            ability: 2,
            returned: 0,
            producer: None,
        }
    } else {
        Site {
            ability: 0,
            returned: 1,
            producer: Some(0),
        }
    }
}
fn literal(index: usize) -> Value {
    let mut value = native(index);
    *objects_mut(&mut value, &site(index)).unwrap() =
        json!({"$sym":MARKER,"args":[guarded(scalar())],"at":10});
    value
}
fn lowered(mut value: Value) -> Value {
    let sites = prepare(&mut value).unwrap();
    lower(&mut value, &sites).unwrap();
    value
}
fn unchanged(value: Value) {
    let mut stripped = value.clone();
    strip(&mut stripped, 0, &mut 131_072).unwrap();
    assert!(
        typed(&stripped).unwrap(),
        "negative guard input must be native-typed"
    );
    assert_eq!(lowered(value), stripped);
}
#[test]
fn both_raw_literal_routes_use_the_existing_actual_definitions_without_scope_relocation() {
    for index in 0..2 {
        assert_eq!(lowered(literal(index)), native(index));
        let mut value = native(index);
        *objects_mut(&mut value, &site(index)).unwrap() = json!({"kind":"One","args":{"$sym":"remember_object","args":[scalar()["args"]],"at":10}});
        assert_eq!(prepare(&mut value).unwrap().len(), 1);
        let sites = [site(index)];
        // This unit starts already bound, so resolve only the known reference.
        *objects_mut(&mut value, &site(index)).unwrap() = scalar();
        lower(&mut value, &sites).unwrap();
        assert_eq!(value, native(index));
    }
}
#[test]
fn projections_require_the_identical_parent_proof_current_result_phase_and_whole_group() {
    for index in 0..2 {
        let value = native(index);
        let projected = projection(&value).unwrap().unwrap();
        assert_eq!(
            returned(&projected, &site(index)).unwrap()["args"]["objects"],
            if index == 0 {
                scalar()
            } else {
                guarded(scalar())
            }
        );
    }
    for operand in [
        guarded(json!({"kind":"Transition","args":{"result":0,"phase":"Before"}})),
        guarded(json!({"kind":"Transition","args":{"result":9,"phase":"After"}})),
        guarded(
            json!({"kind":"One","args":{"kind":"Result","args":{"result":0,"phase":"After","index":0}}}),
        ),
    ] {
        let mut value = native(1);
        *objects_mut(&mut value, &site(1)).unwrap() = operand;
        assert!(projection(&value).unwrap().is_none());
    }
}
#[test]
fn linked_entry_requires_unique_own_card_selection_and_same_holder() {
    let mut foreign = literal(0);
    foreign[0]["args"]["event"]["ZoneChange"]["objects"]["args"] = json!({"kind":"GrantingObject"});
    unchanged(foreign);
    let mut twice = literal(0);
    let entry = twice[0].clone();
    twice.as_array_mut().unwrap().push(entry);
    unchanged(twice);
    for (field, operand) in [
        ("count", json!({"AsManyAsCan":{"kind":"Constant","args":2}})),
        (
            "objects",
            json!({"kind":"Zone","args":{"scope":{"Players":{"zone":"Graveyard","players":{"kind":"One","args":controller()}}},"kind":"Card","filter":{"kind":"Any"}}}),
        ),
    ] {
        let mut value = literal(0);
        value[0]["args"]["body"]["instructions"][0]["args"]["body"]["instructions"][0]["args"]["operation"]
            ["args"][field] = operand;
        unchanged(value);
    }
}
#[test]
fn literal_scalar_action_without_entry_keeps_original_guard_and_no_focus_metadata() {
    let mut value = literal(0);
    value.as_array_mut().unwrap().drain(..2);
    let expected = guarded(scalar());
    let sites = prepare(&mut value).unwrap();
    lower(&mut value, &sites).unwrap();
    assert_eq!(
        value[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"],
        expected
    );
}
#[test]
fn actual_single_target_group_has_no_card_count_one_projection() {
    let mut value = literal(1);
    value[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"] =
        json!({"kind":"Targets","args":0});
    let lowered = lowered(value);
    let objects = &returned(&lowered, &site(1)).unwrap()["args"]["objects"];
    assert_eq!(objects["args"]["objects"]["kind"], "Transition");
    assert_eq!(objects["args"]["objects"]["args"]["phase"], "After");
    assert!(objects["args"]["objects"]["args"].get("index").is_none());
}
#[test]
fn activation_payment_plural_target_and_duplicate_definition_are_not_receipts() {
    let mut cost = literal(1);
    cost[0]["args"]["cost"]["parts"] = json!([{"Life":{"kind":"Constant","args":2}}]);
    unchanged(cost);
    let mut plural = literal(1);
    plural[0]["args"]["announcement"]["targets"][0]["count"]["Exactly"]["args"] = json!(2);
    unchanged(plural);
    let mut duplicate = literal(1);
    duplicate[0]["args"]["body"]["instructions"][1]["args"]["result"] = json!(0);
    unchanged(duplicate);
}
#[test]
fn optional_branch_new_trigger_and_foreign_explicit_link_do_not_borrow() {
    let mut optional = literal(1);
    let effect = optional[0]["args"]["body"]["instructions"][0].take();
    optional[0]["args"]["body"]["instructions"][0] =
        json!({"kind":"Optional","args":{"decider":controller(),"body":{"instructions":[effect]}}});
    unchanged(optional);
    let mut trigger = literal(1);
    let delayed_body =
        trigger[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["body"].clone();
    trigger[0]["args"]["body"]["instructions"][1]["args"]["operation"] = json!({"kind":"CreateTrigger","args":{
        "event":{"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}},
        "duration":"EndOfTurn","announcement":{"targets":[],"modes":null},"body":delayed_body,
        "once":false,"intervening_if":null}});
    unchanged(trigger);
    let mut foreign = literal(1);
    *objects_mut(&mut foreign, &site(1)).unwrap() = guarded(
        json!({"kind":"One","args":{"kind":"Query","args":{"objects":{"kind":"Zone","args":{"scope":{"Shared":"Exile"},"kind":"Card","filter":{"kind":"ExiledWith","args":{"kind":"Target","args":{"slot":0,"index":0}}}}},"index":0}}}),
    );
    unchanged(foreign);
}
#[test]
fn holder_and_wrong_timing_exclude_both_forward_and_inverse() {
    let value = literal(1);
    let outer = json!([{ "kind":"Static", "args":{"kind":"Ordinary","affected":{"Objects":{"kind":"One","args":{"kind":"Source"}}},"changes":[{"kind":"GrantAbility","args":value[0]}]}}]);
    unchanged(outer);
    let mut value = literal(1);
    value[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["start"] =
        json!("NextTurn");
    unchanged(value);
}
#[test]
fn deep_producer_or_entry_is_bounded_before_native_decoding() {
    let mut value = literal(0);
    let mut filter = json!({"kind":"Any"});
    for _ in 0..150 {
        filter = json!({"kind":"Not","args":filter});
    }
    value[0]["args"]["body"]["instructions"][0]["args"]["body"]["instructions"][0]["args"]["operation"]
        ["args"]["objects"]["args"]["filter"] = filter;
    assert!(prepare(&mut value).is_err());
}
