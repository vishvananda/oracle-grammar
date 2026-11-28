use super::*;

#[test]
fn guided_print_can_keep_the_middle_pair_of_four_static_changes_together() {
    let name = "One with the Multiverse";
    let text = "You may look at the top card of your library any time.\n\
        You may play lands and cast spells from the top of your library.\n\
        Once during each of your turns, you may cast a spell from your hand or the top of your library without paying its mana cost.";
    let characteristics = crate::model::ir::Characteristics {
        card_types: vec![crate::model::mtg_types::CardType::Enchantment],
        subtypes: Vec::new(),
        supertypes: Vec::new(),
        mana_cost: None,
        color_indicator: Vec::new(),
        power_toughness: None,
        loyalty: None,
        defense: None,
    };
    let context = engine()
        .card_input_context(
            name,
            text,
            &characteristics,
            crate::engine::PrintedPowerToughness::Absent,
        )
        .unwrap();
    let parsed = engine()
        .parse_card_with_source(name, text, &context)
        .unwrap();
    let reading = &parsed.readings[0];
    let key = engine()
        .card_key_of_with_source(name, reading, &context, text, true)
        .unwrap()
        .expect("the joined middle pair has an exact print trail");
    assert_eq!(
        engine()
            .print_card_trail_with_source(name, reading, &context, &key)
            .unwrap()
            .as_deref(),
        Some(text)
    );
}

#[test]
fn a_reachable_text_has_no_frontier_and_a_longer_one_names_where_it_stops() {
    let text = "Draw a card.";
    let reading = engine().parse("Example", text).unwrap().readings.remove(0);
    assert!(
        engine()
            .print_frontier("Example", &reading, text)
            .unwrap()
            .is_none()
    );
    let frontier = engine()
        .print_frontier("Example", &reading, "Draw a card and gain 3 life.")
        .unwrap()
        .expect("an unreachable text has a frontier");
    // Every derivation shares "Draw a card"; none reaches the end of the text.
    let at = frontier["at"].as_u64().unwrap();
    assert!((3..8).contains(&at), "{frontier}");
    assert!(
        frontier["before"]
            .as_str()
            .unwrap()
            .starts_with("Draw a card")
    );
    assert!(!frontier["blamed"].as_array().unwrap().is_empty());
}
