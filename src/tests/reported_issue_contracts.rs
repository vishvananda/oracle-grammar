//! The seven open reports retain their corrected meanings and print operands.
use super::*;
use crate::engine::semantic_abilities;

fn cases() -> Vec<Value> {
    let data: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/reported-corrections-20261116.json"
    ))
    .unwrap();
    data["cases"].as_array().unwrap().clone()
}

fn case(name: &str) -> Value {
    cases()
        .into_iter()
        .find(|case| case["name"] == name)
        .unwrap()
}

fn reading(name: &str) -> Value {
    let case = case(name);
    reading_case(&case)
}

fn reading_case(case: &Value) -> Value {
    let name = case["name"].as_str().unwrap();
    let context = serde_json::from_value(
        case.get("context")
            .unwrap_or(&case["characteristics"])
            .clone(),
    )
    .unwrap();
    let parsed = engine()
        .parse_card(name, case["text"].as_str().unwrap(), &context)
        .unwrap();
    let expected = semantic_abilities(&case["expected"]).unwrap();
    assert_eq!(
        parsed.readings.len(),
        case["readings"].as_u64().unwrap_or(1) as usize,
        "{name}"
    );
    parsed
        .readings
        .into_iter()
        .find_map(|reading| {
            (semantic_abilities(&reading.abilities).unwrap() == expected)
                .then_some(reading.abilities)
        })
        .unwrap_or_else(|| panic!("{name}: no expected reading"))
}

fn check_wording(cases: Vec<Value>) {
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let abilities = reading_case(&case);
        let types = crate::corpus::object_print_context(
            case.get("context").unwrap_or(&case["characteristics"]),
        );
        let default = engine().print_keyed(name, &abilities, &types, &[]).unwrap();
        if case["default"].as_bool().unwrap_or(true) {
            assert!(default.is_some(), "{name}: no verified default print");
        }
        if case["exact"].as_bool() == Some(false) {
            continue;
        }
        if default.as_deref() == Some(text) {
            continue;
        }
        let key = engine()
            .derivation_of(name, &abilities, text)
            .unwrap()
            .unwrap();
        let ordinary = engine().productions_of(name, &abilities, &types).unwrap();
        let short = Engine::trim_key(&key, &ordinary);
        let exact = [short, key].iter().any(|key| {
            engine()
                .print_keyed(name, &abilities, &types, key)
                .unwrap()
                .as_deref()
                == Some(text)
        });
        if !exact {
            let trail = engine().trail_of(name, &abilities, text).unwrap().unwrap();
            assert_eq!(
                engine()
                    .print_trail(name, &abilities, &trail)
                    .unwrap()
                    .as_deref(),
                Some(text),
                "{name}"
            );
        }
    }
}

#[test]
fn every_reported_card_has_only_its_supported_corrected_reading() {
    check_wording(cases());
}

#[test]
fn immediate_reveal_normalization_keeps_every_reviewed_reference_and_its_wording() {
    let data: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/reported-observation-proofs.json"
    ))
    .unwrap();
    check_wording(data["cases"].as_array().unwrap().clone());
}

#[test]
fn grouped_mana_keeps_one_effect_and_prints_changed_outputs() {
    for (name, count) in [("Naked Singularity", 5), ("Reality Twist", 4)] {
        let mut abilities = reading(name);
        let mut replacements = Vec::new();
        visit(&abilities, "Replacement", &mut replacements);
        assert_eq!(replacements.len(), 1, "{name}");
        let options = &mut abilities[1]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]
            ["change"]["ReplaceManaChoices"];
        assert_eq!(options.as_array().unwrap().len(), count);
        options[0]["color"] = json!("Blue");
        let text = printed_types(&abilities, &["Enchantment"]);
        assert!(text.contains("{U}"), "{text}");
        assert!(!text.contains("Plains produce {R}"), "{text}");
    }
}

#[test]
fn linked_tokens_remain_distinct_from_broad_source_creations() {
    let abilities = reading("Tetravus");
    let mut linked = Vec::new();
    let mut broad = Vec::new();
    visit(&abilities, "CreatedWith", &mut linked);
    visit(&abilities, "MadeBy", &mut broad);
    assert_eq!(linked, [json!({"kind":"Source"})]);
    assert!(broad.is_empty());
    assert!(abilities.to_string().contains("Token"));
}

#[test]
fn historical_death_return_never_becomes_a_source_move() {
    let abilities = reading("Krovikan Vampire");
    let mut moves = Vec::new();
    visit(&abilities, "Move", &mut moves);
    assert_eq!(moves.len(), 1);
    let query = &moves[0]["objects"];
    assert_eq!(query["kind"], "FromHistoryDestinations");
    assert_eq!(query["args"]["window"], "ThisTurn");
    assert_eq!(query["args"]["event"]["ZoneChange"]["from"], "Battlefield");
    assert_eq!(query["args"]["event"]["ZoneChange"]["to"], "Graveyard");
    assert!(query.to_string().contains("Creature"));
    assert!(query.to_string().contains("Damage"));
}

#[test]
fn entry_history_preserves_current_word_filters_and_local_receipts() {
    let abilities = reading("Wood Elemental");
    let mut historical = Vec::new();
    visit(&abilities, "SacrificedAsEntered", &mut historical);
    assert_eq!(
        historical,
        [json!({"kind":"Source"}), json!({"kind":"Source"})]
    );
    let mut earlier = Vec::new();
    visit(&abilities, "Earlier", &mut earlier);
    assert!(earlier.is_empty());
    let dracoplasm = reading("Dracoplasm");
    let mut transitions = Vec::new();
    visit(&dracoplasm, "Transition", &mut transitions);
    assert!(
        transitions
            .iter()
            .all(|receipt| receipt["phase"] == "Before")
    );
    assert!(!transitions.is_empty());
}

#[test]
fn mimeoplasm_counts_recorded_exile_arrivals_and_esper_moves_the_observation() {
    let abilities = reading("Mimeoplasm, Revered One");
    let counter = &abilities[0]["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["change"]
        ["AddCounters"]["amount"];
    assert!(counter.to_string().contains("Transition"));
    assert!(counter.to_string().contains("After"));
    assert!(counter.to_string().contains("InZone"));
    assert!(counter.to_string().contains("Exile"));
    let abilities = reading("Summon: Esper Maduin");
    let body = &abilities[0]["args"]["chapters"][0]["args"]["body"]["instructions"];
    assert_eq!(
        body[1]["args"]["then"]["instructions"][0]["args"]["operation"]["args"]["objects"],
        json!({"kind":"Observed","args":body[0]["args"]["result"]})
    );
}

#[test]
fn the_five_gates_keep_wording_without_a_second_intrinsic_activation() {
    for name in [
        "Gate of the Black Dragon",
        "Gate to Manorborn",
        "Gate to Seatower",
        "Gate to Tumbledown",
        "Gate to the Citadel",
    ] {
        let case = case(name);
        let face: crate::model::ir::Card = serde_json::from_value(json!({
            "schema_version":crate::model::ir::SCHEMA_VERSION,
            "name":name,"characteristics":case["characteristics"],
            "abilities":reading(name)
        }))
        .unwrap();
        let written = crate::model::canonical::card(&face).unwrap();
        let meaning = crate::model::canonical::semantic_card(&face).unwrap();
        assert_eq!(
            written.abilities.len(),
            meaning.abilities.len() + 1,
            "{name}"
        );
    }
}
