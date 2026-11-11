//! Public Engine checks distinguish actual target contrasts from preserved fallback roles.
use crate::engine::{Engine, canonical_abilities};
use serde_json::{Value, json};
use std::sync::OnceLock;
fn engine() -> &'static Engine {
    static E: OnceLock<Engine> = OnceLock::new();
    E.get_or_init(|| Engine::load().unwrap())
}
fn data() -> Value {
    serde_json::from_str(include_str!(
        "../tests/fixtures/target-contrast-reviewed.json"
    ))
    .unwrap()
}
fn exact_set(name: &str, text: &str, expected: &[Value]) {
    let mut canonical = Vec::new();
    for value in expected {
        let value = canonical_abilities(value).unwrap();
        if !canonical.contains(&value) {
            canonical.push(value);
        }
    }
    let expected = canonical;
    let got = engine().parse(name, text).unwrap().readings;
    assert_eq!(got.len(), expected.len(), "{name}: {text}");
    assert!(got.iter().all(|v| expected.contains(v)), "{name}: {got:?}");
}
#[test]
fn target_contrast_whole_originals_are_sole_complete_native() {
    for c in data()["originals"].as_array().unwrap() {
        exact_set(
            c["name"].as_str().unwrap(),
            c["text"].as_str().unwrap(),
            &[c["expected"].clone()],
        );
    }
}
#[test]
fn target_contrast_ten_reviewed_retirements_change_only_the_proved_reference() {
    for c in data()["reviewed"].as_array().unwrap() {
        exact_set(
            c["name"].as_str().unwrap(),
            c["text"].as_str().unwrap(),
            c["expected_all"].as_array().unwrap(),
        );
        let wrong = canonical_abilities(&c["old_wrong"]).unwrap();
        assert!(
            !engine()
                .parse(c["name"].as_str().unwrap(), c["text"].as_str().unwrap())
                .unwrap()
                .readings
                .contains(&wrong),
            "{}",
            c["name"]
        );
    }
}
#[test]
fn target_contrast_preserves_all_three_spell_union_forms_and_five_pump_move_roles() {
    for c in data()["protected"].as_array().unwrap() {
        exact_set(
            c["name"].as_str().unwrap(),
            c["text"].as_str().unwrap(),
            c["expected_all"].as_array().unwrap(),
        );
    }
}
#[test]
fn target_contrast_costs_future_announcements_holder_and_leaving_epoch_are_unchanged() {
    for c in data()["roles"].as_array().unwrap() {
        exact_set(
            c["name"].as_str().unwrap(),
            c["text"].as_str().unwrap(),
            c["expected_all"].as_array().unwrap(),
        );
    }
}
#[test]
fn target_contrast_owned_receipt_spans_work_without_mined_rules() {
    let d = data();
    let off = engine().rule_set(|r| r.starts_with("m_") || r.starts_with("t_"));
    for c in d["originals"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let original = c["text"].as_str().unwrap();
        let (text, expected) = if name == "Hotel of Fears" {
            (
                original.split('\n').next().unwrap(),
                json!([c["expected"][0]]),
            )
        } else {
            (original, c["expected"].clone())
        };
        assert_eq!(
            engine().parse_without(name, text, &off).unwrap().readings,
            vec![canonical_abilities(&expected).unwrap()]
        );
    }
}
fn printing(name: &str, text: &str, expected: &Value, types: &[String]) {
    let expected = canonical_abilities(expected).unwrap();
    let key = engine().derivation_of(name, &expected, text).unwrap();
    let keyed = key.and_then(|k| engine().print_keyed(name, &expected, types, &k).unwrap());
    let exact = if keyed.as_deref() == Some(text) {
        keyed
    } else {
        engine()
            .trail_of(name, &expected, text)
            .unwrap()
            .and_then(|k| engine().print_trail(name, &expected, &k).unwrap())
    };
    assert_eq!(exact.as_deref(), Some(text));
    let printed = engine()
        .print_keyed(name, &expected, types, &[])
        .unwrap()
        .expect("default print");
    assert!(
        engine()
            .parse(name, &printed)
            .unwrap()
            .readings
            .contains(&expected),
        "{printed}"
    );
}
#[test]
fn target_contrast_original_and_edited_receipt_print_current_operands() {
    let d = data();
    for c in d["originals"].as_array().unwrap() {
        let types: Vec<_> = c["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_owned())
            .collect();
        printing(
            c["name"].as_str().unwrap(),
            c["text"].as_str().unwrap(),
            &c["expected"],
            &types,
        );
    }
    let c = &d["originals"][0];
    let mut expected = json!([c["expected"][0]]);
    expected[0]["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["duration"] =
        json!({"UntilEndOfNextTurn":{"kind":"AbilityController"}});
    let text = "At the beginning of your upkeep, exile the top card of your library. You lose life equal to its mana value. You may play that card until the end of your next turn.";
    printing("Receipt Example", text, &expected, &["Artifact".into()]);
}
