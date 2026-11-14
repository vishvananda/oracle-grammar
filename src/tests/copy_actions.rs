use super::*;

fn one(text: &str) -> Value {
    let mut readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{text}: {readings:#?}");
    readings.remove(0)
}

#[test]
fn copied_abilities_doors_and_activation_reductions_keep_original_meanings_and_prints() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../../tests/fixtures/copy-actions.json")).unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let wanted = canonical_abilities(&case["expected"]).unwrap();
        let parsed = engine().parse(name, text).unwrap().readings;
        assert_eq!(
            parsed.len(),
            case["readings"].as_u64().unwrap() as usize,
            "{name}"
        );
        assert!(parsed.contains(&wanted), "{name}");
        let types: Vec<String> = serde_json::from_value(case["types"].clone()).unwrap();
        let operands = &case["expected"];
        let printed = engine()
            .print_keyed(name, operands, &types, &[])
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
        let key = engine()
            .derivation_of(name, operands, text)
            .unwrap()
            .expect(name);
        if engine()
            .print_keyed(name, operands, &types, &key)
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
fn copied_token_quantities_print_the_current_count_and_independent_ability() {
    let text = "Create a token that's a copy of target creature, except it has haste and \"At the beginning of the end step, exile this token.\"";
    let mut reading = one(text);
    let token = &mut reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["tokens"]
        [0];
    assert_eq!(token["amount"], json!({"kind":"Constant","args":1}));
    token["amount"]["args"] = json!(3);
    token["recipe"]["Copy"]["exceptions"][0]["args"][0]["args"] = json!({"kind":"Flying"});
    let printed = printed_types(&reading, &["Sorcery"]);
    assert!(
        printed.contains("three tokens") || printed.contains("3 tokens"),
        "{printed}"
    );
    assert!(printed.contains("flying"), "{printed}");
    assert!(!printed.contains("exile Context example"), "{printed}");
}

#[test]
fn copied_token_holder_and_named_creator_remain_distinct() {
    for (reference, expected) in [
        ("this token", "Source"),
        ("Context example", "GrantingObject"),
    ] {
        let text = format!(
            "Create a token that's a copy of target creature, except it has haste and \"At the beginning of the end step, exile {reference}.\""
        );
        let reading = one(&text);
        let mut movement = Vec::new();
        visit(&reading, "Move", &mut movement);
        assert_eq!(movement.len(), 1);
        assert_eq!(
            movement[0]["objects"],
            json!({"kind":"One","args":{"kind":expected}})
        );
        printed_types(&reading, &["Sorcery"]);
    }
}

#[test]
fn entry_copy_keeps_its_existing_holder() {
    let text = "You may have Context example enter as a copy of any creature on the battlefield, except it has \"At the beginning of the end step, sacrifice Context example.\"";
    let reading = one(text);
    let mut sacrifice = Vec::new();
    visit(&reading, "Sacrifice", &mut sacrifice);
    assert_eq!(sacrifice.len(), 1);
    assert_eq!(
        sacrifice[0]["objects"],
        json!({"kind":"One","args":{"kind":"Source"}})
    );
    printed_types(&reading, &["Creature"]);
}

#[test]
fn exiled_card_copy_uses_the_current_activation_component() {
    let text = "{2}, {T}, Exile a creature card from your graveyard: Create a token that's a copy of the exiled card, except it's an artifact in addition to its other types.";
    let mut reading = one(text);
    let object = &reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["tokens"]
        [0]["recipe"]["Copy"]["object"];
    assert_eq!(
        object["args"]["objects"],
        json!({"kind":"CostObjects","args":{
            "payment":"Activation","component":[2],"view":"Performed","phase":"After"
        }})
    );
    reading[0]["args"]["cost"]["parts"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"Life":{"kind":"Constant","args":1}}));
    reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["tokens"][0]["recipe"]
        ["Copy"]["object"]["args"]["objects"]["args"]["component"] = json!([3]);
    printed_types(&reading, &["Artifact"]);
    let plural = "{2}, Exile two creature cards from your graveyard: Create a token that's a copy of the exiled card, except it's an artifact in addition to its other types.";
    assert!(engine().parse("Context example", plural).is_err());
}

#[test]
fn seek_filter_spell_reference_binds_before_the_move_receipt() {
    let text = "Counter target spell unless its controller pays {2}. If there are seven or more cards in your graveyard, seek a creature card with mana value equal to that spell's mana value.";
    let reading = one(text);
    let mut properties = Vec::new();
    visit(&reading, "ObjectProperty", &mut properties);
    assert_eq!(properties.len(), 1);
    assert_eq!(
        properties[0]["object"],
        json!({"kind":"Target","args":{"slot":0,"index":0}})
    );
    printed_types(&reading, &["Instant"]);
}

#[test]
fn door_action_after_observable_prefix_and_variable_reduction_keep_current_operands() {
    let text =
        "Draw a card. You gain that much life. Lock or unlock a door of target Room you control.";
    let reading = one(text);
    let mut actions = Vec::new();
    visit(&reading, "Keyword", &mut actions);
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0]["actor"],
        json!({"kind":"One","args":{"kind":"AbilityController"}})
    );
    assert_eq!(
        actions[0]["action"]["LockOrUnlockDoor"]["object"],
        json!({"kind":"Target","args":{"slot":0,"index":0}})
    );
    printed_types(&reading, &["Sorcery"]);

    let mut reduction = one(
        "{8}: Draw a card. This ability costs {3} less to activate for each charge counter on this permanent. Activate only as a sorcery.",
    );
    let modifiers = &mut reduction[0]["args"]["limits"]["modifiers"];
    let mut products = Vec::new();
    visit(modifiers, "Multiply", &mut products);
    assert_eq!(products.len(), 1);
    assert_eq!(products[0][0], json!({"kind":"Constant","args":3}));
    crate::traverse::rewrite(modifiers, &mut |node| {
        if node["kind"] == "Multiply" {
            node["args"][0] = json!({"kind":"Constant","args":5});
        }
        crate::traverse::Visit::Children
    });
    let printed = printed_types(&reduction, &["Artifact"]);
    assert!(printed.contains("costs {5} less"), "{printed}");
}
