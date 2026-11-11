//! The infinity heading is semantic eligibility, never display or a number.
use crate::engine::{Engine, canonical_abilities};
use crate::tokens::{self, Names, Token};
use serde_json::Value;

fn engine() -> &'static Engine {
    crate::test_support::engine()
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../tests/fixtures/harnessed-turn-heading-native.json"
    ))
    .unwrap()
}
fn family() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("../families/harnessed-turn-heading.json")).unwrap()
        ["cases"]
        .as_array()
        .unwrap()
        .clone()
}
fn checked_print(name: &str, source: &str, value: &Value, types: &[String]) {
    let trail = engine().trail_of(name, value, source).unwrap().unwrap();
    assert_eq!(
        engine()
            .print_trail(name, value, &trail)
            .unwrap()
            .as_deref(),
        Some(source)
    );
    let printed = engine()
        .print_keyed(name, value, types, &[])
        .unwrap()
        .unwrap();
    assert_eq!(
        engine().parse(name, &printed).unwrap().readings,
        vec![value.clone()]
    );
    if source.contains('∞') {
        assert!(printed.contains('∞'), "{printed}");
    }
}

#[test]
fn infinity_is_a_mark_and_does_not_become_a_numeric_expression() {
    let names = Names::of("Example");
    let marks = tokens::tokenize("∞ —", &names, &Default::default()).unwrap();
    assert_eq!(marks, vec![Token::Mark('∞'), Token::Mark('—')]);
    assert_eq!(
        tokens::tokenize("2", &names, &Default::default()).unwrap(),
        vec![Token::Number(2)]
    );
    assert!(engine().parse("Example", "Draw ∞ cards.").is_err());
    assert!(engine().parse("Example", "{∞}: Draw a card.").is_err());
}

#[test]
fn complete_real_infinity_original_is_sole_native_with_event_time_gate() {
    let data = fixture();
    let name = data["name"].as_str().unwrap();
    let source = data["text"].as_str().unwrap();
    let expected = canonical_abilities(&data["native_source"]["abilities"]).unwrap();
    assert_eq!(expected, canonical_abilities(&data["expected"]).unwrap());
    let parsed = engine().parse(name, source).unwrap();
    assert_eq!(parsed.readings, vec![expected.clone()]);
    let trigger = expected.as_array().unwrap().last().unwrap();
    assert_eq!(trigger["kind"], "Triggered");
    assert_eq!(trigger["args"]["intervening_if"], Value::Null);
    assert_eq!(
        trigger["args"]["event"]["Filtered"]["condition"],
        serde_json::json!({"kind":"ObjectMatches","args":{
            "object":{"kind":"Source"},"filter":{"kind":"State","args":"Harnessed"}}})
    );
    checked_print(name, source, &expected, &["Artifact".into()]);
}

#[test]
fn header_removal_changes_meaning_and_edited_operands_print_current_values() {
    let data = fixture();
    let name = data["name"].as_str().unwrap();
    let source = data["text"].as_str().unwrap();
    let expected = canonical_abilities(&data["expected"]).unwrap();
    let removed = source.replace("∞ — ", "");
    let mut plain = data["expected"].clone();
    let trigger = plain.as_array_mut().unwrap().last_mut().unwrap();
    trigger["args"]["event"] = trigger["args"]["event"]["Filtered"]["event"].clone();
    let plain = canonical_abilities(&plain).unwrap();
    assert_ne!(plain, expected);
    assert_eq!(
        engine().parse(name, &removed).unwrap().readings,
        vec![plain]
    );
    let edited = canonical_abilities(&data["edited"]["expected"]).unwrap();
    assert_ne!(edited, expected);
    let trail = engine().trail_of(name, &expected, source).unwrap().unwrap();
    if let Some(text) = engine().print_trail(name, &edited, &trail).unwrap() {
        assert_eq!(
            engine().parse(name, &text).unwrap().readings,
            vec![edited.clone()]
        );
    }
    checked_print(
        name,
        data["edited"]["text"].as_str().unwrap(),
        &edited,
        &["Artifact".into()],
    );
    let text = engine()
        .print_keyed(name, &edited, &["Artifact".into()], &[])
        .unwrap()
        .unwrap();
    assert!(text.contains("{5}"), "{text}");
    assert!(text.to_lowercase().contains("end step"), "{text}");
    assert!(text.to_lowercase().contains("artifact card"), "{text}");
}

#[test]
fn owned_harness_and_turn_compositions_work_without_mined_rules() {
    let off = engine().rule_set(|name| name.starts_with("m_") || name.starts_with("t_"));
    for case in family() {
        let name = case["name"].as_str().unwrap();
        let source = case["text"].as_str().unwrap();
        let expected = canonical_abilities(&case["expected"]).unwrap();
        for result in [
            engine().parse(name, source),
            engine().parse_without(name, source, &off),
        ] {
            assert_eq!(result.unwrap().readings, vec![expected.clone()], "{source}");
        }
        let types: Vec<String> = serde_json::from_value(case["types"].clone()).unwrap();
        checked_print(name, source, &expected, &types);
    }
}

#[test]
fn granted_gate_watches_holder_while_named_harness_object_watches_giver() {
    let cases = family();
    let name = cases[6]["name"].as_str().unwrap();
    let text = cases[6]["text"].as_str().unwrap();
    let value = canonical_abilities(&cases[6]["expected"]).unwrap();
    assert_eq!(
        engine().parse(name, text).unwrap().readings,
        vec![value.clone()]
    );
    let emission = &value[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"];
    assert_eq!(emission["source"]["kind"], "Source");
    let inner = &emission["changes"][0]["args"]["args"];
    assert_eq!(
        inner["event"]["Filtered"]["condition"]["args"]["object"]["kind"],
        "Source"
    );
    assert_eq!(
        inner["body"]["instructions"][0]["args"]["operation"]["args"]["action"]["Harness"]["object"]
            ["kind"],
        "GrantingObject"
    );
    let holder = canonical_abilities(&cases[5]["expected"]).unwrap();
    assert_ne!(holder, value);
}
