use super::*;

/// Lines that lower to separate statics for one player print as written,
/// though reduction holds them as one static with every change.
#[test]
fn statics_lowered_apart_print_where_they_were_written() {
    let lines = [
        "You may look at the top card of your library any time.",
        "You may play lands and cast spells from the top of your library.",
        "Once during each of your turns, you may cast a spell from your hand or the top of your library without paying its mana cost.",
    ];
    let mut lowered = Vec::new();
    for line in lines {
        let mut parsed = engine().parse("Example", line).unwrap().readings;
        assert_eq!(parsed.len(), 1, "{line}");
        lowered.extend(parsed.remove(0).as_array().unwrap().iter().cloned());
    }
    let abilities = Value::Array(lowered);
    let text = lines.join("\n");
    let key = engine()
        .derivation_of("Example", &abilities, &text)
        .unwrap()
        .expect("the written lines are reachable");
    assert_eq!(
        engine()
            .print_keyed("Example", &abilities, &[], &key)
            .unwrap()
            .as_deref(),
        Some(text.as_str())
    );
}

#[test]
fn shared_attachment_preserves_the_granted_ability_source_and_the_edited_gate() {
    let name = "Subject test";
    let types = ["Artifact".into(), "Equipment".into()];
    let text = "Equipped creature has lifelink if you control a Cleric, and \"{B}: Regenerate this creature\" if you control a Rogue.";
    let parsed = engine().parse_types(name, text, &types).unwrap();
    let [mut reading] = <[Value; 1]>::try_from(parsed.readings).unwrap();
    let affected = json!({"Objects":{"kind":"One","args":{
        "kind":"AttachedTo","args":{"kind":"Source"}
    }}});
    for arm in reading.as_array().unwrap() {
        assert_eq!(arm["args"]["then"][0]["args"]["affected"], affected);
    }
    let grant = &reading[1]["args"]["then"][0]["args"]["changes"][0];
    assert_eq!(grant["kind"], "GrantAbility");
    assert_eq!(
        grant["args"]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"],
        json!({"kind":"One","args":{"kind":"Source"}})
    );
    assert_static_trail(name, &types, &reading, text);

    let mut mixed_subjects = reading.clone();
    mixed_subjects[0]["args"]["then"][0]["args"]["affected"] =
        json!({"Objects":{"kind":"One","args":{"kind":"Source"}}});
    assert!(
        engine()
            .trail_of_types(name, &types, &mixed_subjects, text)
            .unwrap()
            .is_none()
    );

    reading[1]["args"]["condition"]["args"]["filter"]["args"] = json!("Wizard");
    let printed = engine()
        .print_keyed(name, &reading, &types, &[])
        .unwrap()
        .unwrap();
    let reparsed = engine().parse_types(name, &printed, &types).unwrap();
    assert_eq!(
        reparsed.readings,
        vec![canonical_abilities(&reading).unwrap()]
    );
    assert!(printed.contains("Wizard"), "{printed}");
    assert!(!printed.contains("Rogue"), "{printed}");
}

#[test]
fn a_shared_filter_can_be_recovered_in_every_printable_way() {
    let name = "Subject test";
    let types = ["Creature".into()];
    let snow = "Each snow creature has lifelink if it's blue, and vigilance if it's blue.";
    let blue = "Each blue creature has lifelink if it's snow, and vigilance if it's snow.";
    let parsed = engine().parse_types(name, snow, &types).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    assert_eq!(
        engine().parse_types(name, blue, &types).unwrap().readings,
        parsed.readings
    );
    for text in [snow, blue] {
        assert_static_trail(name, &types, &parsed.readings[0], text);
    }
}

fn assert_static_trail(name: &str, types: &[String], reading: &Value, text: &str) {
    let key = engine()
        .trail_of_types(name, types, reading, text)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine()
            .print_trail_types(name, types, reading, &key)
            .unwrap()
            .as_deref(),
        Some(text)
    );
}
