use super::*;

#[test]
fn drafting_and_placing_a_card_keeps_the_drafted_card_as_the_move_subject() {
    let text = "When this creature enters, you may sacrifice a land. If you do, draft a card from the Planets Spellbook and put it onto the battlefield tapped.";
    let parsed = engine().parse("Brood Astronomer", text).unwrap();
    let [reading] = parsed.readings.as_slice() else {
        panic!("expected one reading for the draft and placement");
    };
    let steps = &reading[0]["args"]["body"]["instructions"][1]["args"]["then"]["instructions"];
    let draft = &steps[0]["args"];
    assert_eq!(draft["operation"]["kind"], "DraftFromSpellbook");
    let moved = &steps[1]["args"]["operation"]["args"]["objects"];
    assert_eq!(moved["args"]["kind"], "Result");
    assert_eq!(moved["args"]["args"]["result"], draft["result"]);
}

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

#[test]
fn event_property_printing_names_the_referenced_spell_or_creature() {
    for (name, source, types, property) in [
        (
            "Erratic Cyclops",
            "Trample\nWhenever you cast an instant or sorcery spell, this creature gets +X/+0 until end of turn, where X is that spell's mana value.",
            vec!["Creature".into()],
            "that spell's mana value",
        ),
        (
            "Arni, Renowned Champion",
            "Trample\nWhenever another creature you control enters, Arni gets +X/+0 until end of turn, where X is that creature's power.",
            vec!["Legendary".into(), "Creature".into()],
            "that creature's power",
        ),
    ] {
        let parsed = engine().parse(name, source).unwrap();
        assert_eq!(parsed.readings.len(), 1);
        let reading = &parsed.readings[0];
        let text = engine()
            .print_keyed(name, reading, &types, &[])
            .unwrap()
            .expect("an explicit event property prints without a key");
        assert!(text.contains(property), "{text}");
        assert!(
            engine()
                .parse(name, &text)
                .unwrap()
                .readings
                .contains(reading)
        );
    }
}

#[test]
fn a_modal_label_keeps_sentence_initial_capitalization_in_exact_output() {
    let name = "Pip-Boy 3000";
    let text = "Whenever equipped creature attacks, choose one —\n• Sort Inventory — Draw a card, then discard a card.\n• Pick a Perk — Put a +1/+1 counter on that creature.\n• Check Map — Untap up to two target lands.\nEquip {2}";
    let parsed = engine().parse(name, text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let reading = &parsed.readings[0];
    let trail = engine().trail_of(name, reading, text).unwrap().unwrap();
    assert_eq!(
        engine()
            .print_trail(name, reading, &trail)
            .unwrap()
            .as_deref(),
        Some(text),
    );
}
