use super::*;

#[test]
fn entry_payments_and_legendary_equip_recover_original_wording() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../../tests/fixtures/entry-equip.json")).unwrap();
    assert_eq!(cases.len(), 30);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let operands = &case["expected"];
        let wanted = canonical_abilities(operands).unwrap();
        let readings = engine().parse(name, text).unwrap().readings;
        assert_eq!(
            readings.len(),
            case["readings"].as_u64().unwrap() as usize,
            "{name}"
        );
        assert!(readings.contains(&wanted), "{name}");
        let types = serde_json::from_value::<Vec<String>>(case["types"].clone()).unwrap();
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
fn entry_payment_prints_edited_life_and_preserves_its_actual_payer() {
    let original = "As this land enters, you may pay 2 life. If you don't, it enters tapped.";
    let mut reading = engine()
        .parse("Context example", original)
        .unwrap()
        .readings
        .remove(0);
    let amount = &mut reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]
        ["parts"][0]["Life"]["args"];
    assert_eq!(*amount, json!(2));
    *amount = json!(7);
    let printed = printed_types(&reading, &["Land"]);
    assert!(
        printed.contains("7 life") || printed.contains("seven life"),
        "{printed}"
    );
    assert_ne!(printed, original);
    reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["player"] =
        json!({"kind":"Active"});
    let wanted = canonical_abilities(&reading).unwrap();
    if let Some(text) = engine()
        .print_keyed("Context example", &reading, &["Land".into()], &[])
        .unwrap()
    {
        assert!(
            engine()
                .parse("Context example", &text)
                .unwrap()
                .readings
                .contains(&wanted),
            "{text}"
        );
    }
}

#[test]
fn legendary_equip_preserves_the_written_quality_and_edited_payment() {
    let text = "Equip legendary Goblin {2}";
    let parsed = engine().parse("Context example", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let mut reading = parsed.readings[0].clone();
    let filter = &reading[0]["args"]["announcement"]["targets"][0]["filter"];
    let mut subtypes = Vec::new();
    visit(filter, "Subtype", &mut subtypes);
    assert_eq!(subtypes, vec![json!("Goblin")]);
    let mut legendary = Vec::new();
    visit(filter, "Supertype", &mut legendary);
    assert_eq!(legendary, vec![json!("Legendary")]);
    reading[0]["args"]["cost"]["parts"][0]["Mana"][0]["Generic"] = json!(5);
    let filter = &mut reading[0]["args"]["announcement"]["targets"][0]["filter"];
    crate::traverse::rewrite(filter, &mut |node| {
        if node["kind"] == "Subtype" && node["args"] == "Goblin" {
            node["args"] = json!("Elf");
        }
        crate::traverse::Visit::Children
    });
    let printed = printed_types(&reading, &["Artifact"]);
    assert!(
        printed.contains("Elf") && printed.contains("{5}"),
        "{printed}"
    );
    assert!(!printed.contains("Goblin"), "{printed}");
}

#[test]
fn sorcery_and_state_restrictions_compose_without_an_ignored_quantity() {
    let text = "{2}: Draw a card. Activate only as a sorcery and only if you control a creature.";
    let parsed = engine().parse("Context example", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let mut reading = parsed.readings[0].clone();
    let limits = &mut reading[0]["args"]["limits"];
    assert_eq!(limits["sorcery"], json!(true));
    assert_eq!(limits["condition"]["kind"], "Controls");
    limits["condition"]["args"]["filter"] = json!({"kind":"CardType","args":"Artifact"});
    let printed = printed_types(&reading, &["Creature"]);
    assert!(printed.contains("artifact"), "{printed}");
    assert!(
        engine()
            .parse(
                "Context example",
                "{2}: Draw a card. Activate only as two sorcery and only if you control a creature."
            )
            .is_err()
    );
}
