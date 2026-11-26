use super::*;
use crate::{ir::*, validate};

fn bound() -> Value {
    json!({"kind":"BoundValue","args":7})
}
fn count() -> Value {
    json!({"kind":"CountObjects","args":{"kind":"Zone","args":{
        "scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"CardType","args":"Creature"}}}})
}
fn token(amount: Value) -> Value {
    json!({"kind":"CreateTokens","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},
        "tokens":[{"amount":{"kind":"Constant","args":1},"words":"CreatingEffect","recipe":{"Defined":{
            "name":"FromSubtypes","colors":["Green"],"chosen_color":false,"chosen_creature_type":false,
            "supertypes":[],"card_types":["Creature"],"subtypes":["Saproling"],
            "power_toughness":[amount.clone(),amount],"abilities":[],"abilities_of":null}}}],
        "entry":{"controller":null,"tapped":false,"counters":[],"states":[]}}})
}
fn consumers(amount: Value) -> Vec<Value> {
    let mut out = Vec::new();
    for kind in ["Draw", "GainLife", "LoseLife", "Mill"] {
        out.push(json!({"kind":kind,"args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":amount}}));
    }
    out.push(
        json!({"kind":"DealDamage","args":{"source":{"kind":"Source"},
        "recipients":{"kind":"Players","args":{"kind":"All"}},"amount":amount,"preventable":true}}),
    );
    out.push(json!({"kind":"PutCounters","args":{"placements":[{"recipients":{"kind":"Objects","args":{"kind":"One","args":{"kind":"Source"}}},
        "kind":{"PowerToughness":[1,1]},"amount":amount,"selection":null}]}}));
    out.push(token(amount));
    out
}
fn ability(value: Value, operation: Value, capture: bool) -> Value {
    let mut instructions = Vec::new();
    if capture {
        instructions.push(json!({"kind":"Effect","args":{"result":7,
        "operation":{"kind":"BindValue","args":{"value":value}}}}));
    }
    instructions.push(json!({"kind":"Effect","args":{"result":9,"operation":operation}}));
    json!({"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},"body":{"instructions":instructions}}})
}
fn compared(value: Value) -> Value {
    let mut value = json!([value]);
    semantic_abilities(&mut value).unwrap();
    value
}
fn valid(value: &Value) {
    validate::validate_ability(&serde_json::from_value(value.clone()).unwrap()).unwrap();
}

#[test]
fn immediate_scalar_parameters_and_one_token_share_the_captures_evaluation_point() {
    for (captured, direct) in consumers(bound()).into_iter().zip(consumers(count())) {
        let a = ability(count(), captured, true);
        valid(&a);
        let normalized = compared(a.clone());
        let direct = compared(ability(count(), direct, false));
        assert_eq!(normalized, direct);
        valid(&normalized[0]);
        assert_eq!(compared(normalized[0].clone()), normalized);
        assert_eq!(
            normalized[0]["args"]["body"]["instructions"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let mut authored = json!([a]);
        reduced(&mut authored).unwrap();
        assert_eq!(
            authored[0]["args"]["body"]["instructions"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
}

#[test]
fn inverse_parameter_views_use_fresh_ids_and_preserve_current_operands() {
    for direct in consumers(count()) {
        let original = json!([ability(count(), direct, false)]);
        let views = immediate_parameter_print_views(&original).unwrap();
        assert!(!views.is_empty());
        assert!(views.len() <= 3);
        for view in views {
            valid(&view[0]);
            assert_eq!(compared(view[0].clone()), compared(original[0].clone()));
            let items = view[0]["args"]["body"]["instructions"].as_array().unwrap();
            assert_eq!(items.len(), 2);
            assert_ne!(items[0]["args"]["result"], items[1]["args"]["result"]);
            assert_eq!(items[0]["args"]["operation"]["args"]["value"], count());
        }
    }
}

#[test]
fn inverse_captures_have_a_fixed_bound_and_avoid_unused_definition_ids() {
    let mut original = ability(count(), consumers(count()).remove(0), false);
    let item = original["args"]["body"]["instructions"][0].clone();
    original["args"]["body"]["instructions"] = Value::Array(
        (0..66)
            .map(|id| {
                let mut next = item.clone();
                next["args"]["result"] = json!(id);
                next
            })
            .collect(),
    );
    valid(&original);
    let views = immediate_parameter_print_views(&json!([original.clone()])).unwrap();
    assert_eq!(views.len(), 1);
    let items = views[0][0]["args"]["body"]["instructions"]
        .as_array()
        .unwrap();
    assert_eq!(items.len(), 66 + 64);
    assert!(
        items
            .iter()
            .filter(|item| item["args"]["operation"]["kind"] == "BindValue")
            .all(|item| item["args"]["result"].as_u64().unwrap() >= 66)
    );
    valid(&views[0][0]);
    assert_eq!(compared(views[0][0].clone()), compared(original));
}

#[test]
fn later_uses_intervening_effects_and_multi_player_parameters_keep_the_capture() {
    let operation = consumers(bound()).remove(0);
    for variant in 0..3 {
        let mut a = ability(count(), operation.clone(), true);
        if variant == 2 {
            a["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["players"] =
                json!({"kind":"All"});
        } else {
            let mut extra = a["args"]["body"]["instructions"][1].clone();
            extra["args"]["result"] = json!(10);
            if variant == 1 {
                extra["args"]["operation"]["args"]["amount"] = json!({"kind":"Constant","args":1});
            }
            let at = if variant == 0 { 2 } else { 1 };
            a["args"]["body"]["instructions"]
                .as_array_mut()
                .unwrap()
                .insert(at, extra);
        }
        valid(&a);
        assert!(
            compared(a).to_string().contains("BindValue"),
            "variant {variant}"
        );
    }
}

#[test]
fn token_batches_entry_state_and_ability_scopes_keep_the_capture() {
    for variant in 0..6 {
        let mut operation = token(bound());
        match variant {
            0 => operation["args"]["players"] = json!({"kind":"All"}),
            1 => operation["args"]["tokens"][0]["amount"] = json!({"kind":"Constant","args":2}),
            2 => operation["args"]["tokens"][0]["words"] = json!("Rules"),
            3 => operation["args"]["entry"]["tapped"] = json!(true),
            4 => {
                operation["args"]["tokens"][0]["recipe"]["Defined"]["abilities"] =
                    json!([{"kind":"Keyword","args":{"kind":"FirstStrike"}}])
            }
            _ => {
                let copy = operation["args"]["tokens"][0].clone();
                operation["args"]["tokens"]
                    .as_array_mut()
                    .unwrap()
                    .push(copy);
            }
        }
        let a = ability(count(), operation, true);
        valid(&a);
        assert!(
            compared(a).to_string().contains("BindValue"),
            "variant {variant}"
        );
    }
}

#[test]
fn live_effects_history_and_invalid_captured_information_are_not_pure_parameters() {
    let operation = json!({"kind":"CreateContinuousEffect","args":{"source":{"kind":"Source"},
        "affected":{"Objects":{"kind":"One","args":{"kind":"Source"}}},
        "changes":[{"kind":"Pump","args":{"power":bound(),"toughness":bound()}}],
        "duration":"EndOfTurn","evaluation":"LiveObjects"}});
    assert!(
        compared(ability(count(), operation, true))
            .to_string()
            .contains("BindValue")
    );
    let history = json!({"kind":"EventHistory","args":{"event":{"Object":{"event":"Attacks","objects":{"kind":"Any"}}},
        "window":"ThisTurn","measure":"Occurrences"}});
    let a = ability(history, consumers(bound()).remove(0), true);
    valid(&a);
    assert!(compared(a).to_string().contains("BindValue"));
    let captured = json!({"kind":"ObjectProperty","args":{"object":{"kind":"Source"},"property":"Power","information":{"Captured":2}}});
    let a = ability(captured, consumers(bound()).remove(0), true);
    let typed: Ability = serde_json::from_value(a.clone()).unwrap();
    assert!(validate::validate_ability(&typed).is_err());
    let a = compared(a);
    assert!(validate::validate_ability(&serde_json::from_value(a[0].clone()).unwrap()).is_err());
    assert!(a.to_string().contains("BindValue"));
}

#[test]
fn signed_literals_never_make_a_dynamic_counter_count_invalid() {
    let signed = json!({"kind":"Multiply","args":[{"kind":"Constant","args":-1},bound()]});
    let a = ability(
        json!({"kind":"Constant","args":2}),
        consumers(signed).remove(5),
        true,
    );
    valid(&a);
    let a = compared(a);
    valid(&a[0]);
    assert!(a.to_string().contains("BindValue"));
}

#[test]
fn scalar_substitution_keeps_cloned_target_origins_and_removed_capture_sites_consistent() {
    let packet: Value =
        serde_json::from_str(include_str!("../fixtures/extraction-190.json")).unwrap();
    let mut card = crate::migrate::read_json(&packet["records"][0]["card"].to_string()).unwrap();
    let value = json!({"kind":"ObjectProperty","args":{"object":{"kind":"Target","args":{"slot":11,"index":0}},
        "property":"Power","information":"CurrentOrLastKnown"}});
    let mut a = ability(value, token(bound()), true);
    a["args"]["announcement"]["targets"] = json!([{"slot":11,"kind":{"Object":{"kind":"Permanent","zones":["Battlefield"]}},
        "count":{"Exactly":{"kind":"Constant","args":1}},"filter":{"kind":"CardType","args":"Creature"},
        "players":null,"distribute":null,"constraints":[],"chooser":null}]);
    card.faces[0].abilities = vec![serde_json::from_value(a).unwrap()];
    validate::unified(&card).unwrap();
    let mapped = mapped_unified(&card).unwrap();
    assert!(
        !serde_json::to_string(&mapped.card)
            .unwrap()
            .contains("BindValue")
    );
}
