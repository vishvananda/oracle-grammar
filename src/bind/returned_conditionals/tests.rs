use super::*;
use crate::engine::canonical_abilities;
fn fixtures() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/returned-conditionals-native.json"
    ))
    .unwrap()
}
fn lower_frame(frame: &Value) -> Value {
    let mut frame = frame.clone();
    let ability = &mut frame[0];
    ability["args"]["body"]["instructions"][2][META] = json!(true);
    let mut scope = crate::bind::scope_for(ability, "Example", 0);
    lower(ability, &mut scope).unwrap();
    frame
}
#[test]
fn both_old_essence_readings_and_hallowed_partition_before_actions() {
    for case in fixtures()["cases"].as_array().unwrap() {
        for old in case["old_readings"].as_array().unwrap() {
            crate::engine::validate_abilities(old).unwrap();
            let output = lower_frame(old);
            assert_eq!(
                canonical_abilities(&output).unwrap(),
                canonical_abilities(&case["expected"]).unwrap()
            );
            let items = output[0]["args"]["body"]["instructions"][2]["args"]["instructions"]
                .as_array()
                .unwrap();
            assert_eq!(items[0]["args"]["operation"]["kind"], "NoteObjects");
            if case["name"] == "Hallowed Respite" {
                assert_eq!(items[1]["args"]["operation"]["kind"], "NoteObjects");
                assert_eq!(items[2]["args"]["operation"]["kind"], "PutCounters");
                assert_eq!(items[3]["args"]["operation"]["kind"], "SetTapped");
            }
        }
    }
}
#[test]
fn unmarked_explicit_refs_dynamic_counters_and_unrelated_sources_stay_scalar() {
    let f = fixtures();
    let old = &f["cases"][0]["old_readings"][1];
    let mut unmarked = old.clone();
    let mut scope = crate::bind::scope_for(&unmarked[0], "Example", 0);
    lower(&mut unmarked[0], &mut scope).unwrap();
    assert_eq!(unmarked, *old);
    for replacement in [
        json!({"kind":"AnnouncedX"}),
        json!({"kind":"Constant","args":1}),
    ] {
        let mut input = old.clone();
        let args = &mut input[0]["args"]["body"]["instructions"][2]["args"];
        args["then"]["instructions"][0]["args"]["operation"]["args"]["placements"][0]["amount"] =
            replacement;
        args["then"]["instructions"][0]["args"]["operation"]["args"]["placements"][0]["recipients"]
            ["args"]["args"] = json!({"kind":"Source"});
        let output = lower_frame(&input);
        assert_eq!(output, input);
    }
    let mut input = old.clone();
    input[0]["args"]["body"]["instructions"][2]["args"]["then"]["instructions"][0]["args"]["operation"]
        ["args"]["placements"][0]["recipients"]["args"]["args"]["args"]["index"] = json!(1);
    assert_eq!(lower_frame(&input), input);
}
#[test]
fn inverse_requires_exact_current_partitions_constant_actions_and_ids() {
    let f = fixtures();
    let output = lower_frame(&f["cases"][1]["old_readings"][0]);
    let group = &output[0]["args"]["body"]["instructions"][2];
    let items = group["args"]["instructions"].as_array().unwrap();
    let projected = inverse::project(&json!(items)).unwrap();
    assert_eq!(
        projected[0]["args"]["condition"]["args"]["object"],
        operands::scalar(1)
    );
    let mut edited = items.clone();
    edited[2]["args"]["operation"]["args"]["placements"][0]["amount"] =
        json!({"kind":"Constant","args":3});
    let projected = inverse::project(&json!(edited)).unwrap();
    assert_eq!(
        projected[0]["args"]["then"]["instructions"][0]["args"]["operation"]["args"]["placements"]
            [0]["amount"]["args"],
        3
    );
    let mut wrong = items.clone();
    wrong[1]["args"]["operation"]["args"]["objects"]["args"]["excluding"]["args"] = json!(999);
    assert!(inverse::project(&json!(wrong)).is_none());
    let mut wrong = items.clone();
    wrong.swap(1, 2);
    assert!(inverse::project(&json!(wrong)).is_none());
    let mut wrong = items.clone();
    wrong[3]["args"]["operation"]["args"]["objects"] = json!({"kind":"Selected","args":999});
    assert!(inverse::project(&json!(wrong)).is_none());
}
#[test]
fn lexical_annotation_does_not_hide_leading_if_or_mark_explicit_targets() {
    let it = json!({"$sym":"it","args":[],"at":3});
    let condition = json!({"kind":"ObjectMatches","args":{"object":it,"filter":{"kind":"Subtype","args":"Spirit"}}});
    let tap = json!({"kind":"Effect","args":{"result":{"$sym":"next","args":[],"at":7},"operation":{"kind":"SetTapped","args":{"objects":{"kind":"One","args":it},"tapped":true}}}});
    let instruction = json!({"kind":"If","args":{"condition":condition,"then":{"instructions":[tap]},"otherwise":{"instructions":[]}}});
    let wrapped = json!([{"$sym":"leading_if","args":[instruction],"at":0}]);
    let marked = annotate(&wrapped, false).unwrap();
    assert_eq!(marked[0]["$sym"], "leading_if");
    assert_eq!(marked[0]["args"][0]["kind"], "If");
    assert_eq!(marked[0]["args"][0][META], true);
    let mut trigger = json!({"kind":"Triggered","args":{"intervening_if":null,"body":{"instructions":marked.clone()}}});
    assert!(crate::bind::condition_placement(&mut trigger).is_err());
    let mut explicit = wrapped;
    explicit[0]["args"][0]["args"]["condition"]["args"]["object"] =
        json!({"kind":"Target","args":{"slot":0,"index":0}});
    assert_eq!(annotate(&explicit, false).unwrap(), explicit);
}

#[test]
fn actual_engine_load_originals_include_exactly_the_published_partition_frames() {
    let engine = crate::test_support::engine();
    for c in fixtures()["cases"].as_array().unwrap() {
        assert_eq!(
            engine
                .parse(
                    c["name"].as_str().unwrap(),
                    &engine.presentation_free(c["text"].as_str().unwrap())
                )
                .unwrap()
                .readings,
            vec![canonical_abilities(&c["expected"]).unwrap()],
            "{}",
            c["name"]
        );
    }
}

#[test]
fn action_amount_filter_properties_and_explicit_indices_have_separate_boundaries() {
    let f = fixtures();
    let old = &f["cases"][0]["old_readings"][1];
    let mut dynamic = old.clone();
    dynamic[0]["args"]["body"]["instructions"][2]["args"]["then"]["instructions"][0]["args"]["operation"]
        ["args"]["placements"][0]["amount"] = json!({"kind":"AnnouncedX"});
    assert_eq!(lower_frame(&dynamic), dynamic);
    let mut property = old.clone();
    property[0]["args"]["body"]["instructions"][2]["args"]["condition"] = json!({"kind":"Compare","args":{"left":{"kind":"ObjectProperty","args":{"object":{"kind":"Result","args":{"result":1,"phase":"After","index":0}},"property":"Power","information":"CurrentOrLastKnown"}},"test":{"kind":"Compare","args":{"comparison":"Greater","value":{"kind":"Constant","args":2}}}}});
    assert_eq!(lower_frame(&property), property);
    let mut selected = old.clone();
    selected[0]["args"]["body"]["instructions"][2]["args"]["then"]["instructions"][0]["args"]["operation"]
        ["args"]["placements"][0]["recipients"]["args"] = json!({"kind":"Selected","args":0});
    assert_eq!(lower_frame(&selected), selected);
}

#[test]
fn scope_boundaries_do_not_inherit_the_prior_return_into_scalar_branches() {
    let f = fixtures();
    let old = &f["cases"][0]["old_readings"][1];
    let mut input = old.clone();
    let body = &mut input[0]["args"]["body"]["instructions"];
    let mut inner = body[2].clone();
    inner[META] = json!(true);
    body[2] = json!({"kind":"Optional","args":{"decider":{"kind":"AbilityController"},"body":{"instructions":[inner]}}});
    let expected = old[0]["args"]["body"]["instructions"][2].clone();
    let mut scope = crate::bind::scope_for(&input[0], "Example", 0);
    lower(&mut input[0], &mut scope).unwrap();
    assert_eq!(
        input[0]["args"]["body"]["instructions"][2]["args"]["body"]["instructions"][0],
        expected
    );
}

#[test]
fn synthesized_counter_edit_prints_current_amount_and_cannot_replay_old_a_counter() {
    let engine = crate::test_support::engine();
    let f = fixtures();
    let c = &f["cases"][0];
    let original = canonical_abilities(&c["expected"]).unwrap();
    let mut edited = original.clone();
    fn edit(v: &mut Value) {
        if v["kind"] == "PutCounters" {
            v["args"]["placements"][0]["amount"] = json!({"kind":"Constant","args":3});
        }
        match v {
            Value::Object(o) => {
                for v in o.values_mut() {
                    edit(v)
                }
            }
            Value::Array(a) => {
                for v in a {
                    edit(v)
                }
            }
            _ => {}
        }
    }
    edit(&mut edited);
    let trail = engine
        .trail_of(
            c["name"].as_str().unwrap(),
            &original,
            c["text"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap();
    assert!(
        engine
            .print_trail(c["name"].as_str().unwrap(), &edited, &trail)
            .unwrap()
            .is_none()
    );
    let printed = engine
        .print_keyed(c["name"].as_str().unwrap(), &edited, &[], &[])
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .parse(c["name"].as_str().unwrap(), &printed)
            .unwrap()
            .readings,
        vec![edited]
    );
}

#[test]
fn deep_annotation_and_inverse_are_bounded_before_cloning() {
    let mut deep = json!({"kind":"Any"});
    for _ in 0..140 {
        deep = json!({"kind":"Not","args":deep});
    }
    assert!(annotate(&deep, false).is_err());
    assert!(inverse::project(&deep).is_none());
}

#[test]
fn inverse_accepts_only_unobserved_canonical_unused_duplicates() {
    let f = fixtures();
    let output = lower_frame(&f["cases"][1]["old_readings"][0]);
    let mut items = output[0]["args"]["body"]["instructions"][2]["args"]["instructions"]
        .as_array()
        .unwrap()
        .clone();
    items[2]["args"]["result"] = json!(u32::MAX);
    items[3]["args"]["result"] = json!(u32::MAX);
    assert!(inverse::project(&json!(items)).is_some());
    let mut meaningful = items.clone();
    meaningful[0]["args"]["operation"]["args"]["objects"]["args"]["filter"] = json!({"kind":"SameObject","args":{"kind":"Result","args":{"result":u32::MAX,"phase":"After","index":0}}});
    assert!(inverse::project(&json!(meaningful)).is_none());
    let mut duplicate = items.clone();
    duplicate[2]["args"]["result"] = json!(17);
    duplicate[3]["args"]["result"] = json!(17);
    assert!(inverse::project(&json!(duplicate)).is_none());
    // MAX remains a valid meaningful native Note ID when uniquely defined.
    let mut max_note = output[0]["args"]["body"]["instructions"][2]["args"]["instructions"]
        .as_array()
        .unwrap()
        .clone();
    max_note[0]["args"]["result"] = json!(u32::MAX);
    max_note[1]["args"]["operation"]["args"]["objects"]["args"]["excluding"]["args"] =
        json!(u32::MAX);
    max_note[2]["args"]["operation"]["args"]["placements"][0]["recipients"]["args"]["args"] =
        json!(u32::MAX);
    assert!(inverse::project(&json!(max_note)).is_some());
    max_note[2]["args"]["result"] = json!(u32::MAX);
    assert!(inverse::project(&json!(max_note)).is_none());
}

#[test]
fn actual_hallowed_original_and_edited_counter_print_before_cli_relink() {
    let engine = crate::test_support::engine();
    let f = fixtures();
    let c = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Hallowed Respite")
        .unwrap();
    let name = c["name"].as_str().unwrap();
    let text = engine.presentation_free(c["text"].as_str().unwrap());
    let original = canonical_abilities(&c["expected"]).unwrap();
    assert_eq!(
        engine.parse(name, &text).unwrap().readings,
        vec![original.clone()]
    );
    assert!(
        super::inverse::project_with_prefix(&original[0]["args"]["body"]["instructions"], true)
            .is_some(),
        "canonical Hallowed inverse projection"
    );
    let trail = engine.trail_of(name, &original, &text).unwrap().unwrap();
    assert_eq!(
        engine
            .print_trail(name, &original, &trail)
            .unwrap()
            .as_deref(),
        Some(text.as_str())
    );
    let original_default = engine
        .print_keyed(name, &original, &[], &[])
        .unwrap()
        .unwrap();
    assert_eq!(
        engine.parse(name, &original_default).unwrap().readings,
        vec![original.clone()]
    );
    let mut edited = original;
    fn edit(value: &mut Value) {
        if value["kind"] == "PutCounters" {
            value["args"]["placements"][0]["amount"] = json!({"kind":"Constant","args":3});
        }
        match value {
            Value::Object(fields) => fields.values_mut().for_each(edit),
            Value::Array(values) => values.iter_mut().for_each(edit),
            _ => {}
        }
    }
    edit(&mut edited);
    assert!(engine.print_trail(name, &edited, &trail).unwrap().is_none());
    let printed = engine
        .print_keyed(name, &edited, &[], &[])
        .unwrap()
        .unwrap();
    assert_eq!(engine.parse(name, &printed).unwrap().readings, vec![edited]);
}

#[test]
fn inverse_whole_prefix_accepts_lexical_target_introduction_without_erasing_it() {
    let f = fixtures();
    let c = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Hallowed Respite")
        .unwrap();
    let original = canonical_abilities(&c["expected"]).unwrap();
    let forms = crate::bind::unbind_ability(&original[0]);
    let mut introductions = 0;
    for form in forms {
        let items = &form["args"]["body"]["instructions"];
        if items[0]["args"]["operation"]["args"]["objects"]["args"]["args"]["slot"]["$sym"]
            != "target"
        {
            continue;
        }
        introductions += 1;
        let projected =
            inverse::project_with_prefix(items, true).expect("target-introduction spelling proof");
        assert_eq!(projected[0], items[0]);
        assert_eq!(projected[1], items[1]);
        let mut malformed = items.clone();
        malformed[0]["args"]["operation"]["args"]["objects"]["args"]["args"]["slot"]["args"][0]["slot"] =
            json!({"$sym":"slot","args":[]});
        assert!(inverse::project_with_prefix(&malformed, true).is_none());
    }
    assert!(introductions > 0);
}
