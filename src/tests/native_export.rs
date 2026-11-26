use super::*;
use serde_json::json;

fn raw(id: &str, name: &str, text: &str) -> Value {
    json!({"oracle_id":id,"name":name,"layout":"normal","type_line":"Instant","mana_cost":"{U}","oracle_text":text})
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
        "Create a Food token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")",
        "Scry 2. (Look at the top card of your library. You may put that card on the bottom.)",
        "Choose one —\n• Draw a card.\n• Scry 1.\n• Surveil 1.\nEntwine {2} (Choose both if you pay the entwine cost.)",
    ];
    let records = cases.map(|text| raw("one", "Synthetic example", text));
    for face in scryfall(&records).unwrap() {
        let row = outcome(engine, &face);
        assert_eq!(row["status"], "refused", "{}: {row}", face.text);
    }
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
