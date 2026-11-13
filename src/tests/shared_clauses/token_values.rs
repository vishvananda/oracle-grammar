//! Token anaphors name all produced objects, including replacement-created extras.
use super::*;

fn aggregates(reading: &Value) -> Vec<Value> {
    let mut found = Vec::new();
    visit(reading, "Aggregate", &mut found);
    found
}

#[test]
fn satya_measures_and_sacrifices_the_whole_created_batch() {
    let text = "Menace, haste\nWhenever Satya attacks, create a tapped and attacking token \
                that's a copy of up to one other target nontoken creature you control. You \
                get {E}{E}. At the beginning of the next end step, sacrifice that token \
                unless you pay an amount of {E} equal to its mana value.";
    let parsed = engine().parse("Satya, Aetherflux Genius", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let reading = &parsed.readings[0];
    let mut created = Vec::new();
    visit(reading, "Created", &mut created);
    assert_eq!(created.len(), 2, "{reading}");
    assert_eq!(created[0], created[1]);
    assert_eq!(
        aggregates(reading),
        [json!({"objects":{"kind":"Created","args":created[0]},
            "property":"ManaValue","aggregation":"Sum"})]
    );
    let mut scalar = Vec::new();
    visit(reading, "ObjectProperty", &mut scalar);
    assert!(scalar.is_empty());
    let audited = engine()
        .audited(
            "Satya, Aetherflux Genius",
            text,
            &crate::parse::RuleSet::default(),
        )
        .unwrap();
    assert!(
        audited
            .iter()
            .any(|(_, rules)| rules.iter().all(Vec::is_empty))
    );
    let printed = engine()
        .print_keyed(
            "Satya, Aetherflux Genius",
            reading,
            &["Creature".into()],
            &[],
        )
        .unwrap()
        .expect("the batch property has a printing inverse");
    assert!(
        engine()
            .parse("Satya, Aetherflux Genius", &printed)
            .unwrap()
            .readings
            .contains(reading),
        "{printed}"
    );
}

#[test]
fn token_stats_print_current_property_and_receipt_after_an_unrelated_action() {
    let text = "Create a 1/1 white Soldier creature token. You get {E}{E}. You may pay an \
                amount of {E} equal to its mana value. If you do, draw a card.";
    let mut reading = compositional(text);
    let totals = aggregates(&reading);
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0]["objects"]["kind"], "Created");
    printed_types(&reading, &["Sorcery"]);
    let value = &mut reading[0]["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["cost"]
        ["parts"][0]["Energy"];
    value["args"]["property"] = json!("Power");
    let text = printed_types(&reading, &["Sorcery"]);
    assert!(text.contains("power"), "{text}");
    assert!(!text.contains("mana value"), "{text}");
}

#[test]
fn a_new_target_and_an_independent_ability_do_not_borrow_the_token_property() {
    for text in [
        "Create a 1/1 white Soldier creature token. Exile target creature. You gain life \
         equal to its mana value.",
        "When this creature enters, create a 1/1 white Soldier creature token.\n\
         When this creature dies, you gain life equal to its mana value.",
    ] {
        for reading in engine().parse("Context example", text).unwrap().readings {
            assert!(aggregates(&reading).is_empty(), "{reading}");
            let mut properties = Vec::new();
            visit(&reading, "ObjectProperty", &mut properties);
            assert_eq!(properties.len(), 1);
        }
    }
}

#[test]
fn an_explicit_token_possessive_keeps_its_batch_after_a_new_target() {
    let reading = compositional(
        "Create a 1/1 white Soldier creature token. Tap target artifact. You gain life \
         equal to that token's toughness.",
    );
    let totals = aggregates(&reading);
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0]["objects"]["kind"], "Created");
    assert_eq!(totals[0]["property"], "Toughness");
    printed_types(&reading, &["Sorcery"]);
    let plural = "Create two 1/1 white Soldier creature tokens. You gain life equal to \
                  its mana value.";
    assert!(engine().parse("Context example", plural).is_err());
}
