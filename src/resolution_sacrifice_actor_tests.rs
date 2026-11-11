//! Statement-level source proof is independent of same-IR reparse and print style.
use crate::engine::{Engine, canonical_abilities};
use serde_json::{Value, json};
fn engine() -> &'static Engine {
    crate::test_support::engine()
}
fn cases() -> Vec<Value> {
    serde_json::from_str(include_str!(
        "../tests/fixtures/resolution-sacrifice-actors.json"
    ))
    .unwrap()
}
fn expected(c: &Value) -> Vec<Value> {
    let mut frames: Vec<_> = c["expected_readings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| canonical_abilities(f).unwrap())
        .collect();
    frames.sort_by_key(Value::to_string);
    frames.dedup();
    frames
}
fn check(c: &Value, mined_off: bool) {
    let name = c["name"].as_str().unwrap();
    let text = c["text"].as_str().unwrap();
    let off = engine().rule_set(|n| mined_off && (n.starts_with("m_") || n.starts_with("t_")));
    let actual = engine().parse_without(name, text, &off);
    if c["reject"] == true {
        assert!(actual.is_err(), "{text}");
        return;
    }
    let mut actual = actual.unwrap().readings;
    actual.sort_by_key(Value::to_string);
    // Equality is mandatory: a new tagged actor cannot hide an untagged legacy
    // Source/Before reading. Both Entwine branches are compared in full.
    assert_eq!(
        actual,
        expected(c),
        "{} (mined_off={mined_off})",
        c["label"]
    );
}
#[test]
fn actual_implicit_subjects_have_no_surviving_legacy_actor_reading() {
    for c in cases()
        .iter()
        .filter(|c| c["label"].as_str().unwrap().starts_with("implicit-"))
    {
        check(c, false);
        check(c, true);
    }
}
#[test]
fn explicit_actor_and_bare_versus_captured_property_remain_distinct() {
    for c in cases().iter().filter(|c| c["kind"] == "explicit") {
        check(c, false);
        check(c, true);
    }
}
#[test]
fn failed_noun_implication_never_selects_an_older_overlapping_kind() {
    for c in cases().iter().filter(|c| {
        c["label"] == "artifact-earlier-creature"
            || c["label"] == "creature-earlier-artifact"
            || c["label"].as_str().unwrap().starts_with("conjunction-")
    }) {
        check(c, false);
        check(c, true);
    }
}
#[test]
fn prefix_target_and_result_namespaces_and_edited_operands_are_complete() {
    for c in cases().iter().filter(|c| {
        c["label"] == "prefixed-target-result" || c["label"] == "edited-implicit-toughness"
    }) {
        check(c, false);
        check(c, true);
    }
}
#[test]
fn holder_creator_fixed_object_and_activation_cost_controls_keep_all_old_readings() {
    for c in cases()
        .iter()
        .filter(|c| c["kind"] == "holder" || c["kind"] == "preserve")
    {
        check(c, false);
        if c["kind"] == "holder" {
            check(c, true);
        }
    }
}
fn native_print(name: &str, value: &Value, property: &str) {
    for types in [vec![], vec!["Instant".to_owned()]] {
        let text = engine()
            .print_keyed(name, value, &types, &[])
            .unwrap()
            .unwrap();
        // A round trip alone used to accept It/its with two different subjects.
        // Require an explicit spell actor AND an independently captured noun.
        assert!(text.contains("Grab the Reins deals"), "{text}");
        assert!(
            text.contains(&format!("that creature's {property}"))
                || text.contains(&format!("the sacrificed creature's {property}")),
            "{text}"
        );
        assert_eq!(
            engine().parse(name, &text).unwrap().readings,
            vec![value.clone()]
        );
    }
}
#[test]
fn typed_and_untyped_synthesized_native_actor_never_uses_receipt_pronouns() {
    let all = cases();
    let native = &all[0];
    let frame = expected(native).remove(0);
    native_print("Grab the Reins", &frame, "power");
    let edited = all
        .iter()
        .find(|c| c["label"] == "edited-implicit-toughness")
        .unwrap();
    let mut synthesized = edited["expected_readings"][0].clone();
    for branch in ["paid", "unpaid"] {
        synthesized[0]["args"][branch]["args"]["announcement"]["modes"]["options"][1]["body"]["instructions"]
            [1]["args"]["operation"]["args"]["source"] = json!({"kind":"Source"});
    }
    let synthesized = canonical_abilities(&synthesized).unwrap();
    native_print("Grab the Reins", &synthesized, "toughness");
}
#[test]
fn synthesized_before_actor_and_property_print_one_coherent_subject() {
    for c in cases()
        .iter()
        .filter(|c| c["label"] == "implicit-it" || c["label"] == "edited-implicit-toughness")
    {
        let frame = expected(c).remove(0);
        for types in [vec![], vec!["Instant".to_owned()]] {
            let text = engine()
                .print_keyed("Grab the Reins", &frame, &types, &[])
                .unwrap()
                .unwrap();
            assert!(!text.contains("Grab the Reins deals"), "{text}");
            assert!(
                text.contains("it deals")
                    || text.contains("It deals")
                    || text.contains("creature deals")
                    || text.contains("permanent deals"),
                "{text}"
            );
            assert_eq!(
                engine().parse("Grab the Reins", &text).unwrap().readings,
                vec![frame.clone()]
            );
        }
    }
}
#[test]
fn exact_native_source_wording_remains_independent_of_default_printing() {
    let c = &cases()[0];
    let frame = expected(c).remove(0);
    let source = c["text"].as_str().unwrap();
    let exact = engine()
        .derivation_of("Grab the Reins", &frame, source)
        .unwrap()
        .is_some_and(|key| {
            engine()
                .print_keyed("Grab the Reins", &frame, &[], &key)
                .unwrap()
                .as_deref()
                == Some(source)
        });
    let exact = exact
        || engine()
            .trail_of("Grab the Reins", &frame, source)
            .unwrap()
            .is_some_and(|trail| {
                engine()
                    .print_trail("Grab the Reins", &frame, &trail)
                    .unwrap()
                    .as_deref()
                    == Some(source)
            });
    assert!(exact);
}
#[test]
fn whole_input_remains_required() {
    let c = cases()
        .into_iter()
        .find(|c| c["label"] == "whole-input-required")
        .unwrap();
    check(&c, false);
    check(&c, true);
}
