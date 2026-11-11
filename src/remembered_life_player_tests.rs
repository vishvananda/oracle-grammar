//! The target is remembered by the entry ability, not inferred from the leave event.
use crate::engine::{Engine, canonical_abilities};
use serde_json::Value;
fn engine() -> &'static Engine {
    crate::test_support::engine()
}
fn native() -> Vec<Value> {
    serde_json::from_str(include_str!(
        "../tests/fixtures/remembered-life-player-native.json"
    ))
    .unwrap()
}
fn cases() -> Vec<Value> {
    serde_json::from_str(include_str!(
        "../tests/fixtures/remembered-life-player-cases.json"
    ))
    .unwrap()
}
fn actual(c: &Value, off: bool) -> Result<Vec<Value>, crate::engine::Refusal> {
    let rules = engine().rule_set(|name| off && (name.starts_with("m_") || name.starts_with("t_")));
    engine()
        .parse_without(
            c["name"].as_str().unwrap(),
            c["text"].as_str().unwrap(),
            &rules,
        )
        .map(|p| p.readings)
}
#[test]
fn both_whole_originals_are_sole_native_with_wrong_old_controller_reading_retired() {
    for c in native() {
        let expected = canonical_abilities(&c["reviewed_readings"][0]).unwrap();
        assert_eq!(
            expected,
            canonical_abilities(&c["native_reference"]).unwrap()
        );
        for off in [false, true] {
            let got = actual(&c, off).unwrap();
            assert_eq!(got, vec![expected.clone()], "{}", c["name"]);
            for wrong in c["old_readings"].as_array().unwrap() {
                assert!(!got.contains(&canonical_abilities(wrong).unwrap()));
            }
        }
    }
}
#[test]
fn owned_pair_preserves_independent_parameters_and_declines_only_unresolved_readings() {
    for c in cases() {
        for off in [false, true] {
            let got = actual(&c, off);
            if c["reject"] == true {
                assert!(got.is_err(), "{}", c["text"]);
            } else {
                assert_eq!(
                    got.unwrap(),
                    vec![canonical_abilities(&c["expected"]).unwrap()],
                    "{}",
                    c["label"]
                );
            }
        }
    }
}
fn exact(name: &str, text: &str, frame: &Value) -> bool {
    engine()
        .derivation_of(name, frame, text)
        .unwrap()
        .is_some_and(|key| {
            engine()
                .print_keyed(name, frame, &["Creature".into()], &key)
                .unwrap()
                .as_deref()
                == Some(text)
        })
        || engine()
            .trail_of(name, frame, text)
            .unwrap()
            .is_some_and(|trail| {
                engine()
                    .print_trail(name, frame, &trail)
                    .unwrap()
                    .as_deref()
                    == Some(text)
            })
}
#[test]
fn original_exact_and_default_are_separate_and_reparse_without_wrong_alternate() {
    for c in native() {
        let name = c["name"].as_str().unwrap();
        let text = c["text"].as_str().unwrap();
        let frame = canonical_abilities(&c["reviewed_readings"][0]).unwrap();
        assert!(exact(name, text, &frame));
        for types in [vec![], vec!["Creature".to_owned()]] {
            let printed = engine()
                .print_keyed(name, &frame, &types, &[])
                .unwrap()
                .unwrap();
            assert_eq!(
                engine().parse(name, &printed).unwrap().readings,
                vec![frame.clone()]
            );
            assert!(printed.contains("that player gains"), "{printed}");
        }
    }
}
#[test]
fn synthesized_different_loss_gain_and_target_constraints_print_current_operands() {
    let data = cases();
    let c = data
        .iter()
        .find(|c| c["label"] == "independent-amounts-and-opponent")
        .unwrap();
    let frame = canonical_abilities(&c["expected"]).unwrap();
    let text = engine()
        .print_keyed("Example", &frame, &["Creature".into()], &[])
        .unwrap()
        .unwrap();
    assert!(text.contains("target opponent loses 7 life"), "{text}");
    assert!(text.contains("that player gains 2 life"), "{text}");
    assert_eq!(
        engine().parse("Example", &text).unwrap().readings,
        vec![frame]
    );
}
