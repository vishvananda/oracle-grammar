//! Public whole-card checks keep linked history and one activation's receipt distinct.
use crate::engine::{Engine, canonical_abilities};
use serde_json::{Value, json};
use std::sync::OnceLock;
fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| Engine::load().unwrap())
}
fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../tests/fixtures/linked-card-action-native.json"
    ))
    .unwrap()
}
fn complete(name: &str, text: &str, expected: &Value) {
    let expected = canonical_abilities(expected).unwrap();
    let off = engine().rule_set(|r| r.starts_with("m_") || r.starts_with("t_"));
    for parsed in [
        engine().parse(name, text),
        engine().parse_without(name, text, &off),
    ] {
        assert_eq!(
            parsed.unwrap().readings,
            vec![expected.clone()],
            "{name}: {text}"
        );
    }
}
fn exact_and_default(name: &str, text: &str, expected: &Value, types: &[String]) {
    let expected = canonical_abilities(expected).unwrap();
    let keyed = engine()
        .derivation_of(name, &expected, text)
        .unwrap()
        .and_then(|key| engine().print_keyed(name, &expected, types, &key).unwrap());
    let exact = if keyed.as_deref() == Some(text) {
        keyed
    } else {
        engine()
            .trail_of(name, &expected, text)
            .unwrap()
            .and_then(|trail| engine().print_trail(name, &expected, &trail).unwrap())
    };
    assert_eq!(exact.as_deref(), Some(text));
    let printed = engine()
        .print_keyed(name, &expected, types, &[])
        .unwrap()
        .unwrap();
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
fn originals_have_only_complete_native_readings_with_and_without_mined_rules() {
    for case in cases()["cases"].as_array().unwrap() {
        complete(
            case["name"].as_str().unwrap(),
            case["text"].as_str().unwrap(),
            &case["expected"],
        );
    }
}
#[test]
fn native_original_exact_and_default_use_the_same_source_proofs() {
    for (i, case) in cases()["cases"].as_array().unwrap().iter().enumerate() {
        exact_and_default(
            case["name"].as_str().unwrap(),
            case["text"].as_str().unwrap(),
            &case["expected"],
            &[if i == 0 { "Land" } else { "Artifact" }.into()],
        );
    }
}
#[test]
fn edited_linked_hand_selection_filter_and_return_cost_print_current_operands() {
    let data = cases();
    let case = &data["cases"][0];
    let mut expected = case["expected"].clone();
    let filter = &mut expected[0]["args"]["body"]["instructions"][0]["args"]["body"]["instructions"]
        [0]["args"]["operation"]["args"]["objects"]["args"]["filter"]["args"];
    let property = filter
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["kind"] == "Property")
        .unwrap();
    property["args"]["test"]["args"]["value"]["args"] = json!(9);
    expected[2]["args"]["cost"]["parts"] = json!([{"Mana":[{"Generic":2}]},"TapSymbol"]);
    let text = case["text"]
        .as_str()
        .unwrap()
        .replace("value 7", "value 9")
        .replace("{T}: Return", "{2}, {T}: Return");
    complete("Edited land", &text, &expected);
    let canonical = canonical_abilities(&expected).unwrap();
    let printed = engine()
        .print_keyed("Edited land", &canonical, &["Land".into()], &[])
        .unwrap()
        .unwrap();
    assert!(
        printed.contains('9') && printed.contains("{2}"),
        "{printed}"
    );
    complete("Edited land", &printed, &expected);
}
#[test]
fn prefix_target_and_result_namespaces_keep_the_full_actual_after_group_and_tapped_entry() {
    let text = "{3}, Sacrifice this artifact: Tap target artifact. Exile target creature. Return the exiled card to the battlefield tapped under its owner's control at the beginning of the next end step.";
    let off = engine().rule_set(|r| r.starts_with("m_") || r.starts_with("t_"));
    for parsed in [
        engine().parse("Receipt prefix", text),
        engine().parse_without("Receipt prefix", text, &off),
    ] {
        let readings = parsed.unwrap().readings;
        assert_eq!(readings.len(), 1);
        let ability = &readings[0][0];
        let items = &ability["args"]["body"]["instructions"];
        let producer = &items[1];
        assert_eq!(
            producer["args"]["operation"]["args"]["objects"]["args"]["args"]["slot"],
            1
        );
        let moved = &items[2]["args"]["operation"]["args"]["body"]["instructions"][0]["args"]["operation"]
            ["args"];
        assert_eq!(
            moved["objects"],
            json!({"kind":"InZone","args":{"objects":{"kind":"Transition","args":{"result":producer["args"]["result"],"phase":"After"}},"zone":"Exile"}})
        );
        assert_eq!(moved["destination"]["Battlefield"]["controller"], "Owner");
        assert_eq!(
            moved["destination"]["Battlefield"]["arrival"]["tapped"],
            true
        );
        assert_eq!(
            ability["args"]["cost"]["parts"][0],
            json!({"Mana":[{"Generic":3}]})
        );
        let printed = engine()
            .print_keyed("Receipt prefix", &readings[0], &["Artifact".into()], &[])
            .unwrap()
            .unwrap();
        assert!(
            printed.contains("tapped") && printed.contains("{3}"),
            "{printed}"
        );
        assert_eq!(
            engine().parse("Receipt prefix", &printed).unwrap().readings,
            readings
        );
    }
}
#[test]
fn a_return_without_the_source_entry_contract_stays_scalar() {
    let parsed = engine()
        .parse(
            "Unpaired land",
            "{T}: Return the exiled card to its owner's hand.",
        )
        .unwrap();
    for frame in parsed.readings {
        let mut objects =
            &frame[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"];
        if objects["kind"] == "InZone" {
            objects = &objects["args"]["objects"];
        }
        assert_eq!(objects["kind"], "One");
        assert_eq!(objects["args"]["kind"], "Query");
    }
}
#[test]
fn an_explicit_foreign_linked_population_keeps_its_announced_target() {
    let text = "{T}: Return all cards exiled with target artifact to their owners' hands.";
    let off = engine().rule_set(|r| r.starts_with("m_") || r.starts_with("t_"));
    for parsed in [
        engine().parse("Foreign link", text),
        engine().parse_without("Foreign link", text, &off),
    ] {
        for frame in parsed.unwrap().readings {
            let objects = &frame[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]
                ["objects"];
            assert_eq!(objects["kind"], "InZone");
            assert_eq!(
                objects["args"]["objects"]["args"]["filter"],
                json!({"kind":"ExiledWith","args":{"kind":"Target","args":{"slot":0,"index":0}}})
            );
        }
    }
}
