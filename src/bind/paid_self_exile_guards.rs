use super::*;

fn unchanged(mut value: Value) {
    let before = value.clone();
    apply(&mut value);
    assert_eq!(value, before);
}
fn part(value: &mut Value) -> &mut Value {
    &mut value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"][0]
}

#[test]
fn explicit_foreign_and_indexed_recipients_have_no_implicit_lexical_proof() {
    for reference in [
        json!({"kind":"Source"}),
        json!({"kind":"GrantingObject"}),
        json!({"kind":"Target","args":{"slot":0,"index":0}}),
        json!({"kind":"EventSubject"}),
        json!({"kind":"Query","args":{"objects":{"kind":"CostObjects","args":{"payment":{"Resolution":17},"component":[0],"view":"Performed","phase":"After"}},"index":0}}),
        json!({"$sym":"self","args":[],"at":10}),
        json!({"$sym":"named_source","args":[],"at":10}),
        json!({"$sym":"qualified_object","args":["Card",{"kind":"Any"}],"at":10}),
    ] {
        let mut value = raw(false);
        ret_mut(&mut value)["objects"] =
            json!({"$sym":"exile_followup","args":[{"kind":"One","args":reference}],"at":9});
        assert!(prepare(&value).unwrap().is_none());
        unchanged(value);
    }
}

#[test]
fn nonself_plural_variable_and_competing_payment_inputs_never_rebind() {
    for objects in [
        json!({"kind":"One","args":{"kind":"GrantingObject"}}),
        json!({"kind":"One","args":{"kind":"EventSubject"}}),
        json!({"kind":"One","args":{"kind":"Target","args":{"slot":0,"index":0}}}),
        json!({"kind":"Targets","args":0}),
        json!({"kind":"Union","args":[{"kind":"One","args":{"kind":"Source"}}]}),
    ] {
        let mut value = raw(false);
        part(&mut value)["Move"]["selections"][0]["objects"] = objects;
        unchanged(value);
    }
    for count in [
        json!({"Exactly":{"kind":"Constant","args":1}}),
        json!({"UpTo":{"kind":"Constant","args":1}}),
        json!({"Exactly":{"kind":"AnnouncedX"}}),
    ] {
        let mut value = raw(false);
        part(&mut value)["Move"]["selections"][0]["choose"] = count;
        unchanged(value);
    }
    for wrapper in ["Constrained", "Repeated", "Choice"] {
        let mut value = raw(false);
        let old = part(&mut value).take();
        *part(&mut value) = match wrapper {
            "Constrained" => json!({"Constrained":{"part":old,"constraints":[]}}),
            "Repeated" => json!({"Repeated":{"part":old,"times":{"kind":"Constant","args":2}}}),
            _ => json!({"Choice":[{"parts":[old]}]}),
        };
        unchanged(value);
    }
    let mut value = raw(false);
    let extra = part(&mut value).clone();
    value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"]
        .as_array_mut()
        .unwrap()
        .push(extra);
    unchanged(value);
}

#[test]
fn payment_is_optional_own_payer_one_plain_self_move_only() {
    for action in [false, true] {
        let mut value = raw(action);
        value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["optional"] =
            json!(false);
        unchanged(value);
        let mut value = raw(action);
        value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["player"] =
            json!({"kind":"Target","args":0});
        unchanged(value);
    }
    let mut value = raw(true);
    part(&mut value)["PlayerAction"]["body"]["instructions"].as_array_mut().unwrap().push(json!({"kind":"Effect","args":{"result":25,"operation":{"kind":"Draw","args":{"players":{"kind":"One","args":controller()},"amount":{"kind":"Constant","args":1}}}}}));
    unchanged(value);
    for (pointer, replacement) in [
        ("/destination/Exile/visibility", json!("FaceDown")),
        (
            "/destination/Exile/arrival/counters",
            json!([[{"Named":"ChargeCounter"},{"kind":"Constant","args":1}]]),
        ),
        ("/qualifiers", json!(["DifferentNames"])),
    ] {
        let mut value = raw(true);
        let m = &mut part(&mut value)["PlayerAction"]["body"]["instructions"][0]["args"]["operation"]
            ["args"];
        *m.pointer_mut(pointer).unwrap() = replacement;
        unchanged(value);
    }
}

#[test]
fn exact_positive_parent_guard_is_checked_after_binding() {
    for condition in [
        json!({"kind":"PaymentChosen","args":24}),
        json!({"kind":"Not","args":{"kind":"PaymentChosen","args":17}}),
    ] {
        let mut value = raw(true);
        value["args"]["body"]["instructions"][1]["args"]["condition"] = condition;
        unchanged(value);
    }
    let mut value = raw(false);
    value["args"]["body"]["instructions"][1]["args"]["otherwise"]["instructions"] =
        json!([{"kind":"Effect"}]);
    unchanged(value);
    let mut value = raw(true);
    let note = prepare(&value).unwrap();
    part(&mut value)["PlayerAction"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"]
        ["args"] = json!({"kind":"GrantingObject"});
    let before = value.clone();
    lower(&mut value, note).unwrap();
    assert_eq!(value, before);
}

#[test]
fn foreign_holders_modes_groups_and_outer_branches_are_not_traversed() {
    let value = raw(false);
    for outer in [
        json!({"kind":"GrantAbility","args":value.clone()}),
        json!({"$sym":"holder","args":[value.clone()],"at":0}),
        json!({"kind":"ChooseOne","args":{"options":[value.clone()]}}),
        json!({"kind":"If","args":{"then":{"instructions":[value.clone()]}}}),
        json!({"kind":"ForEachPlayer","args":{"body":{"instructions":[value.clone()]}}}),
    ] {
        unchanged(outer);
    }
    let mut grouped = value.clone();
    let items = grouped["args"]["body"]["instructions"].take();
    grouped["args"]["body"]["instructions"] =
        json!([{"kind":"Group","args":{"instructions":items}}]);
    unchanged(grouped);
    let mut value = raw(false);
    value["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"Effect"}));
    unchanged(value);
}

#[test]
fn extra_delayed_actions_wrong_timing_and_empty_capture_scope_are_not_changed() {
    for case in 0..5 {
        let mut value = raw(false);
        let d = &mut value["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0]["args"]
            ["operation"]["args"];
        match case {
            0 => d["body"]["instructions"]
                .as_array_mut()
                .unwrap()
                .push(json!({"kind":"Effect"})),
            1 => d["event"]["Turn"]["point"] = json!("UpkeepBegins"),
            2 => d["start"] = json!("NextTurn"),
            3 => {
                d["intervening_if"] = json!({"kind":"Compare","args":{"left":{"kind":"Constant","args":1},"test":{"kind":"Compare","args":{"comparison":"Equal","value":{"kind":"Constant","args":1}}}}})
            }
            _ => d["expires"] = json!("EndOfTurn"),
        }
        unchanged(value);
    }
    let mut value = raw(false);
    let delay = value["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0].clone();
    value["args"]["body"]["instructions"][1]["args"]["then"]["instructions"]
        .as_array_mut()
        .unwrap()
        .push(delay);
    unchanged(value);
}

#[test]
fn prefix_result_namespaces_and_unchanged_readonly_operands_are_preserved() {
    let mut value = raw(true);
    let prefix = json!({"kind":"Effect","args":{"result":4,"operation":{"kind":"BindValue","args":{"value":{"kind":"ObjectProperty","args":{"object":{"kind":"EventSubject"},"property":"Power","information":"CurrentOrLastKnown"}}}}}});
    value["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .insert(0, prefix.clone());
    let note = prepare(&value).unwrap();
    assert!(note.is_some());
    lower(&mut value, note).unwrap();
    assert_eq!(value["args"]["body"]["instructions"][0], prefix);
    let objects = &value["args"]["body"]["instructions"][2]["args"]["then"]["instructions"][0]["args"]
        ["operation"]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"];
    assert_eq!(
        objects["args"]["objects"]["args"]["payment"],
        json!({"Resolution":17})
    );
    assert_eq!(objects["args"]["objects"]["args"]["component"], json!([0]));
    let mut collision = raw(true);
    collision["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0]["args"]["result"] =
        json!(17);
    unchanged(collision);
}

#[test]
fn lexical_envelope_and_resource_bounds_fail_closed() {
    let mut value = raw(false);
    ret_mut(&mut value)["objects"]["args"][0]["args"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"One","args":{"kind":"Source"}}));
    unchanged(value);
    assert!(bounded(&json!("x".repeat(262_144))).is_err());
}
