//! Computed subjects and current battlefield populations keep one lowering.
use super::*;

const POPULATION: &str = "/0/args/body/instructions/1/args/operation/args/amount/args";

fn population_example() -> Value {
    compositional(
        "Destroy target creature. Draw X cards, where X is the number of creatures you control.",
    )
}

fn redundant_population(reading: &mut Value) {
    let population = reading.pointer_mut(POPULATION).unwrap();
    assert_eq!(population["kind"], "Zone");
    *population = json!({"kind":"Matching","args":{
        "objects":population,"filter":{"kind":"InZone","args":{"Shared":"Battlefield"}}
    }});
}

#[test]
fn current_battlefield_populations_do_not_repeat_their_zone_filter() {
    let expected = population_example();
    let mut legacy = expected.clone();
    redundant_population(&mut legacy);
    validate_abilities(&legacy).unwrap();
    let canonical = canonical_abilities(&legacy).unwrap();
    assert_eq!(canonical, expected);
    assert_eq!(canonical_abilities(&canonical).unwrap(), canonical);
    printed_types(&legacy, &["Sorcery"]);
}

#[test]
fn redundant_zone_filter_keeps_id_origins_through_the_population_move() {
    use crate::model::{canonical, ir};
    let mut reading = population_example();
    reading.pointer_mut(POPULATION).unwrap()["args"]["filter"] = json!({
        "kind":"SameObject","args":{"kind":"Target","args":{"slot":0,"index":0}}
    });
    redundant_population(&mut reading);
    let card: ir::UnifiedCard = serde_json::from_value(json!({
        "schema_version":ir::SCHEMA_VERSION,"layout":"Card","data":[],"faces":[{
            "schema_version":ir::SCHEMA_VERSION,"name":"Context example",
            "characteristics":{"supertypes":[],"card_types":["Sorcery"],"subtypes":[],
                "mana_cost":null,"color_indicator":[],"power_toughness":null,"loyalty":null,
                "defense":null},"abilities":reading
        }]
    }))
    .unwrap();
    let mapped = canonical::mapped_unified(&card).unwrap();
    let prefix = format!("/faces/0/abilities{POPULATION}");
    assert_eq!(
        mapped.map.sites[&format!("{prefix}/args/objects/args/filter/args/args/slot")],
        [format!("{prefix}/args/filter/args/args/slot")]
    );
    assert!(mapped.map.introduced.is_empty());
    assert_eq!(
        serde_json::to_value(mapped.card).unwrap(),
        serde_json::to_value(canonical::unified(&card).unwrap()).unwrap()
    );
}

#[test]
fn zone_normalization_preserves_unknown_fields_and_different_membership() {
    for suffix in ["", "/args", "/args/filter"] {
        let mut invalid = population_example();
        redundant_population(&mut invalid);
        invalid
            .pointer_mut(&format!("{POPULATION}{suffix}"))
            .unwrap()["extra"] = json!(true);
        crate::model::canonical::abilities(&mut invalid).unwrap();
        assert_eq!(
            invalid.pointer(&format!("{POPULATION}{suffix}/extra")),
            Some(&json!(true))
        );
        assert!(validate_abilities(&invalid).is_err());
    }
    let mut other_zone = population_example();
    redundant_population(&mut other_zone);
    other_zone.pointer_mut(POPULATION).unwrap()["args"]["filter"]["args"]["Shared"] =
        json!("Exile");
    let normalized = canonical_abilities(&other_zone).unwrap();
    assert_eq!(normalized.pointer(POPULATION).unwrap()["kind"], "Matching");
    assert_ne!(normalized, population_example());
}

#[test]
fn object_stat_conditions_keep_the_object_predicate_lowering() {
    for text in [
        "Destroy target creature if its toughness is less than or equal to the number of cards in your graveyard.",
        "Whenever another green creature you control enters, if its power is greater than this creature's power, draw a card.",
    ] {
        let reading = compositional(text);
        let mut predicates = Vec::new();
        visit(&reading, "ObjectMatches", &mut predicates);
        assert_eq!(predicates.len(), 1, "{text}");
        assert_eq!(predicates[0]["filter"]["kind"], "Property");
        printed_types(&reading, &["Creature"]);
    }
}

#[test]
fn all_players_choose_before_all_players_sacrifice_their_unselected_lands() {
    let text = "Each player chooses five lands they control and sacrifices the rest.";
    let parsed = engine().parse("Context example", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let mut reading = parsed.readings[0].clone();
    let body = &reading[0]["args"]["body"]["instructions"];
    assert_eq!(body.as_array().unwrap().len(), 2);
    assert_eq!(body[0]["kind"], "ForEachPlayer");
    assert_eq!(body[0]["args"]["players"], json!({"kind":"All"}));
    let choices = &body[0]["args"]["body"]["instructions"];
    assert_eq!(
        choices.as_array().unwrap().len(),
        1,
        "all choices precede the sacrifice action"
    );
    assert_eq!(choices[0]["args"]["operation"]["kind"], "ChooseObjects");
    assert_eq!(
        body[1]["args"]["operation"]["args"]["players"],
        json!({"kind":"All"})
    );
    assert_eq!(
        body[1]["args"]["operation"]["args"]["objects"],
        json!({"kind":"Unselected","args":choices[0]["args"]["result"]})
    );
    printed_types(&reading, &["Sorcery"]);
    reading[0]["args"]["body"]["instructions"][0]["args"]["body"]["instructions"][0]["args"]["operation"]
        ["args"]["count"]["AsManyAsCan"] = json!({"kind":"Constant","args":3});
    assert!(printed_types(&reading, &["Sorcery"]).contains("three lands"));
}
