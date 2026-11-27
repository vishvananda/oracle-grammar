use super::*;
use crate::{engine::CardReading, model::ir::Characteristics};

fn context() -> Characteristics {
    serde_json::from_value(
        json!({"supertypes":[],"card_types":["Instant"],"subtypes":[],"mana_cost":[],
        "color_indicator":[],"power_toughness":null,"loyalty":null,"defense":null}),
    )
    .unwrap()
}
fn read(text: &str) -> CardReading {
    let mut parsed = engine()
        .parse_card("Context example", text, &context())
        .unwrap();
    assert_eq!(parsed.readings.len(), 1, "{text}");
    parsed.readings.remove(0)
}
fn same(reading: &CardReading, text: &str) {
    let parsed = engine()
        .parse_card("Context example", text, &context())
        .unwrap();
    assert!(
        parsed
            .readings
            .iter()
            .any(|r| r.abilities == reading.abilities),
        "{text}"
    );
}
#[test]
fn immediate_amounts_and_token_stats_keep_exact_and_default_printing_after_capture_inlining() {
    for text in [
        "You gain X life, where X is the number of creatures you control.",
        "Create an X/X green Saproling creature token, where X is the number of creatures you control.",
        "Create an X/X black Horror creature token, where X is the number of creatures that died this turn.",
        "You gain X life, where X is twice the damage dealt to you so far this turn by artifacts.",
        "Context example deals X damage to target creature, where X is 2 plus the number of cards in your graveyard that are instant cards, sorcery cards, and/or have an Adventure.",
        "Target player reveals their hand. You choose a nonland card from it. That player puts that card into their library third from the top.",
    ] {
        let reading = read(text);
        assert!(!reading.abilities.to_string().contains("BindValue"));
        let trail = engine()
            .card_key_of("Context example", &reading, &context(), text, true)
            .unwrap()
            .unwrap();
        assert_eq!(
            engine()
                .print_card_trail("Context example", &reading, &context(), &trail)
                .unwrap()
                .as_deref(),
            Some(text)
        );
        let printed = engine()
            .print_card_keyed("Context example", &reading, &context(), &[])
            .unwrap()
            .unwrap();
        same(&reading, &printed);
    }
}
#[test]
fn an_edited_immediate_definition_prints_its_current_counted_population() {
    let mut edited = read("You gain X life, where X is the number of creatures you control.");
    let old = engine()
        .card_key_of(
            "Context example",
            &edited,
            &context(),
            "You gain X life, where X is the number of creatures you control.",
            true,
        )
        .unwrap()
        .unwrap();
    let changed = read("You gain X life, where X is the number of lands you control.");
    edited.abilities = changed.abilities;
    let printed = engine()
        .print_card_keyed("Context example", &edited, &context(), &[])
        .unwrap()
        .unwrap();
    same(&edited, &printed);
    if let Some(text) = engine()
        .print_card_trail("Context example", &edited, &context(), &old)
        .unwrap()
    {
        same(&edited, &text);
    }
}
