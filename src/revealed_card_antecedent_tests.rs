//! Public whole-card regression; no fabricated source substitutions or aliases.
use crate::engine::{Engine, canonical_abilities};
use serde_json::Value;
fn engine() -> &'static Engine {
    crate::test_support::engine()
}
fn cases() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!(
        "../tests/fixtures/revealed-card-antecedents.json"
    ))
    .unwrap()["cases"]
        .as_array()
        .unwrap()
        .clone()
}
#[test]
fn eight_originals_preserve_only_the_complete_native_reveal_reading() {
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let expected = canonical_abilities(&case["expected"]).unwrap();
        let got = engine().parse(name, text).unwrap().readings;
        assert_eq!(got, vec![expected.clone()], "{name}");
        if let Some(old) = case.get("wrong_old") {
            assert!(!got.contains(&canonical_abilities(old).unwrap()), "{name}");
        }
    }
}
#[test]
fn yuriko_exact_and_edited_header_print_with_the_observed_card_value() {
    let case = cases()
        .into_iter()
        .find(|c| c["name"] == "Yuriko, the Tiger's Shadow")
        .unwrap();
    let name = case["name"].as_str().unwrap();
    let text = case["text"].as_str().unwrap();
    let expected = canonical_abilities(&case["expected"]).unwrap();
    let trail = engine().trail_of(name, &expected, text).unwrap().unwrap();
    assert_eq!(
        engine()
            .print_trail(name, &expected, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
    let mut edited = case["expected"].clone();
    for filter in edited[1]["args"]["event"]["Damage"]["sources"]["args"]
        .as_array_mut()
        .unwrap()
    {
        if filter["kind"] == "Subtype" {
            filter["args"] = serde_json::json!("Rogue");
        }
    }
    let edited = canonical_abilities(&edited).unwrap();
    assert_ne!(edited, expected);
    let changed = text.replace("a Ninja", "a Rogue");
    assert_eq!(
        engine().parse(name, &changed).unwrap().readings,
        vec![edited.clone()]
    );
    let printed = engine()
        .print_keyed(name, &edited, &[], &[])
        .unwrap()
        .unwrap();
    assert_eq!(
        engine().parse(name, &printed).unwrap().readings,
        vec![edited.clone()]
    );
    if let Some(replayed) = engine().print_trail(name, &edited, &trail).unwrap() {
        assert_ne!(replayed, text);
        assert_eq!(
            engine().parse(name, &replayed).unwrap().readings,
            vec![edited]
        );
    }
}

#[test]
fn a_typed_older_graveyard_card_is_not_the_newly_revealed_library_card() {
    let mut expected = cases()
        .into_iter()
        .find(|c| c["name"] == "Dark Confidant")
        .unwrap()["expected"]
        .clone();
    expected[0]["args"]["event"] = serde_json::json!({"ZoneChange":{"objects":{"kind":"AllOf","args":[{"kind":"Not","args":{"kind":"Token"}},
            {"kind":"OwnedBy","args":{"kind":"One","args":{"kind":"AbilityController"}}}]},"from":null,"to":"Graveyard"}});
    let text = "Whenever a card is put into your graveyard from anywhere, reveal the top card of your library and put that card into your hand. You lose life equal to its mana value.";
    let expected = canonical_abilities(&expected).unwrap();
    assert_eq!(
        engine()
            .parse("Different card epochs", text)
            .unwrap()
            .readings,
        vec![expected]
    );
}
