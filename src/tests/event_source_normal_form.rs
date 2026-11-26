//! Editing an event must preserve a body reference to its ability holder.
use super::*;
use crate::{engine::CardReading, model::ir::Characteristics};

const NAME: &str = "Context example";
const ORIGINAL: &str = "Whenever this creature is dealt damage, put a +1/+1 counter on it.";
const OTHER_EVENT: &str =
    "Whenever a creature is dealt damage, put a +1/+1 counter on this creature.";

fn context() -> Characteristics {
    serde_json::from_value(
        json!({"supertypes":[],"card_types":["Creature"],"subtypes":[],
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
fn an_event_subject_proved_to_be_the_source_keeps_both_print_paths() {
    let reading = read(ORIGINAL);
    let object = &reading.abilities[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]
        ["placements"][0]["recipients"]["args"]["args"];
    assert_eq!(object, &json!({"kind":"Source"}));
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
fn editing_the_event_does_not_redirect_counters_to_another_creature() {
    let mut edited = read(ORIGINAL);
    let trail = engine()
        .card_key_of(NAME, &edited, &context(), ORIGINAL, true)
        .unwrap()
        .unwrap();
    let other = read(OTHER_EVENT);
    edited.abilities[0]["args"]["event"] = other.abilities[0]["args"]["event"].clone();
    assert_eq!(edited.abilities, other.abilities);
    let printed = engine()
        .print_card_keyed(NAME, &edited, &context(), &[])
        .unwrap()
        .unwrap();
    same(&edited, &printed);
    if let Some(text) = engine()
        .print_card_trail(NAME, &edited, &context(), &trail)
        .unwrap()
    {
        same(&edited, &text);
    }
    let mut counterfactual = edited.abilities.clone();
    counterfactual[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["placements"]
        [0]["recipients"]["args"]["args"] = json!({"kind":"EventSubject"});
    validate_abilities(&counterfactual).unwrap();
    assert_ne!(
        canonical_abilities(&counterfactual).unwrap(),
        edited.abilities
    );
}

#[test]
fn cast_conditions_and_damage_effects_print_their_current_source_roles() {
    for text in [
        "When you cast this spell, if {C} wasn't spent to cast it, draw a card.",
        "When you cast this spell, if {C} wasn't spent to cast it, target opponent may put a creature card from their hand onto the battlefield.",
        "Whenever this creature is dealt damage, it gains first strike until end of turn.",
    ] {
        let reading = read(text);
        let views = crate::model::canonical::event_subject_print_views(&reading.abilities).unwrap();
        assert!(!views.is_empty(), "{text}");
        let trail = engine()
            .card_key_of(NAME, &reading, &context(), text, true)
            .unwrap()
            .unwrap_or_else(|| panic!("no current-operand trail for {text}: {views:#?}"));
        assert_eq!(
            engine()
                .print_card_trail(NAME, &reading, &context(), &trail)
                .unwrap()
                .as_deref(),
            Some(text)
        );
        let printed = engine()
            .print_card_keyed(NAME, &reading, &context(), &[])
            .unwrap()
            .unwrap_or_else(|| panic!("no default print for {text}: {views:#?}"));
        same(&reading, &printed);
    }
}
