use super::*;

#[test]
fn loyalty_damage_keeps_its_amount_separate_from_the_optional_target_count() {
    let name = "Context example";
    let text =
        "[+2]: Add {R}. Context example deals 3 damage to up to one target player or planeswalker.";
    let readings = engine().parse(name, text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{readings:#?}");
    let mut reading = readings[0].clone();
    assert_eq!(
        reading[0]["args"]["announcement"]["targets"][0]["count"],
        json!({"UpTo":{"kind":"Constant","args":1}})
    );
    let amount =
        &mut reading[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["amount"];
    assert_eq!(*amount, json!({"kind":"Constant","args":3}));
    *amount = json!({"kind":"Constant","args":7});
    let printed = engine()
        .print_keyed(name, &reading, &["Planeswalker".into()], &[])
        .unwrap()
        .expect("edited loyalty damage");
    assert!(
        engine()
            .parse(name, &printed)
            .unwrap()
            .readings
            .contains(&reading),
        "{printed}"
    );
    assert!(printed.contains("7 damage"), "{printed}");
}

#[test]
fn closed_followups_preserve_their_receipts_definitions_and_fallbacks() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../tests/fixtures/conditional-followups.json"
    ))
    .unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let operands = &crate::test_support::fixture_abilities(&case["expected"]);
        let wanted = canonical_abilities(operands).unwrap();
        let readings = engine().parse(name, text).unwrap().readings;
        assert_eq!(
            readings.len(),
            case["readings"].as_u64().unwrap_or(1) as usize,
            "{name}"
        );
        assert!(readings.contains(&wanted), "{name}: {readings:#?}");
        let key = engine()
            .derivation_of(name, operands, text)
            .unwrap()
            .expect(name);
        let keyed = engine().print_keyed(name, operands, &[], &key).unwrap();
        if keyed.as_deref() != Some(text) {
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
fn exile_until_cast_uses_the_destroyed_objects_before_controller_after_a_prefix() {
    let text = "Draw a card. You gain that much life. Destroy target creature. If that creature is destroyed this way, its controller exiles cards from the top of their library until they exile a nonland card, then they may cast that card without paying its mana cost.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{readings:#?}");
    let reading = &readings[0];
    let mut destroyed = Vec::new();
    visit(reading, "Effect", &mut destroyed);
    let result = destroyed
        .iter()
        .find(|args| args["operation"]["kind"] == "Destroy")
        .unwrap()["result"]
        .clone();
    assert_ne!(result, json!(0));
    let controller = json!({"kind":"ControllerOf","args":{"kind":"Result","args":{
        "result":result,"phase":"Before","index":0
    }}});
    let mut reveal = Vec::new();
    visit(reading, "RevealUntil", &mut reveal);
    assert_eq!(reveal[0]["library"], controller);
    let mut optional = Vec::new();
    visit(reading, "Optional", &mut optional);
    assert_eq!(optional[0]["decider"], controller);
    let mut cast = Vec::new();
    visit(reading, "PlayCards", &mut cast);
    assert_eq!(cast[0]["actor"], json!({"kind":"One","args":controller}));
    printed_types(reading, &["Sorcery"]);
}

#[test]
fn conditional_optional_offer_has_a_fallback_when_the_condition_is_false() {
    let text = "If you control a creature, you may pay {1}. If you don't, draw a card.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{readings:#?}");
    let body = &readings[0][0]["args"]["body"]["instructions"];
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["kind"], "If");
    assert_eq!(
        body[0]["args"]["otherwise"]["instructions"][0]["args"]["operation"]["kind"],
        "Draw"
    );
    let mut draws = Vec::new();
    visit(&readings[0], "Draw", &mut draws);
    assert_eq!(draws.len(), 2);
    printed_types(&readings[0], &["Sorcery"]);
}
