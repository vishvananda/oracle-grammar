use super::*;
use serde_json::json;

fn raw(id: &str, name: &str, text: &str) -> Value {
    json!({"oracle_id":id,"name":name,"layout":"normal","type_line":"Instant","mana_cost":"{U}","oracle_text":text})
}

#[test]
fn sticker_sheet_export_preserves_costs_abilities_and_stats() {
    let engine = crate::test_support::engine();
    let name = "Three Hot Dog Words";
    let text = "{TK}{TK} — Flying\n{TK}{TK}{TK} — 4/5";
    let faces = scryfall(&[json!({
        "oracle_id":"sheet", "layout":"normal", "name":name,
        "type_line":"Stickers", "oracle_text":text
    })])
    .unwrap();
    let row = outcome(engine, &faces[0]);
    assert_eq!(row["status"], "unique", "{}", row);
    assert_eq!(row["format_version"], 2);
    let mut card: ir::UnifiedCard =
        serde_json::from_value(row["readings"][0]["card"].clone()).unwrap();
    assert!(card.faces.is_empty());
    validate::unified(&card).unwrap();
    assert_eq!(
        engine.print_sticker_sheet(&card).unwrap(),
        Some((name.into(), text.into()))
    );
    let trail = engine
        .sticker_sheet_trail_of(&card, name, text)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine.print_sticker_sheet_trail(&card, &trail).unwrap(),
        Some((name.into(), text.into()))
    );
    let ir::Operand::Stickers(stickers) = &mut card.data[0].value else {
        panic!("expected sticker metadata");
    };
    let ir::Sticker::PowerToughness { power, .. } = &mut stickers[4] else {
        panic!("expected power and toughness sticker");
    };
    *power = 6;
    assert_eq!(
        engine.print_sticker_sheet(&card).unwrap(),
        Some((name.into(), text.replace("4/5", "6/5")))
    );
    assert_eq!(
        engine.print_sticker_sheet_trail(&card, &trail).unwrap(),
        Some((name.into(), text.replace("4/5", "6/5")))
    );
    assert!(
        engine
            .sticker_sheet_trail_of(&card, name, text)
            .unwrap()
            .is_none()
    );
    assert!(
        engine
            .parse_sticker_sheet(name, &format!("{text}\ninvalid tail"))
            .is_err()
    );
    assert!(
        engine
            .parse_sticker_sheet(
                name,
                "{TK} — Whenever Three Hot Dog Words attacks, you gain 1 life.",
            )
            .is_err()
    );
}

#[test]
fn plane_type_line_keeps_the_printed_compound_subtype() {
    let faces = scryfall(&[json!({
        "oracle_id":"plane", "layout":"planar", "name":"Example Plane",
        "type_line":"Plane — Unknown Planet",
        "oracle_text":"Whenever chaos ensues, draw a card."
    })])
    .unwrap();
    let (characteristics, _) = faces[0].context.as_ref().unwrap();
    assert_eq!(
        serde_json::to_value(&characteristics.subtypes).unwrap(),
        json!(["UnknownPlanet"])
    );
}

#[test]
fn source_printer_can_split_coalesced_triggers() {
    let engine = crate::test_support::engine();
    let name = "Gideon the Oathless";
    let text = concat!(
        "Ward—Discard a card.\n",
        "Whenever a creature an opponent controls enters, Gideon deals 1 damage to that player.\n",
        "Whenever an opponent activates a loyalty ability, Gideon deals 1 damage to that player."
    );
    let faces = scryfall(&[json!({
        "oracle_id":"joined", "layout":"normal", "name":name,
        "type_line":"Legendary Creature — Human Soldier", "mana_cost":"{2}{W}",
        "power":"3", "toughness":"3", "oracle_text":text,
    })])
    .unwrap();
    let row = outcome(engine, &faces[0]);
    assert_eq!(row["status"], "unique", "{}", row);
    let context = serde_json::from_value(row["context"].clone()).unwrap();
    let reading = CardReading {
        abilities: row["readings"][0]["card"]["abilities"].clone(),
        notes: Vec::new(),
    };
    assert_eq!(reading.abilities.as_array().unwrap().len(), 2);
    assert!(
        engine
            .print_card_keyed_with_source(name, &reading, &context, &[])
            .unwrap()
            .is_some()
    );
}

#[test]
fn source_identity_includes_record_and_face_position() {
    let records = vec![
        raw("one", "Same", "Draw a card."),
        json!({"oracle_id":"two","name":"Pair","layout":"split","card_faces":[
            {"name":"Same","type_line":"Instant","oracle_text":"Draw two cards."},
            {"name":"Other","type_line":"Sorcery","oracle_text":"Draw a card."}
        ]}),
    ];
    let faces = scryfall(&records).unwrap();
    assert_eq!(faces.len(), 3);
    assert_ne!(faces[0].identity, faces[1].identity);
    assert_eq!(faces[1].identity["face_index"], 0);
    assert_eq!(faces[2].identity["face_index"], 1);
}

#[test]
fn superscript_draw_uses_a_typed_exponent() {
    let engine = crate::test_support::engine();
    let text = concat!(
        "Target player draws 2ˣ cards. ",
        "(2⁰ = 1, 2¹ = 2, 2² = 4, 2³ = 8, 2⁴ = 16, 2⁵ = 32, and so on.)"
    );
    let mut source = raw("power", "Exponential Example", text);
    source["mana_cost"] = json!("{X}{U}");
    let faces = scryfall(&[source.clone()]).unwrap();
    let row = outcome(engine, &faces[0]);
    assert_eq!(row["status"], "unique", "{}", row);
    let spell = &row["readings"][0]["card"]["abilities"][0];
    assert_eq!(
        spell["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"],
        json!({"kind":"Exponentiate","args":[{"kind":"Constant","args":2},{"kind":"AnnouncedX"}]})
    );
    source["oracle_text"] = json!(text.replace("2ˣ", "3ˣ"));
    assert_eq!(
        outcome(engine, &scryfall(&[source]).unwrap()[0])["stage"],
        "lexical"
    );
}

#[test]
fn rainbow_cost_note_checks_the_source_symbol() {
    let engine = crate::test_support::engine();
    let text = concat!(
        "({Z} is rainbow mana, one mana from any source that could add ",
        "two or more colors of mana. This card is all colors.)\nFlying"
    );
    let mut source = raw("rainbow", "Colored Example", text);
    source["mana_cost"] = json!("{2}{Z}");
    source["type_line"] = json!("Creature");
    let row = outcome(engine, &scryfall(&[source.clone()]).unwrap()[0]);
    assert_eq!(row["status"], "unique", "{}", row);
    assert_eq!(
        row["readings"][0]["card"]["characteristics"]["mana_cost"],
        json!([{"Generic":2},"Rainbow"])
    );
    assert_eq!(
        row["readings"][0]["notes"],
        json!([{"kind":"ManaCost","args":{"slot":1}}])
    );
    source["mana_cost"] = json!("{2}{W}");
    assert_eq!(
        outcome(engine, &scryfall(&[source]).unwrap()[0])["stage"],
        "binding"
    );
}

#[test]
fn native_export_validates_and_keeps_refusals_explicit() {
    let engine = crate::test_support::engine();
    let mut unsupported = raw("three", "Unsupported stats", "Flying");
    unsupported["power"] = json!("*");
    let faces = scryfall(&[
        raw("one", "Synthetic draw", "Draw a card."),
        raw("two", "Unknown", "Nonsense not in Oracle."),
        unsupported,
    ])
    .unwrap();
    let rows: Vec<_> = faces.iter().map(|f| outcome(engine, f)).collect();
    assert_eq!(rows[0]["status"], "unique", "{}", rows[0]);
    let card: ir::Card = serde_json::from_value(rows[0]["readings"][0]["card"].clone()).unwrap();
    validate::card(&card).unwrap();
    assert_eq!(rows[1]["status"], "refused");
    assert_eq!(rows[2]["stage"], "unsupported_source");
    assert_eq!(rows[2]["readings"], json!([]));
}

#[test]
fn empower_jace_explanations_keep_the_actual_count() {
    let engine = crate::test_support::engine();
    let valid = concat!(
        "Empower Jace 2. (Put two loyalty counters on a Jace token you control. ",
        "If you don't control one, first create a blue Jace planeswalker token ",
        "with \"[−1]: Surveil 1\" and \"[−3]: Draw a card.\")",
    );
    let changed = valid.replace("two loyalty", "three loyalty");
    let faces = scryfall(&[
        raw("valid", "Synthetic loyalty", valid),
        raw("inconsistent", "Synthetic loyalty", &changed),
    ])
    .unwrap();
    let parsed = outcome(engine, &faces[0]);
    assert_eq!(parsed["status"], "unique", "{parsed}");
    let abilities = &parsed["readings"][0]["card"]["abilities"];
    let operation =
        &abilities[0]["args"]["ability"]["args"]["body"]["instructions"][0]["args"]["operation"];
    assert_eq!(
        operation["args"]["action"]["EmpowerJace"]["amount"],
        json!({"kind":"Constant","args":2}),
        "{abilities}"
    );
    assert_eq!(outcome(engine, &faces[1])["status"], "refused");
}

#[test]
fn a_prepared_entry_licenses_the_prepared_cast_reminder() {
    let engine = crate::test_support::engine();
    let reminder =
        "(While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)";
    let mut prepared = raw(
        "prepared",
        "Synthetic preparer",
        &format!("This creature enters prepared. {reminder}"),
    );
    prepared["type_line"] = json!("Creature — Human");
    prepared["power"] = json!("2");
    prepared["toughness"] = json!("2");
    let mut unrelated = prepared.clone();
    unrelated["oracle_id"] = json!("unrelated");
    unrelated["oracle_text"] = json!(format!("This creature enters tapped. {reminder}"));
    let faces = scryfall(&[prepared, unrelated]).unwrap();
    let parsed = outcome(engine, &faces[0]);
    assert_eq!(parsed["status"], "unique", "{parsed}");
    let replacement = &parsed["readings"][0]["card"]["abilities"][0]["args"]["ability"];
    assert_eq!(replacement["kind"], "Replacement");
    assert_eq!(
        replacement["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["change"],
        json!({"Entry":"Prepared"})
    );
    assert_eq!(outcome(engine, &faces[1])["status"], "refused");
}

#[test]
fn resource_limit_cannot_export_partial_success() {
    let mut engine = Engine::load().unwrap();
    engine.limits.steps = 0;
    let faces = scryfall(&[raw("one", "Draw", "Draw a card.")]).unwrap();
    let row = outcome(&engine, &faces[0]);
    assert_eq!(row["status"], "limit");
    assert_eq!(row["readings"], json!([]));
}

#[test]
fn every_supported_reading_survives_export() {
    let one: Vec<ir::Ability> = serde_json::from_value(
        crate::test_support::engine()
            .parse("Synthetic example", "Draw a card.")
            .unwrap()
            .readings[0]
            .clone(),
    )
    .unwrap();
    let two: Vec<ir::Ability> = serde_json::from_value(
        crate::test_support::engine()
            .parse("Synthetic example", "Draw two cards.")
            .unwrap()
            .readings[0]
            .clone(),
    )
    .unwrap();
    let readings = vec![
        CardReading {
            abilities: json!(one),
            notes: vec![],
        },
        CardReading {
            abilities: json!(two),
            notes: vec![],
        },
    ];
    let result = cards(
        "Draw",
        &crate::engine::empty_card_characteristics(),
        readings,
    )
    .unwrap();
    assert_eq!(result.len(), 2);
    assert_ne!(result[0]["card"], result[1]["card"]);
}

#[test]
fn provider_fields_are_converted_without_reference_rules() {
    let face = json!({"oracle_id":"one","name":"Body","layout":"normal", "type_line":"Legendary Creature — Elf Druid", "mana_cost":"{2}{G}{W/U}{B/P}", "power":"1", "toughness":"2", "oracle_text":"Flying"});
    let faces = scryfall(&[face]).unwrap();
    let (c, printed) = faces[0].context.as_ref().unwrap();
    assert_eq!(c.subtypes.len(), 2);
    assert_eq!(c.supertypes.len(), 1);
    assert_eq!(c.power_toughness, Some((1, 2)));
    assert_eq!(c.mana_cost.as_ref().unwrap().len(), 4);
    assert_eq!(printed.values(), Some((1, 2)));
}

#[test]
fn compound_subtypes_can_share_a_type_line() {
    let mut known = raw("known", "Synthetic doctor", "Flying");
    known["type_line"] = json!("Legendary Creature — Time Lord Doctor");
    let mut unknown = known.clone();
    unknown["type_line"] = json!("Legendary Creature — Time Lord UnknownSubtype");
    let mut over_limit = known.clone();
    over_limit["type_line"] = json!(format!("Creature — {}", "Doctor ".repeat(33)));
    let faces = scryfall(&[known, unknown, over_limit]).unwrap();
    assert_eq!(
        json!(faces[0].context.as_ref().unwrap().0.subtypes),
        json!(["TimeLord", "Doctor"])
    );
    assert!(faces[1].context.is_err());
    assert!(faces[2].context.is_err());
}

#[test]
fn variable_stats_require_a_matching_source_definition() {
    let text = "This creature's power is equal to 1 plus the number of creatures you control.";
    let mut defined = raw("defined", "Synthetic Star", text);
    defined["type_line"] = json!("Creature — Elf");
    defined["power"] = json!("1+*");
    defined["toughness"] = json!("3");
    let mut mismatched = defined.clone();
    mismatched["power"] = json!("2+*");
    let mut unrelated = defined.clone();
    unrelated["oracle_text"] = json!("Flying");
    let faces = scryfall(&[defined, mismatched, unrelated]).unwrap();
    let engine = crate::test_support::engine();
    let row = outcome(engine, &faces[0]);
    assert_eq!(row["status"], "unique", "{row}");
    assert_eq!(
        row["readings"][0]["card"]["characteristics"]["power_toughness"],
        json!([1, 3])
    );
    let context: crate::engine::CardInputContext =
        serde_json::from_value(row["context"].clone()).unwrap();
    assert_eq!(context.printed_power_toughness.values(), None);
    let reading = CardReading {
        abilities: row["readings"][0]["card"]["abilities"].clone(),
        notes: vec![],
    };
    let trail = engine
        .card_key_of_with_source("Synthetic Star", &reading, &context, text, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .print_card_trail_with_source("Synthetic Star", &reading, &context, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
    for face in &faces[1..] {
        let rejected = outcome(engine, face);
        assert_eq!(rejected["stage"], "binding", "{rejected}");
        assert_eq!(rejected["readings"], json!([]));
    }
}

#[test]
fn source_print_keys_preserve_legendary_short_names() {
    let mut record = raw(
        "legendary",
        "Anaxor Ember Speaker",
        "When Anaxor enters, draw a card.",
    );
    record["type_line"] = json!("Legendary Creature — Human Wizard");
    record["power"] = json!("2");
    record["toughness"] = json!("3");
    let faces = scryfall(&[record]).unwrap();
    let face = &faces[0];
    let engine = crate::test_support::engine();
    let row = outcome(engine, face);
    assert_eq!(row["status"], "unique", "{row}");
    let context: crate::engine::CardInputContext =
        serde_json::from_value(row["context"].clone()).unwrap();
    let reading = CardReading {
        abilities: row["readings"][0]["card"]["abilities"].clone(),
        notes: vec![],
    };
    let text = "When Anaxor enters, draw a card.";
    let trail = engine
        .card_key_of_with_source("Anaxor Ember Speaker", &reading, &context, text, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .print_card_trail_with_source("Anaxor Ember Speaker", &reading, &context, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
}

#[test]
fn printed_cost_explanations_roundtrip_with_matching_mana() {
    let engine = crate::test_support::engine();
    for (symbol, explanation) in [
        ("{C}", "{C} represents colorless mana."),
        ("{S}", "{S} can be paid with one mana from a snow source."),
    ] {
        let text = format!("({explanation})\nDraw a card.");
        let mut input = raw("printed", "Synthetic", &text);
        let mismatched = input.clone();
        input["mana_cost"] = json!(symbol);
        let faces = scryfall(&[input, mismatched]).unwrap();
        let context = &faces[0].context.as_ref().unwrap().0;
        let parsed = engine.parse_card("Synthetic", &text, context).unwrap();
        let reading = &parsed.readings[0];
        let key = engine
            .card_key_of("Synthetic", reading, context, &text, true)
            .unwrap()
            .unwrap();
        assert_eq!(
            engine
                .print_card_trail("Synthetic", reading, context, &key)
                .unwrap()
                .as_deref(),
            Some(text.as_str())
        );
        assert!(
            engine
                .parse_card("Synthetic", &text, &faces[1].context.as_ref().unwrap().0)
                .is_err()
        );
    }
}

#[test]
fn entry_or_death_trigger_roundtrips_with_a_shared_creature_filter() {
    let engine = crate::test_support::engine();
    let context = crate::engine::empty_card_characteristics();
    let name = "Synthetic";
    let text = "Whenever another creature you control enters or dies, you gain 1 life.";
    let reading = &engine.parse_card(name, text, &context).unwrap().readings[0];
    let key = engine
        .card_key_of(name, reading, &context, text, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .print_card_trail(name, reading, &context, &key)
            .unwrap()
            .as_deref(),
        Some(text)
    );
}

#[test]
fn contextual_reminders_check_keywords_actions_and_costs() {
    let engine = crate::test_support::engine();
    let examples = [
        (
            "Target creature gains menace until end of turn.",
            "It can't be blocked except by two or more creatures.",
        ),
        (
            "Counter target creature or planeswalker spell. Venture into the dungeon.",
            "Enter the first room or advance to the next room.",
        ),
        (
            "{S}, {T}: Tap target artifact or creature.",
            "{S} can be paid with one mana from a snow source.",
        ),
    ];
    for (index, (ability, note)) in examples.into_iter().enumerate() {
        let text = format!("{ability} ({note})");
        let input = raw("contextual", "Synthetic", &text);
        let faces = scryfall(&[input]).unwrap();
        let row = outcome(engine, &faces[0]);
        assert_eq!(row["status"], "unique", "{row}");
        let reading = CardReading {
            abilities: row["readings"][0]["card"]["abilities"].clone(),
            notes: vec![],
        };
        let context = &faces[0].context.as_ref().unwrap().0;
        let trail = engine
            .card_key_of(&faces[0].name, &reading, context, &text, true)
            .unwrap()
            .unwrap();
        assert_eq!(
            engine
                .print_card_trail(&faces[0].name, &reading, context, &trail)
                .unwrap()
                .as_deref(),
            Some(text.as_str())
        );
        let wrong = format!("{ability} ({})", examples[(index + 1) % examples.len()].1);
        assert!(engine.parse_card(&faces[0].name, &wrong, context).is_err());
    }
}

#[test]
fn granted_reach_explains_the_created_token() {
    let name = "Gloomwidow's Feast";
    let text = "Destroy target creature with flying. If that creature was blue or black, create a 1/2 green Spider creature token with reach. (It can block creatures with flying.)";
    let engine = crate::test_support::engine();
    let context = crate::engine::empty_card_characteristics();
    let parsed = engine.parse_card(name, text, &context).unwrap();
    let printed = engine
        .print_card_keyed(name, &parsed.readings[0], &context, &[])
        .unwrap()
        .unwrap();
    assert!(
        printed.ends_with("(It can block creatures with flying.)"),
        "{printed}"
    );
}

#[test]
fn token_explanations_require_the_corresponding_predefined_recipe() {
    let engine = crate::test_support::engine();
    let context = crate::engine::empty_card_characteristics();
    let name = "Halo Scarab";
    assert!(
        engine
            .parse_card(
                name,
                "Create a Treasure token. (Activate only as an instant.)",
                &context
            )
            .is_err()
    );
    let text = "{2}, Exile this card from your graveyard: Create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")";
    let reading = &engine.parse_card(name, text, &context).unwrap().readings[0];
    let key = engine
        .card_key_of(name, reading, &context, text, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .print_card_trail(name, reading, &context, &key)
            .unwrap()
            .as_deref(),
        Some(text)
    );
    assert!(
        engine
            .parse_card(
                name,
                &text.replace("a Treasure token", "a Food token"),
                &context
            )
            .is_err()
    );
}

#[test]
fn contextual_action_amount_is_checked_in_both_directions() {
    let engine = crate::test_support::engine();
    let context = crate::engine::empty_card_characteristics();
    let name = "Synthetic Giant";
    let text = "{5}{R}: Monstrosity 3. (If this creature isn't monstrous, put three +1/+1 counters on it and it becomes monstrous.)";
    let reading = &engine.parse_card(name, text, &context).unwrap().readings[0];
    let key = engine
        .card_key_of(name, reading, &context, text, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .print_card_trail(name, reading, &context, &key)
            .unwrap()
            .as_deref(),
        Some(text)
    );
    assert!(
        engine
            .parse_card(name, &text.replace("three", "four"), &context)
            .is_err()
    );
    let mut edited = reading.clone();
    let action = edited.abilities.pointer_mut("/0/args/ability/args/body/instructions/0/args/operation/args/action/Monstrosity/amount/args").unwrap();
    *action = json!(6);
    let printed = engine
        .print_card_keyed(name, &edited, &context, &[])
        .unwrap()
        .unwrap();
    assert!(
        printed.contains("Monstrosity 6.") && printed.contains("six +1/+1 counters"),
        "{printed}"
    );
}

#[test]
fn mechanic_reminders_verify_repeated_amounts_and_costs() {
    let engine = crate::test_support::engine();
    let context = crate::engine::empty_card_characteristics();
    let examples = [
        (
            "Soulshift 4 (When this creature dies, you may return target Spirit card with mana value 4 or less from your graveyard to your hand.)",
            "value 4",
            "value 5",
        ),
        (
            "Reconfigure {2} ({2}: Attach to target creature you control; or unattach from a creature. Reconfigure only as a sorcery. While attached, this isn't a creature.)",
            "({2}:",
            "({3}:",
        ),
        (
            "Backup 2 (When this creature enters, put two +1/+1 counters on target creature. If that's another creature, it gains the following ability until end of turn.)\nFlying",
            "put two",
            "put three",
        ),
    ];
    for (text, from, to) in examples {
        let reading = &engine
            .parse_card("Synthetic", text, &context)
            .unwrap()
            .readings[0];
        let key = engine
            .card_key_of("Synthetic", reading, &context, text, true)
            .unwrap()
            .unwrap();
        assert_eq!(
            engine
                .print_card_trail("Synthetic", reading, &context, &key)
                .unwrap()
                .as_deref(),
            Some(text)
        );
        assert!(
            engine
                .parse_card("Synthetic", &text.replace(from, to), &context)
                .is_err()
        );
    }
}

#[test]
fn typecycling_reminder_agrees_on_payment_and_search() {
    let engine = crate::test_support::engine();
    let context = crate::engine::empty_card_characteristics();
    let name = "Synthetic";
    for (cycling, noun) in [("Islandcycling", "an Island"), ("Swampcycling", "a Swamp")] {
        let text = format!(
            "{cycling} {{2}} ({{2}}, Discard this card: Search your library for {noun} card, reveal it, put it into your hand, then shuffle.)"
        );
        let reading = &engine.parse_card(name, &text, &context).unwrap().readings[0];
        let key = engine
            .card_key_of(name, reading, &context, &text, true)
            .unwrap()
            .unwrap();
        assert_eq!(
            engine
                .print_card_trail(name, reading, &context, &key)
                .unwrap()
                .as_deref(),
            Some(text.as_str())
        );
        for bad in [text.replace("({2}", "({3}"), text.replace(noun, "a Plains")] {
            assert!(engine.parse_card(name, &bad, &context).is_err());
        }
        let mut edited = reading.clone();
        *edited
            .abilities
            .pointer_mut("/0/args/ability/args/ability/args/cost/parts/0/Mana/0/Generic")
            .unwrap() = json!(3);
        assert_eq!(
            engine
                .print_card_keyed(name, &edited, &context, &[])
                .unwrap()
                .unwrap(),
            text.replace("{2}", "{3}")
        );
    }
}

#[test]
fn earthbend_and_copy_explanations_check_current_operands() {
    let engine = crate::test_support::engine();
    let context = crate::engine::empty_card_characteristics();
    let name = "Synthetic";
    let earthbend = "When this creature dies, earthbend 2. (Target land you control becomes a 0/0 creature with haste that's still a land. Put two +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)";
    let copy = "Whenever you cast a creature spell, copy it. (The copy becomes a token.)";
    for (text, from, to) in [
        (earthbend, "Put two", "Put three"),
        (copy, "a creature", "an instant"),
    ] {
        let parsed = engine.parse_card(name, text, &context);
        assert!(parsed.is_ok(), "{text}: {parsed:?}");
        assert!(
            engine
                .parse_card(name, &text.replace(from, to), &context)
                .is_err()
        );
    }
}

#[test]
fn saga_chapter_reminder_keeps_the_chapter_trigger() {
    let engine = crate::test_support::engine();
    let context = crate::engine::empty_card_characteristics();
    let name = "Synthetic";
    let text = "(As this Saga enters and after your draw step, add a lore counter. Sacrifice after III.)\nI, II — Create a 2/2 green Alien Salamander creature token with islandwalk. (It can't be blocked as long as defending player controls an Island.)\nIII — Draw a card.";
    let reading = &engine.parse_card(name, text, &context).unwrap().readings[0];
    let key = engine
        .card_key_of(name, reading, &context, text, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .print_card_trail(name, reading, &context, &key)
            .unwrap()
            .as_deref(),
        Some(text)
    );
    assert!(
        engine
            .parse_card(name, &text.replace("an Island", "a Swamp"), &context)
            .is_err()
    );
}

#[test]
fn layout_notes_require_the_printed_battle_or_scheme() {
    let battle = "(As a Siege enters, choose an opponent to protect it. You and others can attack it. When it's defeated, exile it, then cast it transformed.)\nWhen this Siege enters, draw a card.";
    let scheme = "(An ongoing scheme remains face up until it's abandoned.)\nAt the beginning of your upkeep, draw a card.";
    let records = [
        json!({"oracle_id":"battle","name":"Front // Back","layout":"transform","card_faces":[
            {"name":"Front","type_line":"Battle — Siege","defense":"4","oracle_text":battle},
            {"name":"Back","type_line":"Creature — Elf","power":"1","toughness":"2","oracle_text":"Flying"}
        ]}),
        json!({"oracle_id":"scheme","name":"Synthetic scheme","layout":"scheme","type_line":"Ongoing Scheme","oracle_text":scheme}),
        json!({"oracle_id":"wrong","name":"Non-ongoing scheme","layout":"scheme","type_line":"Scheme","oracle_text":scheme}),
    ];
    let faces = scryfall(&records).unwrap();
    let engine = crate::test_support::engine();
    for (index, note, text) in [
        (0, LayoutReminder::Siege, battle),
        (2, LayoutReminder::OngoingScheme, scheme),
    ] {
        let face = &faces[index];
        let row = outcome(engine, face);
        assert_eq!(row["status"], "unique", "{row}");
        let reading = CardReading {
            abilities: row["readings"][0]["card"]["abilities"].clone(),
            notes: vec![CardNote::LayoutReminder(note)],
        };
        let context = &face.context.as_ref().unwrap().0;
        let trail = engine
            .card_key_of(&face.name, &reading, context, text, true)
            .unwrap()
            .unwrap();
        assert_eq!(
            engine
                .print_card_trail(&face.name, &reading, context, &trail)
                .unwrap()
                .as_deref(),
            Some(text)
        );
    }
    assert_eq!(outcome(engine, &faces[3])["stage"], "binding");
}

#[test]
fn ambiguity_is_reported_through_the_complete_export_path() {
    let mut module = crate::engine::module_source("card_notes").unwrap();
    module = module.replacen("card_text ::=", "card_text ::= export_alternative |", 1);
    module.push_str("\n/* @sem CardSurface::CardSurface{abilities:[Keyword(Haste)]} */\nexport_alternative ::= \"flying\"\n");
    let engine = Engine::load_with(&[("card_notes".into(), module)]).unwrap();
    let faces = scryfall(&[raw("one", "Synthetic ambiguity", "Flying")]).unwrap();
    let row = outcome(&engine, &faces[0]);
    assert_eq!(row["status"], "ambiguous", "{row}");
    assert_eq!(row["semantic_readings"], 2);
    assert_eq!(row["readings"].as_array().unwrap().len(), 2);
}

#[test]
fn absent_mana_cost_is_not_a_zero_mana_cost() {
    let mut land = raw("one", "Plain land", "");
    land["type_line"] = json!("Land");
    land["mana_cost"] = json!("");
    let mut zero = raw("two", "Zero", "");
    zero["mana_cost"] = json!("{0}");
    let faces = scryfall(&[land, zero]).unwrap();
    assert!(faces[0].context.as_ref().unwrap().0.mana_cost.is_none());
    assert_eq!(
        serde_json::to_value(&faces[1].context.as_ref().unwrap().0.mana_cost).unwrap(),
        json!([{"Generic":0}])
    );
}

#[test]
fn presentation_keeps_native_meaning_and_exact_wording() {
    let engine = crate::test_support::engine();
    for (plain, text, label, reminder) in [
        (
            "Draw a card.",
            "Beacon of Hope — Draw a card.",
            Some("Beacon of Hope"),
            false,
        ),
        (
            "Draw a card.",
            "A Test of Your Reflexes! — Draw a card.",
            Some("A Test of Your Reflexes!"),
            false,
        ),
        (
            "Draw three cards.\nIf you cast this spell during your main phase, instead scry 3, then draw three cards.",
            "Draw three cards.\nAddendum — If you cast this spell during your main phase, instead scry 3, then draw three cards.",
            Some("Addendum"),
            false,
        ),
        (
            "Tap target creature.\nIf you control three or more artifacts, exile that creature.",
            "Tap target creature.\nMetalcraft — If you control three or more artifacts, exile that creature.",
            Some("Metalcraft"),
            false,
        ),
        (
            "Counter target spell.\nSurveil 1.",
            "Counter target spell.\nSurveil 1. (Look at the top card of your library. You may put it into your graveyard.)",
            None,
            true,
        ),
        (
            "Whenever a land you control enters, create a Treasure token.",
            "Landfall — Whenever a land you control enters, create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")",
            Some("Landfall"),
            true,
        ),
        (
            "Choose one —\n• Draw a card.\n• Scry 1.\nEntwine {2}",
            "Choose one —\n• Draw a card.\n• Scry 1.\nEntwine {2} (Choose both if you pay the entwine cost.)",
            None,
            true,
        ),
        (
            "Partner",
            "Partner (You can have two commanders if both have partner.)",
            None,
            true,
        ),
        (
            "Echo {1}{G}",
            "Echo {1}{G} (At the beginning of your upkeep, if this came under your control since the beginning of your last upkeep, sacrifice it unless you pay its echo cost.)",
            None,
            true,
        ),
        (
            "Crew 2",
            "Crew 2 (Tap any number of creatures you control with total power 2 or more: This Vehicle becomes an artifact creature until end of turn.)",
            None,
            true,
        ),
        (
            "Spree\n+ {2} — Draw a card.\n+ {1} — Scry 1.",
            "Spree (Choose one or more additional costs.)\n+ {2} — Draw a card.\n+ {1} — Scry 1.",
            None,
            true,
        ),
        (
            "Exhaust — {2}: Draw a card.",
            "Exhaust — {2}: Draw a card. (Activate each exhaust ability only once.)",
            None,
            true,
        ),
        (
            "When this creature enters, tap target creature an opponent controls and put a stun counter on it.",
            "When this creature enters, tap target creature an opponent controls and put a stun counter on it. (If a permanent with a stun counter would become untapped, remove one from it instead.)",
            None,
            true,
        ),
        (
            "Tap target creature and put three stun counters on it.\nScry 1.",
            "Tap target creature and put three stun counters on it. (If a permanent with a stun counter would become untapped, remove one from it instead.)\nScry 1. (Look at the top card of your library. You may put that card on the bottom.)",
            None,
            true,
        ),
    ] {
        let name = "Synthetic example";
        let faces = scryfall(&[raw("one", name, text), raw("two", name, plain)]).unwrap();
        let row = outcome(engine, &faces[0]);
        assert_eq!(row["status"], "unique", "{text}: {row}");
        let ability = &row["readings"][0]["card"]["abilities"];
        assert_eq!(ability.as_array().unwrap().len(), 1, "{text}: {row}");
        assert_eq!(ability[0]["kind"], "WithPresentation", "{text}");
        assert_eq!(ability[0]["args"]["label"], json!(label), "{text}");
        assert_eq!(ability[0]["args"]["reminder"], json!(reminder), "{text}");
        let meaning = crate::engine::semantic_abilities(ability).unwrap();
        let baseline = outcome(engine, &faces[1]);
        assert!(
            baseline["readings"].as_array().unwrap().iter().any(|row| {
                crate::engine::semantic_abilities(&row["card"]["abilities"]).unwrap() == meaning
            }),
            "{text}: {baseline}"
        );

        let context = &faces[0].context.as_ref().unwrap().0;
        let reading = CardReading {
            abilities: ability.clone(),
            notes: vec![],
        };
        let trail = engine
            .card_key_of(name, &reading, context, text, true)
            .unwrap()
            .expect("exact card trail");
        assert_eq!(
            engine
                .print_card_trail(name, &reading, context, &trail)
                .unwrap()
                .as_deref(),
            Some(text)
        );
    }
}

#[test]
fn unsupported_presentation_cannot_hide_rules_or_contradict_operands() {
    let engine = crate::test_support::engine();
    let cases = [
        "Uncatalogued heading — Draw a card.",
        "Partner (Draw a card.)",
        "Draw a card. (If a permanent with a stun counter would become untapped, remove one from it instead.)",
        "Create a Food token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")",
        "Scry 2. (Look at the top card of your library. You may put that card on the bottom.)",
        "Crew 2 (Tap any number of creatures you control with total power 3 or more: This Vehicle becomes an artifact creature until end of turn.)",
        "(As this Saga enters and after your draw step, add a lore counter. Sacrifice after II.)\nI — Draw a card.\nIII — Draw two cards.",
        "Choose one —\n• Draw a card.\n• Scry 1.\n• Surveil 1.\nEntwine {2} (Choose both if you pay the entwine cost.)",
    ];
    let records = cases.map(|text| raw("one", "Synthetic example", text));
    for face in scryfall(&records).unwrap() {
        let row = outcome(engine, &face);
        assert_eq!(row["status"], "refused", "{}: {row}", face.text);
    }
}

#[test]
fn layout_reminders_bind_to_the_native_face_and_source_record() {
    let engine = crate::test_support::engine();
    let adventure =
        "Draw a card. (Then exile this card. You may cast the creature later from exile.)";
    let room = "When you unlock this door, draw a card.\n(You may cast either half. That door unlocks on the battlefield. As a sorcery, you may pay the mana cost of a locked door to unlock it.)";
    let records = [
        json!({"oracle_id":"adventure","layout":"adventure","name":"Creature // Quest","card_faces":[
            {"name":"Creature","type_line":"Creature — Human","power":"1","toughness":"1"},
            {"name":"Quest","type_line":"Sorcery — Adventure","oracle_text":adventure}
        ]}),
        json!({"oracle_id":"room","layout":"split","name":"Library // Hall","card_faces":[
            {"name":"Library","type_line":"Enchantment — Room","oracle_text":room},
            {"name":"Hall","type_line":"Enchantment — Room"}
        ]}),
    ];
    let faces = scryfall(&records).unwrap();
    for (face, note) in [
        (
            &faces[1],
            LayoutReminder::Adventure(crate::engine::AdventureReturnType::Creature),
        ),
        (&faces[2], LayoutReminder::Room),
    ] {
        let row = outcome(engine, face);
        assert_eq!(row["status"], "unique", "{row}");
        let reading = CardReading {
            abilities: row["readings"][0]["card"]["abilities"].clone(),
            notes: vec![CardNote::LayoutReminder(note)],
        };
        assert_eq!(row["readings"][0]["notes"], json!(reading.notes));
        let context = &face.context.as_ref().unwrap().0;
        let trail = engine
            .card_key_of(&face.name, &reading, context, &face.text, true)
            .unwrap()
            .expect("exact layout reminder trail");
        assert_eq!(
            engine
                .print_card_trail(&face.name, &reading, context, &trail)
                .unwrap()
                .as_deref(),
            Some(face.text.as_str())
        );
    }

    let mut wrong = records[0].clone();
    wrong["card_faces"][0]["type_line"] = json!("Artifact");
    let wrong = scryfall(&[wrong]).unwrap();
    assert_eq!(outcome(engine, &wrong[1])["stage"], "source_context");
    let wrong = scryfall(&[raw("ordinary", "Quest", adventure)]).unwrap();
    assert_eq!(outcome(engine, &wrong[0])["stage"], "binding");
}

#[test]
fn edited_scry_amount_prints_in_the_action_and_its_reminder() {
    let engine = crate::test_support::engine();
    let name = "Synthetic example";
    let original = "Scry 2. (Look at the top two cards of your library, then put any number of them on the bottom and the rest on top in any order.)";
    let faces = scryfall(&[raw("one", name, original)]).unwrap();
    let context = &faces[0].context.as_ref().unwrap().0;
    let mut reading = engine
        .parse_card(name, original, context)
        .unwrap()
        .readings
        .pop()
        .unwrap();
    let amount = "/0/args/ability/args/body/instructions/0/args/operation/args/amount/args";
    *reading.abilities.pointer_mut(amount).unwrap() = json!(3);
    let printed = engine
        .print_card_keyed(name, &reading, context, &[])
        .unwrap()
        .unwrap();
    assert_eq!(
        printed,
        "Scry 3. (Look at the top three cards of your library, then put any number of them on the bottom and the rest on top in any order.)"
    );
    let parsed = engine.parse_card(name, &printed, context).unwrap();
    assert!(parsed.readings.contains(&reading));
}

#[test]
fn saga_reminder_prints_the_current_final_chapter() {
    let engine = crate::test_support::engine();
    let name = "Synthetic Saga";
    let text = "(As this Saga enters and after your draw step, add a lore counter. Sacrifice after II.)\nI — Draw a card.\nII — Draw two cards.";
    let mut record = raw("one", name, text);
    record["type_line"] = json!("Enchantment — Saga");
    let faces = scryfall(&[record]).unwrap();
    let face = &faces[0];
    let context = &face.context.as_ref().unwrap().0;
    let row = outcome(engine, face);
    assert_eq!(row["status"], "unique", "{row}");
    let mut reading = CardReading {
        abilities: row["readings"][0]["card"]["abilities"].clone(),
        notes: vec![],
    };
    let trail = engine
        .card_key_of(name, &reading, context, text, true)
        .unwrap()
        .expect("exact Saga trail");
    assert_eq!(
        engine
            .print_card_trail(name, &reading, context, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
    let stage = reading.abilities[0]["args"]["ability"]["args"]["chapters"]
        .as_array_mut()
        .unwrap();
    let mut third = stage[1].clone();
    third["args"]["event"]["SagaChapter"] = json!(3);
    stage.push(third);
    let edited =
        text.replace("Sacrifice after II.", "Sacrifice after III.") + "\nIII — Draw two cards.";
    let trail = engine
        .card_key_of(name, &reading, context, &edited, true)
        .unwrap()
        .expect("exact trail for the current chapter list");
    let printed = engine
        .print_card_trail(name, &reading, context, &trail)
        .unwrap()
        .expect("verified edited Saga print");
    assert_eq!(printed, edited);
    assert!(
        engine
            .parse_card(name, &printed, context)
            .unwrap()
            .readings
            .contains(&reading)
    );
}

#[test]
fn meld_reminder_requires_the_other_printed_component() {
    let text = "Draw a card.\n(Melds with Graf Rats.)";
    let mut record = raw("front", "Synthetic meld component", text);
    record["layout"] = json!("meld");
    record["all_parts"] = json!([
        {"component":"meld_part","name":"Synthetic meld component"},
        {"component":"meld_result","name":"Synthetic result"},
        {"component":"meld_part","name":"Graf Rats"}
    ]);
    let mut faces = scryfall(&[record]).unwrap();
    let face = &mut faces[0];
    let engine = crate::test_support::engine();
    let row = outcome(engine, face);
    assert_eq!(row["status"], "unique", "{row}");
    let reading = CardReading {
        abilities: row["readings"][0]["card"]["abilities"].clone(),
        notes: vec![CardNote::MeldWith("Graf Rats".into())],
    };
    let context = &face.context.as_ref().unwrap().0;
    let trail = engine
        .card_key_of(&face.name, &reading, context, text, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .print_card_trail(&face.name, &reading, context, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );

    face.source["all_parts"][2]["name"] = json!("Different partner");
    assert_eq!(outcome(engine, face)["stage"], "source_context");
}
