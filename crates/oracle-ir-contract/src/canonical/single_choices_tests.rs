use super::*;
use crate::{ir::*, validate};

fn ability(count: Value, phase: &str, index: u32, selected: bool) -> Value {
    let objects = if selected {
        json!({"kind":"Selected","args":7})
    } else {
        json!({"kind":"One","args":{"kind":"Result","args":{"result":7,"phase":phase,"index":index}}})
    };
    json!({"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},"body":{"instructions":[
        {"kind":"Effect","args":{"result":7,"operation":{"kind":"ChooseObjects","args":{
            "player":{"kind":"AbilityController"},"objects":{"kind":"Zone","args":{
                "scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"CardType","args":"Creature"}}},
            "count":count,"constraints":[]}}}},
        {"kind":"Effect","args":{"result":9,"operation":{"kind":"Move","args":{"objects":objects,
            "destination":{"OwnersHand":{"counters":[],"states":[]}},"qualifiers":[]}}}}
    ]}}})
}
fn compared(value: Value) -> Value {
    let mut value = json!([value]);
    semantic_abilities(&mut value).unwrap();
    value
}
fn valid(value: &Value) -> bool {
    validate::validate_ability(&serde_json::from_value::<Ability>(value.clone()).unwrap()).is_ok()
}

#[test]
fn a_maximum_one_choice_and_its_direct_after_reference_name_the_selected_identities() {
    for kind in ["Exactly", "UpTo", "AsManyAsCan"] {
        let count = json!({kind:{"kind":"Constant","args":1}});
        let a = ability(count.clone(), "After", 0, false);
        assert!(valid(&a));
        let normalized = compared(a.clone());
        assert_eq!(normalized, compared(ability(count, "After", 0, true)));
        assert!(valid(&normalized[0]));
        assert_eq!(compared(normalized[0].clone()), normalized);
        let views = immediate_parameter_print_views(&normalized).unwrap();
        assert_eq!(views.len(), 1);
        assert!(valid(&views[0][0]));
        assert_eq!(compared(views[0][0].clone()), normalized);
        assert_eq!(
            views[0][0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["objects"]
                ["kind"],
            "One"
        );
        let mut authored = json!([a]);
        reduced(&mut authored).unwrap();
        assert_eq!(
            authored[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["objects"]
                ["kind"],
            "One"
        );
    }
}

#[test]
fn before_nonzero_plural_and_nonadjacent_queries_keep_their_own_meanings() {
    let plural = json!([ability(
        json!({"UpTo":{"kind":"Constant","args":2}}),
        "After",
        0,
        true
    )]);
    assert!(immediate_parameter_print_views(&plural).unwrap().is_empty());
    for (number, phase, index) in [(2, "After", 0), (1, "Before", 0), (1, "After", 1)] {
        let count = json!({"AsManyAsCan":{"kind":"Constant","args":number}});
        assert_ne!(
            compared(ability(count.clone(), phase, index, false)),
            compared(ability(count, phase, index, true))
        );
    }
    let mut a = ability(
        json!({"Exactly":{"kind":"Constant","args":1}}),
        "After",
        0,
        false,
    );
    a["args"]["body"]["instructions"].as_array_mut().unwrap().insert(1,json!({"kind":"Effect","args":{
        "result":8,"operation":{"kind":"GainLife","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}}}}}));
    assert!(valid(&a));
    assert_eq!(
        compared(a)[0]["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["objects"]["kind"],
        "One"
    );
}

#[test]
fn local_iteration_choices_are_single_but_exported_gathered_sets_are_not() {
    let sample = ability(
        json!({"AsManyAsCan":{"kind":"Constant","args":1}}),
        "After",
        0,
        false,
    );
    let loop_instruction = json!({"kind":"ForEachPlayer","args":{"players":{"kind":"All"},"binding":3,
        "starting_with":null,"in_chosen_direction":false,"body":sample["args"]["body"]}});
    let mut a = sample.clone();
    a["args"]["body"]["instructions"] = json!([loop_instruction]);
    assert!(valid(&a));
    let normalized = compared(a);
    assert!(valid(&normalized[0]));
    assert_eq!(
        normalized[0]["args"]["body"]["instructions"][0]["args"]["body"]["instructions"][1]["args"]
            ["operation"]["args"]["objects"]["kind"],
        "Selected"
    );
    let mut a = sample.clone();
    let mut gather = a["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    gather = json!({"kind":"ForEachPlayer","args":{"players":{"kind":"All"},"binding":3,
        "starting_with":null,"in_chosen_direction":false,"body":{"instructions":[gather]}}});
    a["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .insert(0, gather);
    assert!(!valid(&a));
    assert!(!valid(&compared(a)[0]));
}

#[test]
fn a_selected_reference_moves_its_original_result_id_site_without_cloning() {
    let packet: Value =
        serde_json::from_str(include_str!("../fixtures/extraction-190.json")).unwrap();
    let mut card = crate::migrate::read_json(&packet["records"][0]["card"].to_string()).unwrap();
    card.faces[0].abilities = vec![
        serde_json::from_value(ability(
            json!({"UpTo":{"kind":"Constant","args":1}}),
            "After",
            0,
            false,
        ))
        .unwrap(),
    ];
    validate::unified(&card).unwrap();
    let mapped = mapped_unified(&card).unwrap();
    assert_eq!(
        serde_json::to_value(mapped.card).unwrap()["faces"][0]["abilities"][0]["args"]["body"]["instructions"]
            [1]["args"]["operation"]["args"]["objects"]["kind"],
        "Selected"
    );
}

#[test]
fn malformed_program_shapes_supply_no_choice_or_capture_print_proof() {
    for body in [json!(1), json!(null), json!([])] {
        let mut a = ability(
            json!({"Exactly":{"kind":"Constant","args":1}}),
            "After",
            0,
            true,
        );
        a["args"]["body"] = body;
        assert!(
            immediate_parameter_print_views(&json!([a]))
                .unwrap()
                .is_empty()
        );
    }
}
