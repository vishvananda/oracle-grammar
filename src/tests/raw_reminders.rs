use crate::engine::{CardNote, CardReading, PrintedPowerToughness, empty_card_characteristics};
use crate::model::ir::{Characteristics, ManaColor, ManaSymbol};
use crate::model::mtg_types::{CardType, SubType, SuperType};

const NAME: &str = "Synthetic";

fn exact_roundtrip(text: &str, context: &Characteristics) -> CardReading {
    let engine = super::engine();
    let parsed = engine
        .parse_card(NAME, text, context)
        .unwrap_or_else(|error| panic!("{text}: {error:?}"));
    for reading in parsed.readings {
        let key = engine
            .card_key_of(NAME, &reading, context, text, true)
            .unwrap();
        if let Some(key) = key
            && engine
                .print_card_trail(NAME, &reading, context, &key)
                .unwrap()
                .as_deref()
                == Some(text)
        {
            return reading;
        }
    }
    panic!("no exact print for {text}");
}

#[test]
fn labelled_spell_replacement_reprints_edited_damage() {
    let mut context = empty_card_characteristics();
    context.card_types = vec![CardType::Sorcery];
    let text = "Synthetic deals 4 damage to any target.\nRaid — If you attacked this turn, instead Synthetic deals 5 damage to that permanent or player and the damage can't be prevented.";
    let plain = text.replace("Raid — ", "");
    let ordinary = exact_roundtrip(&plain, &context);
    let mut labelled = exact_roundtrip(text, &context);
    let presentation = &mut labelled.abilities[0]["args"];
    assert_eq!(presentation["label"], "Raid");
    assert_eq!(presentation["ability"], ordinary.abilities[0]);

    let damage = &mut presentation["ability"]["args"]["body"]["instructions"][0]["args"]["otherwise"]
        ["instructions"][0]["args"]["operation"]["args"]["amount"];
    assert_eq!(damage["args"], 4);
    damage["args"] = serde_json::json!(3);
    let engine = super::engine();
    let printed = engine
        .print_card_keyed(NAME, &labelled, &context, &[])
        .unwrap()
        .expect("edited replacement must print");
    assert!(printed.contains("deals 3 damage"), "{printed}");
    assert!(
        engine
            .parse_card(NAME, &printed, &context)
            .unwrap()
            .readings
            .contains(&labelled)
    );
}

#[test]
fn amplify_reminder_checks_the_count_and_current_source_types() {
    let mut context = empty_card_characteristics();
    context.card_types = vec![CardType::Creature];
    context.subtypes = vec![SubType::Bird, SubType::Soldier];
    let text = "Amplify 2 (As this creature enters, put two +1/+1 counters on it for each Bird and/or Soldier card you reveal in your hand.)";
    let reading = exact_roundtrip(text, &context);
    assert_eq!(reading.notes, [CardNote::Amplify]);
    let engine = super::engine();
    for altered in [
        text.replace("two +1/+1", "three +1/+1"),
        text.replace("Bird and/or Soldier", "Bird"),
    ] {
        assert!(engine.parse_card(NAME, &altered, &context).is_err());
    }
}

#[test]
fn compleated_reminder_binds_the_printed_phyrexian_symbol() {
    let mut context = empty_card_characteristics();
    context.card_types = vec![CardType::Planeswalker];
    context.mana_cost = Some(vec![
        ManaSymbol::Generic(2),
        ManaSymbol::Phyrexian(vec![ManaColor::Blue]),
    ]);
    let text = "Compleated ({U/P} can be paid with {U} or 2 life. If life was paid, this planeswalker enters with two fewer loyalty counters.)\n+1: Draw a card.";
    let reading = exact_roundtrip(text, &context);
    assert_eq!(reading.notes, [CardNote::Compleated { slot: 1 }]);
    let wrong = text.replace("with {U} or", "with {B} or");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn epic_reminder_uses_the_spells_own_announcement() {
    let mut context = empty_card_characteristics();
    context.card_types = vec![CardType::Sorcery];
    let rule = "For the rest of the game, you can't cast spells. At the beginning of each of your upkeeps, copy this spell except for its epic ability.";
    let retarget = " You may choose a new target for the copy.";
    let plain = "Draw a card.\nEpic";
    let targeted = "Destroy target creature.\nEpic";
    exact_roundtrip(&format!("{plain} ({rule})"), &context);
    exact_roundtrip(&format!("{targeted} ({rule}{retarget})"), &context);
    for wrong in [
        format!("{plain} ({rule}{retarget})"),
        format!("{targeted} ({rule})"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn phase_out_reminder_keeps_the_controller_of_its_target() {
    let context = empty_card_characteristics();
    let text = "{2}, {T}: Target creature you control phases out. (Treat it and anything attached to it as though they don't exist until your next turn.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace("you control", "an opponent controls");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn written_x_in_a_reminder_follows_the_action_amount() {
    let context = empty_card_characteristics();
    let note = "Target land you control becomes a 0/0 creature with haste that's still a land. Put X +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.";
    let text = format!("Earthbend X, where X is the number of Forests you control. ({note})");
    let reading = exact_roundtrip(&text, &context);
    let engine = super::engine();
    assert!(!reading.abilities.to_string().contains("$written_x_amount"));
    assert!(
        engine
            .parse_card(NAME, &format!("Earthbend 2. ({note})"), &context)
            .is_err()
    );
}

#[test]
fn opponent_damage_clause_shares_its_controllers() {
    let text = "When this creature becomes monstrous, it deals 2 damage to each opponent and each creature they control.";
    let context = empty_card_characteristics();
    let engine = super::engine();
    let parsed = engine.parse_card(NAME, text, &context).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let reading = exact_roundtrip(text, &context);
    let recipients = &reading.abilities[0]["args"]["body"]["instructions"][0]["args"]["operation"]
        ["args"]["recipients"]["args"];
    let opponents = &recipients[0]["args"];
    assert_eq!(opponents["kind"], "OpponentsOf");
    assert!(
        recipients[1]["args"]["args"]["filter"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|filter| filter["kind"] == "ControlledBy" && filter["args"] == *opponents)
    );
}

#[test]
fn backup_reminder_agrees_with_the_number_of_granted_abilities() {
    let text = "Backup 1 (When this creature enters, put a +1/+1 counter on target creature. If that's another creature, it gains the following abilities until end of turn.)\nFlying, vigilance";
    let context = empty_card_characteristics();
    exact_roundtrip(text, &context);
    let engine = super::engine();
    for wrong in [
        text.replace("following abilities", "following ability"),
        text.replace("Flying, vigilance", "Flying"),
    ] {
        assert!(engine.parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn granted_suspend_uses_the_exiled_cards_owner() {
    let action = "Exile target creature and put two time counters on it. If it doesn't have suspend, it gains suspend.";
    let owner = "At the beginning of its owner's upkeep, they remove a time counter. When the last is removed, they may play it without paying its mana cost. If it's a creature, it has haste.";
    let controller = "At the beginning of your upkeep, remove a time counter. When the last is removed, you may cast it without paying its mana cost. If it's a creature, it has haste.";
    let context = empty_card_characteristics();
    exact_roundtrip(&format!("{action} ({owner})"), &context);
    assert!(
        super::engine()
            .parse_card(NAME, &format!("{action} ({controller})"), &context)
            .is_err()
    );
}

#[test]
fn native_and_bracketed_loyalty_headings_preserve_the_same_activations() {
    let context = empty_card_characteristics();
    let raw = "+2: Draw a card.\n−1: You gain 2 life.\n0: Scry 1.";
    let prepared = "[+2]: Draw a card.\n[−1]: You gain 2 life.\n[0]: Scry 1.";
    let raw = exact_roundtrip(raw, &context);
    let prepared = exact_roundtrip(prepared, &context);
    assert_eq!(raw, prepared);
    assert!(
        super::engine()
            .parse_card(NAME, "[+2: Draw a card.", &context)
            .is_err()
    );
}

#[test]
fn named_battlefield_entry_preserves_a_required_target() {
    let context = empty_card_characteristics();
    let text = "When this creature enters the battlefield, exile target creature. Its controller seeks a creature card.";
    let long = exact_roundtrip(text, &context);
    let short = exact_roundtrip(&text.replace(" the battlefield", ""), &context);
    assert_eq!(long, short);
    assert_eq!(
        long.abilities[0]["args"]["announcement"]["targets"][0]["count"],
        serde_json::json!({"Exactly":{"kind":"Constant","args":1}})
    );
}

#[test]
fn loyalty_discount_prints_the_draws_target_as_the_caster() {
    let context = empty_card_characteristics();
    let text = "−2: Target player draws two cards. Until your next turn, instant, sorcery, and planeswalker spells that player casts cost {2} less to cast.";
    exact_roundtrip(text, &context);
}

#[test]
fn mana_explanation_checks_the_activation_cost() {
    let context = empty_card_characteristics();
    let text = "{5}{U/P}: Transform this creature. Activate only as a sorcery. ({U/P} can be paid with either {U} or 2 life.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace(
        "({U/P} can be paid with either {U}",
        "({B/P} can be paid with either {B}",
    );
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn mana_explanation_checks_the_symbol_paid_during_resolution() {
    let context = empty_card_characteristics();
    let text = "Whenever you sacrifice a creature, you may pay {U/B}. If you do, draw a card. ({U/B} can be paid with either {U} or {B}.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace(
        "({U/B} can be paid with either {U}",
        "({R/B} can be paid with either {R}",
    );
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn kicker_reminder_agrees_with_the_branch_cost() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Sorcery);
    let text = "Kicker {2}{W} (You may pay an additional {2}{W} as you cast this spell.)\nChoose target creature with mana value 3 or less. If this spell was kicked, instead choose target creature. Exile the chosen creature, then its controller gains life equal to its mana value.";
    exact_roundtrip(text, &context);
    let wrong = text.replace("additional {2}{W}", "additional {3}{W}");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn local_reminders_keep_their_position_inside_casting_branches() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Sorcery);
    let modification = "As an additional cost to cast this spell, sacrifice a creature or sacrifice a modified creature. (Equipment, Auras you control, and counters are modifications.)\nDestroy target creature or planeswalker. If the modified creature was sacrificed, draw a card.";
    let food = concat!(
        "Gift a Food (You may promise an opponent a gift as you cast this spell. If you do, ",
        "they create a Food token before its other effects. It's an artifact with \"{2}, {T}, Sacrifice this token: You gain 3 life.\")\n",
        "Destroy target creature. If the gift wasn't promised, you lose 2 life."
    );
    exact_roundtrip(modification, &context);
    exact_roundtrip(food, &context);
    let wrong = food.replacen("Gift a Food", "Gift a Treasure", 1);
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn an_indestructible_reminder_stays_with_its_mode() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Instant);
    let destroy = "• Destroy target tapped creature.";
    let protect = "• Target creature gains indestructible until end of turn. (Damage and effects that say \"destroy\" don't destroy it.)";
    for (first, last) in [(protect, destroy), (destroy, protect)] {
        let text = format!("Choose one —\n{first}\n{last}");
        assert_eq!(
            super::engine()
                .parse_card(NAME, &text, &context)
                .unwrap()
                .readings
                .len(),
            1
        );
        exact_roundtrip(&text, &context);
    }
}

#[test]
fn joined_energy_reminders_agree_with_the_symbol_count() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Creature);
    let text = "When this creature enters, draw a card and you get {E}{E} (two energy counters).";
    exact_roundtrip(text, &context);
    let wrong = text.replace("two energy counters", "three energy counters");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn token_description_checks_the_created_type_and_quoted_ability() {
    let context = empty_card_characteristics();
    let text = "When this creature enters, create two Shard tokens. (They're enchantments with \"{2}, Sacrifice this token: Scry 1, then draw a card.\")";
    exact_roundtrip(text, &context);
    let engine = super::engine();
    for wrong in [
        text.replace("Scry 1", "Scry 2"),
        text.replace("enchantments", "artifacts"),
        text.replace("They're", "It's")
            .replace("enchantments", "an enchantment"),
    ] {
        assert!(
            engine.parse_card(NAME, &wrong, &context).is_err(),
            "{wrong}"
        );
    }
}

#[test]
fn energy_choice_prints_the_same_exiled_card_without_a_key() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Creature);
    let text = "Haste\nWhenever this creature attacks, exile the top card of your library. You may get {E}{E} (two energy counters). If you don't, you may play that card this turn.\n{T}, Pay {E}{E}{E}: This creature deals 3 damage to any target.";
    let reading = exact_roundtrip(text, &context);
    let engine = super::engine();
    let printed = engine
        .print_card_keyed(NAME, &reading, &context, &[])
        .unwrap()
        .expect("the acceptance and refusal paths in the energy choice must print");
    assert!(
        engine
            .parse_card(NAME, &printed, &context)
            .unwrap()
            .readings
            .contains(&reading)
    );
}

#[test]
fn a_front_face_return_preserves_the_exile_arrival() {
    let context = empty_card_characteristics();
    let original = "Exile Synthetic, then return it to the battlefield (front face up).";
    let plain = "Exile Synthetic, then return it to the battlefield.";
    let reading = exact_roundtrip(original, &context);
    let bare = super::engine().parse_card(NAME, plain, &context).unwrap();
    assert!(bare.readings.contains(&reading));
    let instructions = &reading.abilities[0]["args"]["body"]["instructions"];
    let objects = &instructions[1]["args"]["operation"]["args"]["objects"];
    assert_eq!(objects["args"]["zone"], "Exile");
    assert_eq!(objects["args"]["objects"]["args"]["phase"], "After");
    assert_eq!(
        objects["args"]["objects"]["args"]["result"],
        instructions[0]["args"]["result"]
    );
}

#[test]
fn reminder_checks_both_colors_spent_to_cast_the_spell() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Sorcery);
    let text = "This spell deals 3 damage to each creature without flying if {R} was spent to cast this spell and 3 damage to each creature with flying if {G} was spent to cast this spell. (Do both if {R}{G} was spent.)";
    exact_roundtrip(text, &context);
    exact_roundtrip(&text.replace("{R}{G}", "{G}{R}"), &context);
    for cost in ["{R}{W}", "{R}{R}", "{R}{G}{G}"] {
        let wrong = text.replace("{R}{G}", cost);
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn mana_reminder_preserves_two_separate_spell_conditions() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Instant);
    let bare = "If {B} was spent to cast this spell, you may have target creature get -3/-3 until end of turn. If {G} was spent to cast this spell, you may have target creature get +3/+3 until end of turn.";
    let text = format!("{bare} (Do both if {{B}}{{G}} was spent.)");
    let reading = exact_roundtrip(&text, &context);
    let abilities = exact_roundtrip(bare, &context).abilities;
    assert_eq!(reading.abilities[0]["args"]["ability"], abilities[0]);
    let wrong = text.replace("{B}{G}", "{U}{G}");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn ripple_reminder_uses_the_current_reveal_count() {
    let context = empty_card_characteristics();
    let text = "Ripple 4 (When you cast this spell, you may reveal the top four cards of your library. You may cast spells with the same name as this spell from among those cards without paying their mana costs. Put the rest on the bottom of your library.)";
    exact_roundtrip(text, &context);
    exact_roundtrip(&text.replace('4', "3").replace("four", "three"), &context);
    let wrong = text.replace("four", "five");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn bands_reminder_checks_the_quoted_legendary_creature_filter() {
    let context = empty_card_characteristics();
    let text = concat!(
        "White legendary creatures you control have \"bands with other legendary creatures.\" ",
        "(Any legendary creatures can attack in a band as long as at least one has \"bands with other legendary creatures.\" ",
        "Bands are blocked as a group. If at least two legendary creatures you control, one of which has \"bands with other legendary creatures,\" ",
        "are blocking or being blocked by the same creature, you divide that creature's combat damage, ",
        "not its controller, among any of the creatures it's being blocked by or is blocking.)"
    );
    exact_roundtrip(text, &context);
    let wrong = text.replacen(
        "bands with other legendary creatures",
        "bands with other creatures",
        1,
    );
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn mayhem_reminder_repeats_the_printed_cost() {
    let context = empty_card_characteristics();
    let text = "Mayhem {1}{R} (You may cast this card from your graveyard for {1}{R} if you discarded it this turn. Timing rules still apply.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace("for {1}{R}", "for {2}{R}");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn partner_reminder_names_the_same_card_as_the_ability() {
    let context = empty_card_characteristics();
    let text = "Partner with Rory Williams (When this creature enters, target player may put Rory into their hand from their library, then shuffle.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace("put Rory", "put Evie");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn role_reminder_checks_the_attached_recipe_and_prints_its_attack_trigger() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Creature);
    let text = concat!(
        "{1}, Sacrifice this creature: Create a Young Hero Role token attached to another target creature. ",
        "(If you control another Role on it, put that one into the graveyard. ",
        "Enchanted creature has \"Whenever this creature attacks, if its toughness is 3 or less, ",
        "put a +1/+1 counter on it.\")"
    );
    exact_roundtrip(text, &context);
    let wrong = text.replace("Young Hero Role token", "Royal Role token");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn exiled_creature_reminder_follows_the_same_move_receipt() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Enchantment);
    let text = "When this enchantment enters, exile target creature until this enchantment leaves the battlefield. (That creature returns under its owner's control.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace("exile target creature", "exile target artifact");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn cast_copy_reminder_requires_a_spell_cast_from_the_copy_result() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Creature);
    let text = "When this creature enters, exile target nonland card with mana value 3 or less from your graveyard. Copy it, then you may cast the copy without paying its mana cost. (A copy of a permanent spell becomes a token.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace(
        ", then you may cast the copy without paying its mana cost",
        "",
    );
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn definite_spell_copy_reminder_requires_a_permanent_source() {
    let engine = super::engine();
    let mut context = empty_card_characteristics();
    let text = "When you cast this spell, copy it. (The copy becomes a token.)";
    assert!(engine.parse_card(NAME, text, &context).is_err());
    context.card_types = vec![CardType::Sorcery];
    assert!(engine.parse_card(NAME, text, &context).is_err());
    context.card_types = vec![CardType::Artifact];
    exact_roundtrip(text, &context);
}

#[test]
fn monocolored_hybrid_reminder_checks_the_glyph_and_total() {
    let engine = super::engine();
    let mut context = empty_card_characteristics();
    context.mana_cost = Some(vec![
        ManaSymbol::Generic(3),
        ManaSymbol::MonocoloredHybrid(ManaColor::Blue),
    ]);
    let text = "({2/U} can be paid with any two mana or with {U}. This card's mana value is 5.)\nDraw a card.";
    let reading = exact_roundtrip(text, &context);
    assert_eq!(reading.notes, [CardNote::MonocoloredHybrid { slot: 1 }]);
    for changed in [
        text.replace("value is 5", "value is 4"),
        text.replace("with {U}", "with {B}"),
    ] {
        assert!(engine.parse_card(NAME, &changed, &context).is_err());
    }
    context.mana_cost = Some(vec![
        ManaSymbol::Generic(4),
        ManaSymbol::MonocoloredHybrid(ManaColor::Red),
    ]);
    let edited = text.replace('U', "R").replace("value is 5", "value is 6");
    let key = engine
        .card_key_of(NAME, &reading, &context, &edited, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .print_card_trail(NAME, &reading, &context, &key)
            .unwrap()
            .as_deref(),
        Some(edited.as_str())
    );
}

#[test]
fn transform_reminder_reads_the_current_front_name_from_source() {
    let engine = super::engine();
    let name = "Vault of Catlacan";
    let mut characteristics = empty_card_characteristics();
    characteristics.supertypes.push(SuperType::Legendary);
    let text = "(Transforms from Storm the Vault.)\n{T}: Add {C}.";
    let mut source = engine
        .card_input_context(name, text, &characteristics, PrintedPowerToughness::Absent)
        .unwrap();
    source.transform_front = Some("Storm the Vault".into());
    let parsed = engine.parse_card_with_source(name, text, &source).unwrap();
    let [reading] = parsed.readings.as_slice() else {
        panic!("expected one reading");
    };
    assert_eq!(reading.notes, [CardNote::TransformsFrom]);
    let key = engine
        .card_key_of_with_source(name, reading, &source, text, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .print_card_trail_with_source(name, reading, &source, &key)
            .unwrap()
            .as_deref(),
        Some(text)
    );
    assert!(engine.parse_card(name, text, &characteristics).is_err());
    source.transform_front = Some("Dowsing Device".into());
    assert!(engine.parse_card_with_source(name, text, &source).is_err());
    let edited = engine
        .print_card_keyed_with_source(name, reading, &source, &[])
        .unwrap()
        .unwrap();
    assert!(edited.starts_with("(Transforms from Dowsing Device.)\n"));
    let reparsed = engine
        .parse_card_with_source(name, &edited, &source)
        .unwrap();
    assert_eq!(reparsed.readings.as_slice(), std::slice::from_ref(reading));
}

#[test]
fn oracle_punctuation_spacing_preserves_the_token_stream() {
    use crate::tokens::{Names, tokenize};
    let names = Names::of(NAME);
    let vocabulary = ["investigate", "create", "throw"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    for (text, printed) in [
        ("Investigate.(Create.)", "Investigate. (Create.)"),
        ("Throw ... —", "Throw... —"),
        ("\"Investigate.\"(Create.)", "\"Investigate.\" (Create.)"),
    ] {
        assert_eq!(
            tokenize(text, &names, &vocabulary).unwrap(),
            tokenize(printed, &names, &vocabulary).unwrap()
        );
    }
    for wrong in [
        "Investigate . (Create.)",
        "Throw  ... —",
        "Investigate(Clue)",
    ] {
        assert!(tokenize(wrong, &names, &vocabulary).is_err());
    }
}

#[test]
fn teamwork_reminder_preserves_the_optional_casting_branch() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Sorcery);
    let bare = "Teamwork 2\nDraw three cards. Then discard a card unless this spell was cast using teamwork.";
    let note = "2 (As an additional cost to cast this spell, you may tap any number of creatures you control with total power 2 or more.)";
    let reminded = bare.replacen('2', note, 1);
    assert_eq!(
        exact_roundtrip(bare, &context),
        exact_roundtrip(&reminded, &context)
    );
    let wrong = reminded.replace("power 2", "power 3");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn player_hexproof_reminder_refers_to_the_ability_controller() {
    let context = empty_card_characteristics();
    let text = "You have hexproof. (You can't be the target of spells or abilities your opponents control.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace("You have hexproof", "Creatures you control have hexproof");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn assist_limit_prints_from_the_current_mana_cost_slot() {
    let mut context = empty_card_characteristics();
    context.mana_cost = Some(vec![
        ManaSymbol::Generic(5),
        ManaSymbol::Color(ManaColor::Blue),
    ]);
    let text = "Assist (Another player can pay up to {5} of this spell's cost.)\nDraw a card.";
    let reading = exact_roundtrip(text, &context);
    assert_eq!(reading.notes, vec![CardNote::AssistLimit { slot: 0 }]);
    context.mana_cost.as_mut().unwrap()[0] = ManaSymbol::Generic(3);
    assert!(super::engine().parse_card(NAME, text, &context).is_err());
    assert_eq!(
        super::engine()
            .print_card_keyed(NAME, &reading, &context, &[])
            .unwrap()
            .unwrap(),
        text.replace("{5}", "{3}")
    );
}

#[test]
fn legendary_sorcery_explanation_requires_both_printed_types() {
    let mut context = empty_card_characteristics();
    context.card_types = vec![CardType::Sorcery];
    context.supertypes = vec![SuperType::Legendary];
    let text = "(You may cast a legendary sorcery only if you control a legendary creature or planeswalker.)\nDraw a card.";
    exact_roundtrip(text, &context);
    context.supertypes.clear();
    assert!(super::engine().parse_card(NAME, text, &context).is_err());
}

#[test]
fn awaken_explanation_checks_both_payment_and_counter_amount() {
    let context = empty_card_characteristics();
    let text = "Draw a card.\nAwaken 4—{4}{W} (If you cast this spell for {4}{W}, also put four +1/+1 counters on target land you control and it becomes a 0/0 Elemental creature with haste. It's still a land.)";
    exact_roundtrip(text, &context);
    for wrong in [
        text.replace("put four", "put three"),
        text.replace("for {4}{W}", "for {5}{W}"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn ward_explanation_repeats_the_granted_payment() {
    let context = empty_card_characteristics();
    let text = "Enchanted creature has ward {2}. (Whenever it becomes the target of a spell or ability an opponent controls, counter it unless that player pays {2}.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace("pays {2}", "pays {3}");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    let equipped = "Equipped creature gets +0/+1 and has ward {2}. (Whenever equipped creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {2}.)\nEquip {1}";
    exact_roundtrip(equipped, &context);
}

#[test]
fn prowl_explanation_checks_the_source_creature_types() {
    let mut context = empty_card_characteristics();
    context.card_types = vec![CardType::Creature];
    context.subtypes = vec![SubType::Faerie, SubType::Rogue];
    let text = "Flying\nProwl {2}{U} (You may cast this for its prowl cost if you dealt combat damage to a player this turn with a Faerie or Rogue.)\nWhen this creature enters, draw a card.";
    let reading = exact_roundtrip(text, &context);
    assert_eq!(reading.notes, [CardNote::Prowl]);
    for altered in [
        text.replace("Faerie or Rogue", "Rogue"),
        text.replace("Faerie", "Goblin"),
    ] {
        assert!(
            super::engine()
                .parse_card(NAME, &altered, &context)
                .is_err()
        );
    }
}

#[test]
fn optional_mill_explanation_keeps_its_amount_and_optionality() {
    let context = empty_card_characteristics();
    let text = "When this creature enters, you may mill two cards. (You may put the top two cards of your library into your graveyard.)";
    exact_roundtrip(text, &context);
    for wrong in [
        text.replace("may mill two", "may mill three"),
        text.replace("you may mill", "you mill"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn a_reminder_preserves_the_combined_entry_or_death_trigger() {
    let context = empty_card_characteristics();
    let text = "When this creature enters or dies, surveil 1. (Look at the top card of your library. You may put it into your graveyard.)";
    let parsed = super::engine().parse_card(NAME, text, &context).unwrap();
    let [reading] = parsed.readings.as_slice() else {
        panic!("expected one combined trigger");
    };
    assert_eq!(reading.abilities.as_array().unwrap().len(), 1);
    exact_roundtrip(text, &context);
}

#[test]
fn devour_explanation_checks_the_filter_and_multiplier() {
    let context = empty_card_characteristics();
    let text = "Devour artifact 2 (As this creature enters, you may sacrifice any number of artifacts. It enters with twice that many +1/+1 counters on it.)";
    exact_roundtrip(text, &context);
    for wrong in [
        text.replace("of artifacts.", "of creatures."),
        text.replace("twice", "three times"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn reinforce_explanation_checks_the_payment_and_amount() {
    let context = empty_card_characteristics();
    let text = "Reinforce 2—{2}{W} ({2}{W}, Discard this card: Put two +1/+1 counters on target creature.)";
    exact_roundtrip(text, &context);
    for wrong in [
        text.replace("({2}{W}", "({2}{U}"),
        text.replace("Put two", "Put three"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn a_fixed_airbend_explanation_keeps_the_action_and_payment() {
    let context = empty_card_characteristics();
    let text = "Airbend target nonland permanent. (Exile it. While it's exiled, its owner may cast it for {2} rather than its mana cost.)";
    exact_roundtrip(text, &context);
    for wrong in [
        text.replace("for {2}", "for {3}"),
        text.replace("Airbend", "Exile"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn support_keeps_the_permanent_and_spell_target_domains_distinct() {
    let context = empty_card_characteristics();
    let spell = "Support 2. (Put a +1/+1 counter on each of up to two target creatures.)";
    let permanent = "When this creature enters, support 2. (Put a +1/+1 counter on each of up to two other target creatures.)";
    let spell_reading = exact_roundtrip(spell, &context);
    let permanent_reading = exact_roundtrip(permanent, &context);
    let filter = |reading: &CardReading| {
        reading.abilities[0]["args"]["ability"]["args"]["announcement"]["targets"][0]["filter"]
            .clone()
    };
    assert_eq!(filter(&spell_reading)["kind"], "CardType");
    assert_eq!(
        filter(&permanent_reading)["args"][1]["args"]["args"]["kind"],
        "Source"
    );
    for wrong in [
        spell.replace("two target", "two other target"),
        permanent.replace("two other", "three other"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn a_spell_copy_note_uses_the_bound_cast_trigger_subject() {
    let context = empty_card_characteristics();
    let text = "Whenever you cast a creature spell, copy it. (The copy becomes a token.)";
    let reading = exact_roundtrip(text, &context);
    assert!(
        reading.abilities[0]["args"]
            .get("$bound_reminders")
            .is_none()
    );
    for wrong in [
        text.replace("creature spell", "instant spell"),
        text.replace("copy it", "draw a card"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn preparation_note_names_the_current_holder_after_binding() {
    let context = empty_card_characteristics();
    let text = "At the beginning of your end step, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace("this creature", "target creature");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn battlefield_return_note_requires_a_battlefield_incarnation() {
    let context = empty_card_characteristics();
    let text = "At the beginning of the end step, return this creature to its owner's hand. (Return it only if it's on the battlefield.)";
    exact_roundtrip(text, &context);
    let wrong = "Return target creature card from your graveyard to your hand. (Return it only if it's on the battlefield.)";
    assert!(super::engine().parse_card(NAME, wrong, &context).is_err());
}

#[test]
fn copy_explanation_distinguishes_permanent_spells_from_abilities() {
    let context = empty_card_characteristics();
    let text = "{4}, {T}: Copy target permanent spell you control. (The copy becomes a token.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace("permanent spell", "activated ability");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn two_reminder_claims_must_agree_with_the_same_resolution() {
    let context = empty_card_characteristics();
    let text = "Whenever Synthetic attacks, attach up to one target Equipment to it. Then amass Orcs X, where X is the number of Equipment attached to Synthetic. (Control of the Equipment doesn't change. To amass Orcs X, put X +1/+1 counters on an Army you control. It's also an Orc. If you don't control an Army, create a 0/0 black Orc Army creature token first.)";
    exact_roundtrip(text, &context);
    let wrong_amount = text.replace("To amass Orcs X, put X", "To amass Orcs 2, put 2");
    let wrong_mechanic = text.replace(
        "Control of the Equipment doesn't change.",
        "Damage causes loss of life.",
    );
    for wrong in [wrong_amount, wrong_mechanic] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn storm_token_copies_require_a_permanent_source() {
    let mut context = empty_card_characteristics();
    let text = "Storm (When you cast this spell, copy it for each spell cast before it this turn. Copies become tokens.)";
    context.card_types = vec![CardType::Creature];
    exact_roundtrip(text, &context);
    context.card_types = vec![CardType::Sorcery];
    assert!(super::engine().parse_card(NAME, text, &context).is_err());
}

#[test]
fn delayed_phase_out_looks_up_targets_in_the_owning_announcement() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Planeswalker);
    let text = "+1: Up to two other target planeswalkers you control phase out at the beginning of the next end step. (Treat them and anything attached to them as though they don't exist until your next turn.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace("you control", "an opponent controls");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn first_cascade_reminder_preserves_the_origin_of_the_first_spell() {
    let context = empty_card_characteristics();
    let text = "The first spell you cast from exile each turn has cascade. (When you cast your first spell from exile, exile cards from the top of your library until you exile a nonland card that costs less. You may cast it without paying its mana cost. Put the exiled cards on the bottom in a random order.)";
    exact_roundtrip(text, &context);
    let wrong = text.replacen("from exile each turn", "each turn", 1);
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn graveyard_copy_reminders_agree_on_cost_and_source_creature_types() {
    let engine = super::engine();
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Creature);
    context.subtypes = vec![SubType::Human, SubType::Cleric];
    let embalm = "Vigilance\nEmbalm {3}{W} ({3}{W}, Exile this card from your graveyard: Create a token that's a copy of it, except it's a white Zombie Human Cleric with no mana cost. Embalm only as a sorcery.)";
    let eternalize = "Eternalize {4}{B} ({4}{B}, Exile this card from your graveyard: Create a token that's a copy of it, except it's a 4/4 black Zombie Human Cleric with no mana cost. Eternalize only as a sorcery.)";
    let mut reading = exact_roundtrip(embalm, &context);
    assert_eq!(reading.notes, [CardNote::CopyTokenTypes]);
    exact_roundtrip(eternalize, &context);
    for wrong in [
        embalm.replacen("({3}{W}", "({2}{W}", 1),
        embalm.replace("Human Cleric", "Human Wizard"),
        embalm.replace("Zombie Human Cleric", "Human Cleric"),
        eternalize.replace("4/4 black", "3/4 black"),
    ] {
        assert!(
            engine.parse_card(NAME, &wrong, &context).is_err(),
            "{wrong}"
        );
    }

    reading.abilities[1]["args"]["cost"]["parts"][0]["Mana"][0]["Generic"] = serde_json::json!(2);
    context.subtypes = vec![SubType::Cat];
    let edited = embalm.replace("{3}", "{2}").replace("Human Cleric", "Cat");
    let key = engine
        .card_key_of(NAME, &reading, &context, &edited, true)
        .unwrap()
        .expect("edited cost and creature types must print");
    assert_eq!(
        engine
            .print_card_trail(NAME, &reading, &context, &key)
            .unwrap()
            .as_deref(),
        Some(edited.as_str())
    );
}

#[test]
fn an_unearth_reminder_repeats_the_cost_of_the_granted_ability() {
    let context = empty_card_characteristics();
    let text = "Each creature card in your graveyard has unearth {2}{B}. ({2}{B}: Return the card to the battlefield. The creature gains haste. Exile it at the beginning of the next end step or if it would leave the battlefield. Unearth only as a sorcery.)";
    exact_roundtrip(text, &context);
    let wrong = text.replacen("({2}{B}", "({1}{B}", 1);
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn shared_typecycling_reminder_checks_both_activations() {
    let context = empty_card_characteristics();
    let text = "Forestcycling {2}, plainscycling {2} ({2}, Discard this card: Search your library for a Forest or Plains card, reveal it, put it into your hand, then shuffle.)";
    exact_roundtrip(text, &context);
    for wrong in [
        text.replace("plainscycling {2}", "plainscycling {3}"),
        text.replace("Forest or Plains", "Forest or Swamp"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
    let edited = text.replace("{2}", "{3}").replace("Plains", "Mountain");
    exact_roundtrip(
        &edited.replace("plainscycling", "mountaincycling"),
        &context,
    );
}

#[test]
fn token_prowess_reminder_refers_to_the_token_created_by_the_line() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Sorcery);
    let text = "Create a 1/1 red Monk creature token with prowess. (Whenever you cast a noncreature spell, the token gets +1/+1 until end of turn.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace("with prowess", "with trample");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn a_hidden_card_reminder_requires_face_down_exile() {
    let mut context = empty_card_characteristics();
    context.card_types.push(CardType::Creature);
    let text = "Whenever this creature attacks, exile the top card of your library face down. (You can't look at it.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace(" face down", "");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}
