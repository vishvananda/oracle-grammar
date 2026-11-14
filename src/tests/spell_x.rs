use super::*;

fn only(text: &str) -> Value {
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{text}: {readings:#?}");
    readings.into_iter().next().unwrap()
}

fn instructions(abilities: &Value) -> &[Value] {
    abilities[0]["args"]["body"]["instructions"]
        .as_array()
        .unwrap()
}

fn spell_x() -> Value {
    json!({"kind":"AnnouncedX"})
}

#[test]
fn seven_spell_x_followups_keep_their_native_meanings_and_prints() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../../tests/fixtures/spell-x.json")).unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let wanted = canonical_abilities(&case["expected"]).unwrap();
        assert_eq!(
            engine().parse(name, text).unwrap().readings,
            vec![wanted.clone()],
            "{name}"
        );
        let without = engine().rule_set(crate::grammar::seeded);
        assert_eq!(
            engine()
                .parse_without(name, text, &without)
                .unwrap()
                .readings,
            vec![wanted.clone()],
            "{name}"
        );
        let printed = engine()
            .print_keyed(name, &wanted, &["Sorcery".into()], &[])
            .unwrap()
            .expect(name);
        assert!(
            engine()
                .parse(name, &printed)
                .unwrap()
                .readings
                .contains(&wanted),
            "{name}: {printed}"
        );
        let trail = engine().trail_of(name, &wanted, text).unwrap().expect(name);
        assert_eq!(
            engine()
                .print_trail(name, &wanted, &trail)
                .unwrap()
                .as_deref(),
            Some(text),
            "{name}"
        );
    }
}

#[test]
fn normalized_casting_x_references_keep_their_original_prints() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../../tests/fixtures/spell-x-normalized.json")).unwrap();
    assert_eq!(cases.len(), 21);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let operands = &case["expected"];
        let wanted = canonical_abilities(operands).unwrap();
        assert!(
            engine()
                .parse(name, text)
                .unwrap()
                .readings
                .contains(&wanted),
            "{name}"
        );
        // Shatterskull's divided-damage inverse was already an open gap.
        if case.get("exact") == Some(&json!(false)) {
            continue;
        }
        let key = engine()
            .derivation_of(name, operands, text)
            .unwrap()
            .expect(name);
        if engine()
            .print_keyed(name, operands, &[], &key)
            .unwrap()
            .as_deref()
            != Some(text)
        {
            let trail = engine()
                .trail_of(name, operands, text)
                .unwrap()
                .expect(name);
            assert_eq!(
                engine()
                    .print_trail(name, operands, &trail)
                    .unwrap()
                    .as_deref(),
                Some(text),
                "{name}"
            );
        }
    }
}

#[test]
fn casting_condition_spelling_keeps_current_operands_and_independent_frames() {
    let reading = only("Draw X cards. If X is 5 or more, you gain 3 life.");
    let mut spell = reading[0].clone();
    spell["args"]["body"]["instructions"][1]["args"]["condition"]["args"]["test"]["args"]["value"]
        ["args"] = json!(8);
    let mut projected = crate::bind::spell_x::projection(&spell)
        .unwrap()
        .expect("casting condition has a legacy spelling");
    assert_eq!(
        projected["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"],
        spell_x()
    );
    crate::bind::spell_x::lower(&mut projected).unwrap();
    assert_eq!(projected, spell);
    let printed = printed_types(&json!([spell]), &["Sorcery"]);
    assert!(printed.contains("8 or more"), "{printed}");

    let activation = only("{X}: Draw X cards. If X is 5 or more, you gain 3 life.")[0].clone();
    assert!(
        crate::bind::spell_x::projection(&activation)
            .unwrap()
            .is_none()
    );
    let mut grant = only("Target creature gains flying until end of turn.")[0].clone();
    grant["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"][0] =
        json!({"kind":"GrantAbility","args":activation});
    assert!(crate::bind::spell_x::projection(&grant).unwrap().is_none());
}

#[test]
fn source_spell_x_conditions_end_at_their_sentence() {
    let text = "Draw a card. If X is 5 or more, you gain 3 life. Draw two cards.";
    let reading = only(text);
    let body = instructions(&reading);
    assert_eq!(body.len(), 3);
    assert_eq!(body[0]["args"]["operation"]["kind"], "Draw");
    assert_eq!(body[1]["kind"], "If");
    assert_eq!(body[1]["args"]["condition"]["args"]["left"], spell_x());
    assert_eq!(
        body[1]["args"]["then"]["instructions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        body[1]["args"]["then"]["instructions"][0]["args"]["operation"]["kind"],
        "GainLife"
    );
    assert_eq!(body[2]["args"]["operation"]["kind"], "Draw");
    let trail = engine()
        .trail_of("Context example", &reading, text)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine()
            .print_trail("Context example", &reading, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
}

#[test]
fn repeated_thresholds_keep_independent_prefixes_and_suffixes() {
    let text = "Draw a card. You gain 1 life. If X is 5 or more, you gain 3 life. You lose 2 life. If X is 8 or more, draw two cards. Draw three cards.";
    let reading = only(text);
    let body = instructions(&reading);
    assert_eq!(body.len(), 6);
    for index in [2, 4] {
        assert_eq!(body[index]["kind"], "If");
        assert_eq!(body[index]["args"]["condition"]["args"]["left"], spell_x());
        assert_eq!(
            body[index]["args"]["then"]["instructions"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    assert_eq!(body[3]["args"]["operation"]["kind"], "LoseLife");
    assert_eq!(body[5]["args"]["operation"]["kind"], "Draw");
    printed_types(&reading, &["Sorcery"]);
}

#[test]
fn changed_spell_x_thresholds_print_their_current_comparison_and_amount() {
    let mut reading = only("Draw a card. If X is 5 or more, you gain 3 life.");
    let test = &mut reading[0]["args"]["body"]["instructions"][1]["args"]["condition"]["args"]["test"]
        ["args"];
    test["comparison"] = json!("LessOrEqual");
    test["value"]["args"] = json!(8);
    let text = printed_types(&reading, &["Sorcery"]);
    assert!(
        text.contains("8 or less") || text.contains("8 or fewer"),
        "{text}"
    );
    let bare = only("If X is 3 or less, scry 2.");
    assert_eq!(
        instructions(&bare)[0]["args"]["condition"]["args"]["left"],
        spell_x()
    );
    printed_types(&bare, &["Sorcery"]);
}

#[test]
fn fixed_numbers_and_activation_x_do_not_become_source_spell_x() {
    let fixed = only("Draw a card. If 7 is 5 or more, you gain 3 life.");
    assert_eq!(
        instructions(&fixed)[1]["args"]["condition"]["args"]["left"],
        json!({"kind":"Constant","args":7})
    );
    let activation = only("{X}: Draw a card. If X is greater than 5, you gain 3 life.");
    assert_eq!(
        instructions(&activation)[1]["args"]["condition"]["args"]["left"],
        json!({"kind":"AnnouncedX"})
    );
    printed(&activation);
}

#[test]
fn numeric_conditions_use_the_complete_where_definition() {
    for text in [
        "Draw X cards, where X is the number of creature cards in your graveyard. If X is 3 or more, you gain 3 life.",
        "Draw X cards, where X is the number of creature cards in your graveyard. If X is 3 or less, you gain 3 life.",
        "Whenever you cast a noncreature spell, this creature gets +X/+0 until end of turn, where X is the amount of mana spent to cast that spell. If X is 4 or more, draw a card.",
    ] {
        let reading = only(text);
        let mut predicates = Vec::new();
        visit(&reading, "Compare", &mut predicates);
        assert!(
            !predicates
                .iter()
                .any(|predicate| predicate["left"] == json!({"kind":"AnnouncedX"})),
            "{reading:#?}"
        );
        printed_types(&reading, &["Sorcery"]);
    }
}

#[test]
fn source_x_normalization_preserves_other_objects_and_independent_frames() {
    let source_x = json!({"kind":"ObjectProperty","args":{"object":{"kind":"Source"},"property":"SpellX","information":"CurrentOrLastKnown"}});
    let mut spell = only("Draw X cards.")[0].clone();
    spell["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"] =
        source_x.clone();
    let mut normalized = spell.clone();
    crate::bind::spell_x::lower(&mut normalized).unwrap();
    assert_eq!(
        normalized["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"],
        json!({"kind":"AnnouncedX"})
    );

    for (object, information) in [
        (json!({"kind":"EventSubject"}), json!("CurrentOrLastKnown")),
        (json!({"kind":"Source"}), json!({"Captured":0})),
    ] {
        let mut preserved = spell.clone();
        let expression = &mut preserved["args"]["body"]["instructions"][0]["args"]["operation"]["args"]
            ["amount"]["args"];
        expression["object"] = object;
        expression["information"] = information;
        let before = preserved.clone();
        crate::bind::spell_x::lower(&mut preserved).unwrap();
        assert_eq!(preserved, before);
    }

    let mut activation = only("{X}: Draw X cards.")[0].clone();
    activation["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"] = source_x;
    let mut grant = only("Target creature gains flying until end of turn.")[0].clone();
    grant["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"][0] =
        json!({"kind":"GrantAbility","args":activation});
    let before = grant.clone();
    crate::bind::spell_x::lower(&mut grant).unwrap();
    assert_eq!(
        grant, before,
        "a gained ability's announced X has a different owner"
    );

    let mut delayed = spell.clone();
    delayed["args"]["body"]["instructions"][0]["args"]["operation"] = json!({"kind":"CreateDelayedTrigger","args":{
        "event":{"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}},
        "start":"Immediately","expires":null,"intervening_if":null,"body":spell["args"]["body"]
    }});
    let before = delayed.clone();
    crate::bind::spell_x::lower(&mut delayed).unwrap();
    assert_eq!(
        delayed, before,
        "a deferred body retains explicit creator references"
    );

    let mut invalid = spell;
    invalid["unexpected"] = json!(true);
    assert!(crate::bind::spell_x::lower(&mut invalid).is_err());
}
