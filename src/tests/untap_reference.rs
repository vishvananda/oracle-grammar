//! A named player's step differs from an affected permanent's later controller.
use super::*;

fn effect(value: &mut Value) -> &mut Value {
    &mut value[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]
}
fn controller() -> Value {
    json!({"kind":"One","args":{"kind":"ControllerOf","args":{"kind":"Affected"}}})
}

fn untap_effects(value: &Value, out: &mut Vec<Value>) {
    if value["kind"] == "CreateContinuousEffect"
        && value["args"]["changes"]
            .as_array()
            .is_some_and(|changes| changes.iter().any(|c| c["args"].get("UntapStep").is_some()))
    {
        out.push(value["args"].clone());
    }
    match value {
        Value::Object(fields) => fields.values().for_each(|v| untap_effects(v, out)),
        Value::Array(items) => items.iter().for_each(|v| untap_effects(v, out)),
        _ => {}
    }
}

#[test]
fn reported_next_untap_cases_and_controller_controls_keep_their_exact_meanings() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/untap-reference-corrections.json"
    ))
    .unwrap();
    for case in fixtures["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        validate_abilities(&case["expected"]).unwrap();
        let expected = canonical_abilities(&case["expected"]).unwrap();
        let readings = engine().parse(name, text).unwrap().readings;
        assert_eq!(
            readings.len() as u64,
            case["readings"].as_u64().unwrap(),
            "{name}"
        );
        assert!(readings.contains(&expected), "{name}");
        let mut expected_rules = Vec::new();
        untap_effects(&expected, &mut expected_rules);
        for reading in readings {
            let mut rules = Vec::new();
            untap_effects(&reading, &mut rules);
            assert_eq!(
                rules, expected_rules,
                "{name}: every supported reading retains the named untap player"
            );
        }
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
            .map(|t| t.as_str().unwrap().to_string())
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
fn live_controller_population_implies_only_its_literal_stable_untap_player() {
    let current = compositional("Lands you control don't untap during your next untap step.");
    let mut legacy = current.clone();
    effect(&mut legacy)["changes"][0]["args"]["UntapStep"]["players"] = controller();
    validate_abilities(&legacy).unwrap();
    assert_eq!(canonical_abilities(&legacy).unwrap(), current);
    assert_eq!(canonical_abilities(&current).unwrap(), current);
    printed(&current);
    for change in [
        "snapshot",
        "unfiltered",
        "fixed",
        "controller_window",
        "extra",
        "conjunction_bound",
    ] {
        let mut unproved = legacy.clone();
        let op = effect(&mut unproved);
        match change {
            "snapshot" => op["evaluation"] = json!("Snapshot"),
            "unfiltered" => {
                op["affected"]["Objects"]["args"]["filter"] =
                    json!({"kind":"CardType","args":"Land"})
            }
            "fixed" => op["affected"] = json!({"Objects":{"kind":"One","args":{"kind":"Source"}}}),
            "controller_window" => op["duration"] = json!("DuringControllersNextUntapStep"),
            "conjunction_bound" => {
                let filter = &mut op["affected"]["Objects"]["args"]["filter"];
                let parts = filter["args"].as_array_mut().unwrap();
                parts.extend((0..65).map(|_| json!({"kind":"CardType","args":"Land"})));
            }
            "extra" => {
                op["extra"] = json!(true);
            }
            _ => unreachable!(),
        }
        let mut result = canonical_abilities(&unproved).unwrap();
        assert_eq!(
            effect(&mut result)["changes"][0]["args"]["UntapStep"]["players"],
            controller(),
            "{change}"
        );
        if change == "extra" {
            assert!(validate_abilities(&result).is_err());
        }
    }
}

#[test]
fn named_untap_player_and_later_controller_are_distinct_printable_operands() {
    let mut named = compositional("Target creature doesn't untap during your next untap step.");
    let mut following =
        compositional("Target creature doesn't untap during its controller's next untap step.");
    assert_eq!(
        effect(&mut named)["duration"],
        json!({"DuringNextUntapStep":{"kind":"AbilityController"}})
    );
    assert_eq!(
        effect(&mut named)["changes"][0]["args"]["UntapStep"]["players"],
        json!({"kind":"One","args":{"kind":"AbilityController"}})
    );
    assert_eq!(
        effect(&mut following)["duration"],
        json!("DuringControllersNextUntapStep")
    );
    assert_eq!(
        effect(&mut following)["changes"][0]["args"]["UntapStep"]["players"],
        controller()
    );
    assert_ne!(named, following);
    printed(&named);
    printed(&following);
}
