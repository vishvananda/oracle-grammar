//! Native modal announcements must not absorb unconditional neighboring programs.
use super::*;
use std::sync::OnceLock;

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| Engine::load().unwrap())
}
fn fixtures() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/modal-spell-neighbors-native.json"
    ))
    .unwrap()
}
fn controls() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/modal-spell-neighbors-controls.json"
    ))
    .unwrap()
}
fn expected(case: &Value) -> Value {
    validate_abilities(&case["expected"]).unwrap();
    canonical_abilities(&case["expected"]).unwrap()
}
fn text(case: &Value) -> &str {
    case["text"].as_str().unwrap()
}
fn check(case: &Value) {
    let name = case["name"].as_str().unwrap();
    let off = engine().rule_set(|n| n.starts_with("m_") || n.starts_with("t_"));
    let want = expected(case);
    for parsed in [
        engine().parse(name, text(case)),
        engine().parse_without(name, text(case), &off),
    ] {
        assert_eq!(
            parsed.unwrap().readings,
            vec![want.clone()],
            "{}",
            text(case)
        );
    }
}
#[test]
fn actual_bind_native_and_both_announcement_orders_before_cli_relink() {
    for case in fixtures()["cases"].as_array().unwrap() {
        check(case);
    }
    for case in controls()["cases"].as_array().unwrap() {
        check(case);
    }
}
#[test]
fn modal_boundaries_preserve_direct_shape_labels_and_unknown_announcements() {
    let f = fixtures();
    let modal = f["cases"][0]["expected"][0].clone();
    let ordinary = f["cases"][0]["expected"][1].clone();
    for input in [
        vec![modal.clone(), ordinary.clone()],
        vec![ordinary.clone(), modal.clone()],
        vec![modal.clone(), modal.clone()],
    ] {
        assert_eq!(merge_spells(input.clone()), input);
    }
    let wrapped = json!({"$sym":"scope","at":0,"args":[ordinary.clone()]});
    let input = vec![ordinary.clone(), wrapped];
    assert_eq!(merge_spells(input.clone()), input);
    let labelled = json!({"kind":"Keyword","args":{"kind":"Flying"}});
    let input = vec![ordinary.clone(), labelled, ordinary.clone()];
    assert_eq!(merge_spells(input.clone()), input);
    let mut symbolic = ordinary;
    symbolic["args"]["announcement"] = json!({"$sym":"announcement","at":0});
    assert_eq!(merge_spells(vec![symbolic.clone(), symbolic]).len(), 1);
}
#[test]
fn ordinary_target_namespaces_still_merge_but_modal_neighbors_do_not() {
    let c = controls();
    for (index, count) in [(0, 2), (1, 2), (2, 1)] {
        let case = &c["cases"][index];
        let readings = engine().parse("Example", text(case)).unwrap().readings;
        assert_eq!(readings, vec![expected(case)]);
        assert_eq!(readings[0].as_array().unwrap().len(), count);
    }
    let frame = expected(&c["cases"][2]);
    assert_eq!(
        frame[0]["args"]["announcement"]["targets"][1]["slot"],
        json!(1)
    );
    let prefix = expected(&c["cases"][4]);
    assert_eq!(
        prefix[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["card"]["Duplicate"]
            ["args"]["args"]["slot"],
        json!(1)
    );
}
fn exact_and_default(case: &Value) {
    let name = case["name"].as_str().unwrap();
    let ir = expected(case);
    let trail = engine().trail_of(name, &ir, text(case)).unwrap().unwrap();
    assert_eq!(
        engine().print_trail(name, &ir, &trail).unwrap().as_deref(),
        Some(text(case))
    );
    let printed = engine().print_keyed(name, &ir, &[], &[]).unwrap().unwrap();
    assert_eq!(engine().parse(name, &printed).unwrap().readings, vec![ir]);
}
#[test]
fn actual_bind_and_edited_grant_print_before_cli_relink() {
    exact_and_default(&fixtures()["cases"][0]);
    let c = controls();
    exact_and_default(&c["cases"][7]);
    let original = &c["cases"][5];
    let edited = expected(&c["cases"][7]);
    let trail = engine()
        .trail_of("Example", &expected(original), text(original))
        .unwrap()
        .unwrap();
    let replayed = engine()
        .print_trail("Example", &edited, &trail)
        .unwrap()
        .unwrap();
    assert_ne!(replayed, text(original));
    assert_eq!(
        engine().parse("Example", &replayed).unwrap().readings,
        vec![edited]
    );
}
#[test]
fn granted_holder_and_named_creator_remain_distinct_and_bound_to_duplicate() {
    let c = controls();
    for (index, actor) in [(5, "Source"), (6, "GrantingObject")] {
        let want = expected(&c["cases"][index]);
        check(&c["cases"][index]);
        let grant = &want[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"];
        assert_eq!(grant["objects"]["args"]["kind"], "Result");
        assert_eq!(grant["objects"]["args"]["args"]["phase"], "After");
        assert_eq!(grant["objects"]["args"]["args"]["index"], 0);
        assert_eq!(
            grant["changes"][0]["args"]["args"]["event"]["Object"]["objects"]["args"]["kind"],
            actor
        );
    }
}
#[test]
fn no_plural_or_selected_result_receipt_is_inferred_for_singular_duplicate() {
    let c = controls();
    let text = text(&c["cases"][3]);
    for changed in [
        text.replace("a duplicate", "two duplicates"),
        text.replace("It perpetually", "The second card perpetually"),
        text.replace("It perpetually", "Target creature perpetually"),
    ] {
        if let Ok(parsed) = engine().parse("Example", &changed) {
            assert!(!parsed.readings.contains(&expected(&c["cases"][3])));
        }
    }
}
