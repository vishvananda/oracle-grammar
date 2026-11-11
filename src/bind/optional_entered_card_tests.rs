use crate::engine::{Engine, canonical_abilities, validate_abilities};
use serde_json::{Value, json};

fn fixtures() -> Vec<Value> {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/optional-entered-card-receipts.json"
    ))
    .unwrap()
}
fn types(case: &Value) -> Vec<String> {
    serde_json::from_value(case["types"].clone()).unwrap()
}
fn exact(engine: &Engine, case: &Value, reading: &Value) -> bool {
    let name = case["name"].as_str().unwrap();
    let text = case["text"].as_str().unwrap();
    if let Some(key) = engine.derivation_of(name, reading, text).unwrap()
        && engine
            .print_keyed(name, reading, &types(case), &key)
            .unwrap()
            .as_deref()
            == Some(text)
    {
        return true;
    }
    engine
        .trail_of(name, reading, text)
        .unwrap()
        .is_some_and(|trail| {
            engine
                .print_trail(name, reading, &trail)
                .unwrap()
                .as_deref()
                == Some(text)
        })
}

#[test]
fn whole_original_cards_keep_scopes_and_retire_only_wrong_implicit_objects() {
    let engine = crate::test_support::engine();
    for case in fixtures() {
        let name = case["name"].as_str().unwrap();
        let parsed = engine.parse(name, case["text"].as_str().unwrap()).unwrap();
        let mut expected = case["expected_readings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| canonical_abilities(v).unwrap())
            .collect::<Vec<_>>();
        expected.sort_by_key(Value::to_string);
        expected.dedup();
        let mut actual = parsed.readings.clone();
        actual.sort_by_key(Value::to_string);
        assert_eq!(
            actual, expected,
            "{name}: reference-only correction must preserve every scope"
        );
        for old in case["proper_previous_readings"].as_array().unwrap() {
            let old = canonical_abilities(old).unwrap();
            assert!(
                parsed.readings.contains(&old),
                "{name}: proper old Dawn Optional reading"
            );
        }
        for reading in &parsed.readings {
            validate_abilities(reading).unwrap();
            assert!(exact(engine, &case, reading), "{name}: exact print");
            assert!(
                engine
                    .print_keyed(name, reading, &types(&case), &[])
                    .unwrap()
                    .is_some(),
                "{name}: verified default print"
            );
        }
    }
}

#[test]
fn surprise_native_default_and_edited_color_use_the_current_entry_receipt() {
    let engine = crate::test_support::engine();
    let case = fixtures()
        .into_iter()
        .find(|c| c["name"] == "Surprise Deployment")
        .unwrap();
    let name = case["name"].as_str().unwrap();
    let native = canonical_abilities(&case["native"]).unwrap();
    let parsed = engine.parse(name, case["text"].as_str().unwrap()).unwrap();
    assert_eq!(parsed.readings, vec![native.clone()]);
    let printed = engine
        .print_keyed(name, &native, &types(&case), &[])
        .unwrap()
        .unwrap();
    assert!(
        engine
            .parse(name, &printed)
            .unwrap()
            .readings
            .contains(&native)
    );
    let mut edited = native;
    *edited.pointer_mut("/1/args/body/instructions/0/args/body/instructions/0/args/operation/args/objects/args/filter/args/1/args/args").unwrap() = json!("Blue");
    let edited = canonical_abilities(&edited).unwrap();
    let text = engine
        .print_keyed(name, &edited, &types(&case), &[])
        .unwrap()
        .unwrap();
    assert!(
        text.contains("blue") && !text.contains("nonwhite"),
        "{text}"
    );
    assert!(
        engine
            .parse(name, &text)
            .unwrap()
            .readings
            .contains(&edited)
    );
    assert!(
        engine
            .parse(
                name,
                &case["text"]
                    .as_str()
                    .unwrap()
                    .replace("nonwhite", "nonblue")
            )
            .unwrap()
            .readings
            .contains(&edited)
    );
}

fn operation(instruction: &Value) -> Option<&Value> {
    (instruction["kind"] == "Effect").then_some(&instruction["args"]["operation"])
}
fn one_result(objects: &Value, result: &Value) -> bool {
    objects
        == &json!({"kind":"One","args":{"kind":"Result","args":{
            "result":result,"phase":"After","index":0
        }}})
}
fn entered_followups(ability: &Value) {
    let all = ability["args"]["body"]["instructions"].as_array().unwrap();
    let optional = all.iter().find(|i| i["kind"] == "Optional").unwrap();
    let body = optional["args"]["body"]["instructions"].as_array().unwrap();
    let moved = body
        .iter()
        .find(|i| operation(i).is_some_and(|o| o["kind"] == "Move"))
        .unwrap();
    let result = &moved["args"]["result"];
    let mut tails: Vec<&Value> = body.iter().skip_while(|i| *i != moved).skip(1).collect();
    tails.extend(all.iter().skip_while(|i| *i != optional).skip(1));
    assert!(!tails.is_empty());
    for tail in tails {
        let op = operation(tail).unwrap();
        if op["kind"] == "CreateContinuousEffect" {
            assert_eq!(op["args"]["source"], json!({"kind":"Source"}));
            assert!(one_result(&op["args"]["affected"]["Objects"], result));
        } else {
            assert_eq!(op["kind"], "CreateDelayedTrigger");
            let action = &op["args"]["body"]["instructions"][0]["args"]["operation"];
            assert!(one_result(&action["args"]["objects"], result));
        }
    }
}

#[test]
fn prior_results_and_targets_cannot_steal_the_actual_hand_entry() {
    let engine = crate::test_support::engine();
    for prefix in [
        "Draw a card. ",
        "Tap target creature. ",
        "Tap target creature. Tap target creature. ",
        "Create two 1/1 green Saproling creature tokens. ",
    ] {
        let text = format!(
            "{{2}}{{G}}: {prefix}You may put an Elemental creature card from your hand onto the battlefield. That creature gains flying until end of combat. Sacrifice it at the beginning of the next end step."
        );
        let parsed = engine.parse("Entry Controls", &text).unwrap();
        for reading in &parsed.readings {
            entered_followups(&reading[0]);
            let printed = engine
                .print_keyed("Entry Controls", reading, &["Creature".into()], &[])
                .unwrap()
                .unwrap();
            assert!(
                printed.contains("{2}{G}") && printed.contains("flying"),
                "{printed}"
            );
            assert!(
                engine
                    .parse("Entry Controls", &printed)
                    .unwrap()
                    .readings
                    .contains(reading)
            );
        }
    }
}

#[test]
fn prior_target_slot_and_result_do_not_change_the_graveyard_receipt() {
    let engine = crate::test_support::engine();
    let text = "At the beginning of your upkeep, tap target creature. Draw a card. You may return target creature card from your graveyard to the battlefield. That creature gains haste until end of turn. Exile it at the beginning of the next end step.";
    let parsed = engine.parse("Entry Controls", text).unwrap();
    for reading in &parsed.readings {
        entered_followups(&reading[0]);
        let targets = reading[0]["args"]["announcement"]["targets"]
            .as_array()
            .unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[1]["slot"], 1);
        assert_eq!(targets[1]["kind"]["Object"]["kind"], "Card");
    }
}

#[test]
fn explicit_ability_holder_is_not_an_implicit_entered_card() {
    let engine = crate::test_support::engine();
    for noun in ["this creature", "Entry Controls"] {
        let text = format!(
            "{{R}}: You may put a planeswalker card from your hand onto the battlefield. Sacrifice {noun} at the beginning of the next end step."
        );
        let parsed = engine.parse("Entry Controls", &text).unwrap();
        for reading in &parsed.readings {
            let text = reading.to_string();
            assert!(!text.contains("\"phase\":\"After\""));
        }
    }
}

#[test]
fn source_aware_station_control_preserves_the_112th_complete_meaning() {
    use crate::engine::{CardNote, CardReading, PrintedPowerToughness};
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../tests/fixtures/station-context-native.json"
    ))
    .unwrap();
    let case = cases
        .iter()
        .find(|c| c["name"] == "Sledge-Class Seedship")
        .unwrap();
    let engine = crate::test_support::engine();
    let name = case["name"].as_str().unwrap();
    let characteristics = serde_json::from_value(case["context"].clone()).unwrap();
    let pt = case["printed_power_toughness"].as_array().unwrap();
    let source = engine
        .card_input_context(
            name,
            case["raw"].as_str().unwrap(),
            &characteristics,
            PrintedPowerToughness::Known {
                power: i32::try_from(pt[0].as_i64().unwrap()).unwrap(),
                toughness: i32::try_from(pt[1].as_i64().unwrap()).unwrap(),
            },
        )
        .unwrap();
    assert!(source.station_present && source.characteristics.power_toughness.is_none());
    assert_eq!(
        serde_json::to_value(&source.station_animation).unwrap(),
        case["station_animation"]
    );
    let expected = CardReading {
        abilities: canonical_abilities(&case["expected"]).unwrap(),
        notes: vec![CardNote::StationPowerToughness {
            ability: 1,
            stage: 0,
        }],
    };
    let parsed = engine
        .parse_card_with_source(name, case["text"].as_str().unwrap(), &source)
        .unwrap();
    assert_eq!(parsed.readings, vec![expected]);
}
