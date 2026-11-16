//! Resolving untap choices remain distinct from announced targets.
use super::*;

#[test]
fn resolving_untaps_print_changed_population_and_count() {
    let mut frame = compositional("Untap up to two artifacts.");
    assert!(
        frame[0]["args"]["announcement"]["targets"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let body = &mut frame[0]["args"]["body"]["instructions"];
    let choice_id = body[0]["args"]["result"].clone();
    assert_eq!(
        body[1]["args"]["operation"]["args"]["objects"],
        json!({"kind":"Selected","args":choice_id})
    );
    body[0]["args"]["operation"]["args"]["objects"]["args"]["filter"] =
        json!({"kind":"CardType","args":"Creature"});
    body[0]["args"]["operation"]["args"]["count"] = json!({"UpTo":{"kind":"Constant","args":3}});
    let text = printed_types(&frame, &["Sorcery"]).to_lowercase();
    assert!(text.contains("creatures"), "{text}");
    assert!(text.contains("three") || text.contains('3'), "{text}");
}

#[test]
fn optional_untap_targets_are_announced_without_a_resolving_choice() {
    let frame = compositional("Untap up to one target creature.");
    let targets = frame[0]["args"]["announcement"]["targets"]
        .as_array()
        .unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(
        targets[0]["count"],
        json!({"UpTo":{"kind":"Constant","args":1}})
    );
    let mut choices = Vec::new();
    visit(&frame, "ChooseObjects", &mut choices);
    assert!(choices.is_empty());
}

#[test]
fn full_optional_target_cards_retain_every_correct_reading() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/optional-untap-targets.json"
    ))
    .unwrap();
    for case in fixtures["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut actual = engine()
            .parse(name, case["text"].as_str().unwrap())
            .unwrap()
            .readings;
        let mut expected: Vec<_> = case["expected_readings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|reading| canonical_abilities(reading).unwrap())
            .collect();
        actual.sort_by_key(Value::to_string);
        expected.sort_by_key(Value::to_string);
        assert_eq!(actual, expected, "{name}");
    }
}

#[test]
fn temporary_characteristic_changes_print_the_edited_operands() {
    let mut frame = compositional(
        "Target creature loses flying and has base power and toughness 2/2 until end of turn.",
    );
    let changes =
        &mut frame[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"];
    changes[0]["args"]["args"]["form"] = json!("Trample");
    changes[1]["args"]["power"] = json!({"kind":"Constant","args":4});
    changes[1]["args"]["toughness"] = json!({"kind":"Constant","args":5});
    let text = printed_types(&frame, &["Instant"]).to_lowercase();
    assert!(text.contains("trample"), "{text}");
    assert!(text.contains("4/5"), "{text}");
    assert!(!text.contains("flying"), "{text}");
}
