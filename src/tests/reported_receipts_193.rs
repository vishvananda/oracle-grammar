//! The follow-up issue cohort keeps identities and prints edited operands.
use super::*;
use crate::engine::semantic_abilities;

fn cases() -> Vec<Value> {
    let data: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/reported-receipts-193.json"
    ))
    .unwrap();
    data["cases"].as_array().unwrap().clone()
}

#[test]
fn receipt_followups_parse_only_the_reviewed_meaning_and_reprint_edits() {
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let context = serde_json::from_value(case["characteristics"].clone()).unwrap();
        let parsed = engine().parse_card(name, text, &context).unwrap();
        assert_eq!(parsed.readings.len(), 1, "{name}");
        let expected = semantic_abilities(&case["expected"]).unwrap();
        assert_eq!(
            semantic_abilities(&parsed.readings[0].abilities).unwrap(),
            expected,
            "{name}"
        );
        let reading = &parsed.readings[0];
        let trail = engine()
            .card_key_of(name, reading, &context, text, true)
            .unwrap()
            .unwrap();
        assert_eq!(
            engine()
                .print_card_trail(name, reading, &context, &trail)
                .unwrap()
                .as_deref(),
            Some(text),
            "{name}"
        );
        let types = crate::corpus::object_print_context(&case["characteristics"]);
        assert!(
            engine()
                .print_keyed(name, &case["expected"], &types, &[])
                .unwrap()
                .is_some(),
            "{name}"
        );
    }
}

#[test]
fn north_star_cannot_print_one_spell_for_an_edited_two_use_permission() {
    let case = cases()
        .into_iter()
        .find(|c| c["name"] == "North Star")
        .unwrap();
    let mut expected = case["expected"].clone();
    expected[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"][0]["args"]
        ["SpendManaAsLimited"]["uses"] = json!(2);
    let types = crate::corpus::object_print_context(&case["characteristics"]);
    if let Some(text) = engine()
        .print_keyed("North Star", &expected, &types, &[])
        .unwrap()
    {
        assert!(!text.contains("For one spell"), "{text}");
        let context = serde_json::from_value(case["characteristics"].clone()).unwrap();
        let parsed = engine().parse_card("North Star", &text, &context).unwrap();
        let expected = semantic_abilities(&expected).unwrap();
        assert!(
            parsed
                .readings
                .iter()
                .any(|r| semantic_abilities(&r.abilities).unwrap() == expected)
        );
    }
}
