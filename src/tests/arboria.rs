//! Putting actor and entering controller remain distinct across named turns.
use super::*;

fn case() -> Value {
    serde_json::from_str::<Value>(include_str!("../../tests/fixtures/arboria-correction.json"))
        .unwrap()["cases"][0]
        .clone()
}
fn history(value: &mut Value, edit: &mut impl FnMut(&mut Value)) {
    if value["kind"] == "EventHistory" {
        edit(&mut value["args"]);
        return;
    }
    match value {
        Value::Object(fields) => fields.values_mut().for_each(|v| history(v, edit)),
        Value::Array(items) => items.iter_mut().for_each(|v| history(v, edit)),
        _ => {}
    }
}

#[test]
fn arboria_has_only_the_actor_history_meaning_and_exact_native_print() {
    let case = case();
    let text = case["text"].as_str().unwrap();
    let expected = canonical_abilities(&case["expected"]).unwrap();
    assert_eq!(
        engine().parse("Arboria", text).unwrap().readings,
        vec![expected.clone()]
    );
    let trail = engine()
        .trail_of("Arboria", &expected, text)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine().print_trail("Arboria", &expected, &trail).unwrap(),
        Some(text.to_string())
    );
    assert!(
        engine()
            .print_keyed("Arboria", &expected, &["Enchantment".to_string()], &[])
            .unwrap()
            .is_some()
    );
    let mut events = Vec::new();
    visit(&expected, "EventHistory", &mut events);
    assert_eq!(events.len(), 1);
    let query = &events[0];
    assert_eq!(query["event"]["Player"]["event"], "PutsOntoBattlefield");
    assert_eq!(query["event"]["Player"]["players"]["kind"], "One");
    assert_eq!(
        query["event"]["Player"]["players"]["args"],
        query["window"]["LastTurnOf"]
    );
    assert_eq!(query["measure"], "Occurrences");
    let mut cast = Vec::new();
    visit(&expected, "PlayerDid", &mut cast);
    assert_eq!(cast.len(), 1);
    assert!(cast[0]["event"].get("CastSpell").is_some());
}

#[test]
fn controller_and_other_turn_windows_cannot_normalize_to_the_putting_actor() {
    let expected = canonical_abilities(&case()["expected"]).unwrap();
    for edit in 0..4 {
        let mut different = expected.clone();
        history(&mut different, &mut |query| match edit {
            0 => query["event"]["Player"]["event"] = json!("EntersUnderControl"),
            1 => query["window"] = json!("LastTurn"),
            2 => query["window"] = json!({"SinceLastTurnOf":query["window"]["LastTurnOf"]}),
            _ => {
                query["event"]["Player"]["players"] =
                    json!({"kind":"One","args":{"kind":"AbilityController"}})
            }
        });
        validate_abilities(&different).unwrap();
        assert_ne!(canonical_abilities(&different).unwrap(), expected);
    }
}

#[test]
fn actor_population_prints_edited_current_filter_operands() {
    let mut expected = canonical_abilities(&case()["expected"]).unwrap();
    history(&mut expected, &mut |query| {
        query["event"]["Player"]["objects"] = json!({"kind":"AllOf","args":[{"kind":"Not","args":{"kind":"Token"}},{"kind":"CardType","args":"Artifact"}]});
    });
    assert!(printed_types(&expected, &["Enchantment"]).contains("artifact"));
}
