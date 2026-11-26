//! Combat history and intrinsic definitions preserve their actual scope.
use super::*;

fn cases() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!(
        "../../tests/fixtures/history-cda-corrections.json"
    ))
    .unwrap()["cases"]
        .as_array()
        .unwrap()
        .clone()
}
fn frame(name: &str) -> Value {
    canonical_abilities(&cases().into_iter().find(|c| c["name"] == name).unwrap()["expected"])
        .unwrap()
}

#[test]
fn combat_and_cda_reports_and_controls_have_only_the_correct_reading_and_print() {
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        validate_abilities(&case["expected"]).unwrap();
        let expected = canonical_abilities(&case["expected"]).unwrap();
        assert_eq!(
            engine().parse(name, text).unwrap().readings,
            vec![expected.clone()],
            "{name}"
        );
        let trail = engine().trail_of(name, &expected, text).unwrap().unwrap();
        assert_eq!(
            engine().print_trail(name, &expected, &trail).unwrap(),
            Some(text.to_string()),
            "{name}"
        );
        let types: Vec<_> = case["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(
            engine()
                .print_keyed(name, &expected, &types, &[])
                .unwrap()
                .is_some(),
            "{name}"
        );
    }
}

#[test]
fn total_war_attack_history_is_current_combat_while_control_exemption_is_turn_long() {
    let reading = frame("Total War");
    let mut histories = Vec::new();
    visit(&reading, "EventHistory", &mut histories);
    assert_eq!(histories.len(), 1);
    assert_eq!(histories[0]["window"], "ThisCombat");
    assert_eq!(histories[0]["event"]["Object"]["event"], "Attacks");
    assert_eq!(
        histories[0]["event"]["Object"]["objects"],
        json!({"kind":"SameObject","args":{"kind":"Candidate"}})
    );
    let mut controlled = Vec::new();
    visit(&reading, "ControlledBy", &mut controlled);
    assert_eq!(
        controlled,
        vec![json!({"kind":"One","args":{"kind":"EventSubject"}})]
    );
    assert!(reading.to_string().contains("ControlledSinceTurnBegan"));
    let mut histories = Vec::new();
    visit(
        &frame("Explicit turn history control"),
        "EventHistory",
        &mut histories,
    );
    assert_eq!(histories.len(), 1);
    assert_eq!(histories[0]["window"], "ThisTurn");
    let text = cases()
        .into_iter()
        .find(|c| c["name"] == "Total War")
        .unwrap()["text"]
        .as_str()
        .unwrap()
        .replace("one or more", "two or more");
    assert!(engine().parse("Total War", &text).is_err());
}

#[test]
fn chosen_number_definition_retains_its_symbolic_formula_and_prints_edited_constant() {
    let mut reading = frame("Shapeshifter");
    let definition = &mut reading[2]["args"];
    assert_eq!(
        definition["kind"],
        json!({"CharacteristicDefining":{"keyword":null,"contributes_to_color_identity":true}})
    );
    assert_eq!(
        definition["affected"],
        json!({"Objects":{"kind":"One","args":{"kind":"Source"}}})
    );
    assert_eq!(
        definition["changes"],
        json!([{"kind":"SetBasePowerToughness","args":{"power":{"kind":"ChosenNumber"},"toughness":{"kind":"Subtract","args":[{"kind":"Constant","args":7},{"kind":"ChosenNumber"}]}}}])
    );
    definition["changes"][0]["args"]["toughness"]["args"][0]["args"] = json!(9);
    let text = printed(&reading);
    assert!(
        text.contains("9 minus") || text.contains("nine minus"),
        "{text}"
    );
    assert_eq!(
        frame("Ordinary self-setting control")[0]["args"]["kind"],
        "Ordinary"
    );
    let text = "This creature's power is equal to the last chosen number and its toughness is equal to 7 minus that number.";
    assert_eq!(compositional(text), json!([frame("Shapeshifter")[2]]));
}

#[test]
fn chosen_number_intervals_print_their_current_domain() {
    let name = "Synthetic interval";
    for (lower, upper) in [(0, 7), (10, 100)] {
        let text = format!("Choose a number between {lower} and {upper}.");
        let result = engine().parse(name, &text).unwrap();
        assert_eq!(result.readings.len(), 1);
        let reading = &result.readings[0];
        let key = engine().trail_of(name, reading, &text).unwrap().unwrap();
        assert_eq!(
            engine().print_trail(name, reading, &key).unwrap(),
            Some(text)
        );
    }
    let mut reading = frame("Shapeshifter");
    let entry_choice = &mut reading[0]["args"]["body"]["instructions"][0]["args"]["operation"];
    let options = entry_choice["args"]["terms"]["options"]
        .as_array_mut()
        .unwrap();
    assert_eq!(options.len(), 8);
    options.pop();
    let text = printed(&reading);
    assert!(text.contains("choose a number between 0 and 6"), "{text}");
    assert!(
        text.contains("you may choose a number between 0 and 7"),
        "{text}"
    );
}
