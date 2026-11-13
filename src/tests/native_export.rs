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
