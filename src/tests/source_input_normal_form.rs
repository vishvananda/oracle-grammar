//! A source input proof keeps the current cost and every printed source role.
use super::*;
use crate::{engine::CardReading, model::ir::Characteristics};

const NAME: &str = "Context example";
const ORIGINAL: &str =
    "{2}, {T}, Sacrifice this artifact: This artifact deals 2 damage to any target.";
const OTHER_COST: &str =
    "{2}, {T}, Sacrifice another creature: This artifact deals 2 damage to any target.";

fn context() -> Characteristics {
    serde_json::from_value(
        json!({"supertypes":[],"card_types":["Artifact"],"subtypes":[],
        "mana_cost":[],"color_indicator":[],"power_toughness":null,"loyalty":null,"defense":null}),
    )
    .unwrap()
}
fn read(text: &str) -> CardReading {
    let mut parsed = engine().parse_card(NAME, text, &context()).unwrap();
    assert_eq!(parsed.readings.len(), 1, "{text}");
    parsed.readings.remove(0)
}
fn same(reading: &CardReading, text: &str) {
    let expected = canonical_abilities(&reading.abilities).unwrap();
    let parsed = engine().parse_card(NAME, text, &context()).unwrap();
    assert!(
        parsed.readings.iter().any(|r| r.abilities == expected),
        "{text}"
    );
}

#[test]
fn a_proved_source_cost_returns_one_meaning_and_keeps_exact_and_default_printing() {
    let reading = read(ORIGINAL);
    assert_eq!(
        reading.abilities[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["source"],
        json!({"kind":"Source"})
    );
    let trail = engine()
        .card_key_of(NAME, &reading, &context(), ORIGINAL, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine()
            .print_card_trail(NAME, &reading, &context(), &trail)
            .unwrap()
            .as_deref(),
        Some(ORIGINAL)
    );
    let printed = engine()
        .print_card_keyed(NAME, &reading, &context(), &[])
        .unwrap()
        .unwrap();
    same(&reading, &printed);
}

#[test]
fn editing_the_cost_does_not_retarget_the_source_to_the_sacrificed_creature() {
    let mut edited = read(ORIGINAL);
    let old_trail = engine()
        .card_key_of(NAME, &edited, &context(), ORIGINAL, true)
        .unwrap()
        .unwrap();
    let other = read(OTHER_COST);
    edited.abilities[0]["args"]["cost"] = other.abilities[0]["args"]["cost"].clone();
    assert_eq!(edited.abilities, other.abilities);
    let printed = engine()
        .print_card_keyed(NAME, &edited, &context(), &[])
        .unwrap()
        .unwrap();
    same(&edited, &printed);
    if let Some(printed) = engine()
        .print_card_trail(NAME, &edited, &context(), &old_trail)
        .unwrap()
    {
        same(&edited, &printed);
    }
    let parts = edited.abilities[0]["args"]["cost"]["parts"]
        .as_array()
        .unwrap();
    let component = parts
        .iter()
        .position(|part| part.get("Sacrifice").is_some())
        .unwrap();
    let mut chosen = edited.abilities.clone();
    chosen[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["source"] = json!({"kind":"Query","args":{
        "objects":{"kind":"CostObjects","args":{"payment":"Activation","component":[component],"view":"Inputs","phase":"Before"}},"index":0}});
    validate_abilities(&chosen).unwrap();
    assert_ne!(
        canonical_abilities(&chosen).unwrap(),
        canonical_abilities(&edited.abilities).unwrap()
    );
}
