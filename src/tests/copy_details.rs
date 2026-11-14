use super::*;

#[test]
fn copied_names_print_the_current_literal_independently_of_the_source_name() {
    let text = "This creature becomes a copy of target creature until end of turn, except its name is Context example.";
    let parsed = engine().parse("Context example", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let mut reading = parsed.readings[0].clone();
    let name = &mut reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"]
        [0]["args"]["exceptions"][0];
    assert_eq!(name, &json!({"kind":"SetName","args":"Context example"}));
    name["args"] = json!("Command Tower");
    let text = printed_types(&reading, &["Creature"]);
    assert!(text.contains("name is Command Tower"), "{text}");
    assert!(!text.contains("name is Context example"), "{text}");
    let printed = engine()
        .print_keyed("Another source", &reading, &["Creature".into()], &[])
        .unwrap()
        .expect("an explicit copied name is independent of its creator");
    assert!(printed.contains("name is Command Tower"), "{printed}");
    assert!(
        engine()
            .parse("Another source", &printed)
            .unwrap()
            .readings
            .contains(&reading)
    );
}

#[test]
fn indefinite_copy_exceptions_keep_the_current_ability_and_graveyard_target() {
    let text = "{X}: Tap target land. This creature becomes a copy of target creature card in your graveyard with mana value X, except it has flying and this ability. Activate only as a sorcery.";
    let parsed = engine().parse("Context example", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let mut reading = parsed.readings[0].clone();
    let args = &mut reading[0]["args"];
    assert_eq!(args["cost"], json!({"parts":[{"Mana":["X"]}]}));
    let target = &mut args["announcement"]["targets"][1];
    assert_eq!(
        target["kind"],
        json!({"Object":{"kind":"Card","zones":["Graveyard"]}})
    );
    crate::traverse::rewrite(&mut target["filter"], &mut |v| {
        if v["kind"] == "CardType" && v["args"] == "Creature" {
            v["args"] = json!("Artifact");
        } else if v["kind"] == "AnnouncedX" {
            *v = json!({"kind":"Constant","args":3});
        }
        crate::traverse::Visit::Children
    });
    let copied = &mut args["body"]["instructions"][1]["args"]["operation"]["args"];
    assert_eq!(copied["duration"], "Indefinite");
    assert_eq!(
        copied["changes"][0]["args"]["object"],
        json!({"kind":"Target","args":{"slot":1,"index":0}})
    );
    let exceptions = &mut copied["changes"][0]["args"]["exceptions"];
    assert_eq!(
        *exceptions,
        json!([
            {"kind":"AddAbilities","args":[{"kind":"Keyword","args":{"kind":"Flying"}}]},
            {"kind":"AddCurrentAbility"}
        ])
    );
    exceptions[0]["args"][0]["args"]["kind"] = json!("Vigilance");
    let text = printed_types(&reading, &["Creature"]).to_lowercase();
    assert!(
        text.contains("{x}") && text.contains("artifact card"),
        "{text}"
    );
    assert!(text.contains("vigilance and this ability"), "{text}");
    assert!(
        !text.contains("flying") && !text.contains("until end of turn"),
        "{text}"
    );
}

#[test]
fn qualifiers_on_either_side_of_the_graveyard_have_one_target_meaning() {
    let mut expected = None;
    for text in [
        "Exile target creature card with mana value 3 from your graveyard.",
        "Exile target creature card with mana value 3 in your graveyard.",
        "Exile target creature card from your graveyard with mana value 3.",
        "Exile target creature card in your graveyard with mana value 3.",
    ] {
        let parsed = engine().parse("Context example", text).unwrap();
        assert_eq!(parsed.readings.len(), 1, "{text}");
        let reading = &parsed.readings[0];
        if let Some(expected) = &expected {
            assert_eq!(reading, expected, "{text}");
        } else {
            expected = Some(reading.clone());
        }
        assert_eq!(
            reading[0]["args"]["announcement"]["targets"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        printed_types(reading, &["Sorcery"]);
    }
}
