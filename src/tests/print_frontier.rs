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

#[test]
fn long_subtype_population_preserves_oracle_order_and_prints_current_operands() {
    let name = "Spider-Ham, Peter Porker";
    let text = "Animal May-Ham — Other Spiders, Boars, Bats, Bears, Birds, Cats, Dogs, Frogs, Jackals, Lizards, Mice, Otters, Rabbits, Raccoons, Rats, Squirrels, Turtles, and Wolves you control get +1/+1.";
    let parsed = engine().parse(name, text).unwrap();
    let [reading] = parsed.readings.as_slice() else {
        panic!("expected one reading for the subtype population");
    };
    let printed = engine()
        .print_keyed(name, reading, &[], &[])
        .unwrap()
        .expect("the subtype population has a default print");
    assert!(
        engine()
            .parse(name, &printed)
            .unwrap()
            .readings
            .contains(reading)
    );

    let trail = engine()
        .trail_of(name, reading, text)
        .unwrap()
        .expect("Oracle order has an exact trail");
    assert_eq!(
        engine()
            .print_trail(name, reading, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );

    let edited_text = text.replace("Bats", "Apes");
    let edited = engine().parse(name, &edited_text).unwrap();
    let [edited_reading] = edited.readings.as_slice() else {
        panic!("expected one reading after changing a subtype");
    };
    assert_ne!(edited_reading, reading);
    let edited_print = engine()
        .print_keyed(name, edited_reading, &[], &[])
        .unwrap()
        .expect("the current subtype population has a default print");
    assert!(edited_print.contains("Apes"));
    assert!(
        engine()
            .parse(name, &edited_print)
            .unwrap()
            .readings
            .contains(edited_reading)
    );
    let edited_trail = engine()
        .trail_of(name, edited_reading, &edited_text)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine()
            .print_trail(name, edited_reading, &edited_trail)
            .unwrap()
            .as_deref(),
        Some(edited_text.as_str())
    );
}

#[test]
fn cast_spell_power_thresholds_print_independently() {
    let name = "Eshki, Temur's Roar";
    let text = "Whenever you cast a creature spell, put a +1/+1 counter on Eshki. If that spell's power is 4 or greater, draw a card. If that spell's power is 6 or greater, Eshki deals damage equal to Eshki's power to each opponent.";
    for text in [
        text.to_string(),
        text.replace("6 or greater", "7 or greater"),
    ] {
        let parsed = engine().parse(name, &text).unwrap();
        let [reading] = parsed.readings.as_slice() else {
            panic!("expected one reading for the two spell power thresholds");
        };
        let printed = engine()
            .print_keyed(name, reading, &[], &[])
            .unwrap()
            .unwrap();
        assert!(
            engine()
                .parse(name, &printed)
                .unwrap()
                .readings
                .contains(reading)
        );
        let exact = engine().trail_of(name, reading, &text).unwrap().unwrap();
        assert_eq!(
            engine()
                .print_trail(name, reading, &exact)
                .unwrap()
                .as_deref(),
            Some(text.as_str())
        );
    }
}

#[test]
fn foil_mode_title_preserves_oracle_capitalization() {
    let name = "You Find the Villains' Lair";
    let text = "Choose one —\n• Foil Their Scheme — Counter target spell.\n• Learn Their Secrets — Draw two cards, then discard two cards.";
    let parsed = engine().parse(name, text).unwrap();
    let [reading] = parsed.readings.as_slice() else {
        panic!("expected one reading for the named modes");
    };
    let trail = engine().trail_of(name, reading, text).unwrap().unwrap();
    assert_eq!(
        engine()
            .print_trail(name, reading, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
}

#[test]
fn separate_spent_colors_preserve_announcement_and_hand_observation() {
    let name = "River's Grasp";
    let text = "If {U} was spent to cast this spell, return up to one target creature to its owner's hand. If {B} was spent to cast this spell, target player reveals their hand, you choose a nonland card from it, then that player discards that card. (Do both if {U}{B} was spent.)";
    let parsed = engine().parse(name, text).unwrap();
    let [reading] = parsed.readings.as_slice() else {
        panic!("expected one reading for the independent mana conditions");
    };
    let printed = engine()
        .print_keyed(name, reading, &[], &[])
        .unwrap()
        .expect("the two conditional effects have a default print");
    let expected = crate::engine::semantic_abilities(reading).unwrap();
    assert!(
        engine()
            .parse(name, &printed)
            .unwrap()
            .readings
            .iter()
            .any(|reading| { crate::engine::semantic_abilities(reading).unwrap() == expected })
    );
    let trail = engine().trail_of(name, reading, text).unwrap().unwrap();
    assert_eq!(
        engine()
            .print_trail(name, reading, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
}
