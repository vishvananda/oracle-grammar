//! Correct reported source/recipient/filter operands independently of gold scoring.
use super::*;

#[test]
fn reported_prevention_operands_have_only_the_correct_reading_and_print() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/prevention-reference-corrections.json"
    ))
    .unwrap();
    for case in fixtures["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        validate_abilities(&case["expected"]).unwrap();
        let expected = canonical_abilities(&case["expected"]).unwrap();
        assert_eq!(
            engine().parse(name, text).unwrap().readings,
            vec![expected.clone()],
            "{name} must have the corrected meaning alone"
        );
        let trail = engine().trail_of(name, &expected, text).unwrap().unwrap();
        assert_eq!(
            engine().print_trail(name, &expected, &trail).unwrap(),
            Some(text.to_string()),
            "{name} exact wording"
        );
        let types: Vec<_> = case["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_string())
            .collect();
        let default = engine()
            .print_keyed(name, &expected, &types, &[])
            .unwrap()
            .expect("corrected operands must print and reparse");
        assert!(
            engine()
                .parse(name, &default)
                .unwrap()
                .readings
                .contains(&expected)
        );
    }
}
