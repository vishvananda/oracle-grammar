//! One immediate, sole-use capture and a Snapshot pump have one native meaning.
use super::*;

const PROGRAM: &str = "/0/args/body/instructions";
fn instructions(value: &mut Value) -> &mut Vec<Value> {
    value.pointer_mut(PROGRAM).unwrap().as_array_mut().unwrap()
}
fn stats(instruction: &mut Value) -> &mut Value {
    &mut instruction["args"]["operation"]["args"]["changes"][0]["args"]
}
fn captured(mut reading: Value, at: usize) -> Value {
    let items = instructions(&mut reading);
    let definition = stats(&mut items[at])["power"].clone();
    assert_eq!(definition["kind"], "CountObjects");
    for field in ["power", "toughness"] {
        let operand = &mut stats(&mut items[at])[field];
        if operand == &definition {
            *operand = json!({"kind":"BoundValue","args":42});
        } else if operand["kind"] == "Multiply" && operand["args"][1] == definition {
            operand["args"][1] = json!({"kind":"BoundValue","args":42});
        } else {
            assert_eq!(operand["kind"], "Constant");
        }
    }
    items.insert(
        at,
        json!({"kind":"Effect","args":{"result":42,
        "operation":{"kind":"BindValue","args":{"value":definition}}}}),
    );
    reading
}
fn bind_count(reading: &Value) -> usize {
    let mut found = Vec::new();
    visit(reading, "BindValue", &mut found);
    found.len()
}

#[test]
fn immediate_snapshot_pumps_inline_their_only_capture_and_print_edited_values() {
    for change in ["+X/+X", "+X/+0", "+X/-X"] {
        let text = format!(
            "This creature gets {change} until end of turn, where X is the number of creatures you control."
        );
        let reading = compositional(&text);
        let mut legacy = captured(reading.clone(), 0);
        validate_abilities(&legacy).unwrap();
        let normalized = canonical_abilities(&legacy).unwrap();
        assert_eq!(normalized, reading);
        assert_eq!(canonical_abilities(&normalized).unwrap(), normalized);
        printed(&legacy);
        let definition = &mut instructions(&mut legacy)[0]["args"]["operation"]["args"]["value"];
        let filter = &mut definition["args"]["args"]["filter"];
        fn artifact(value: &mut Value) {
            if *value == json!({"kind":"CardType","args":"Creature"}) {
                value["args"] = json!("Artifact");
            } else if let Some(values) = value.as_array_mut() {
                values.iter_mut().for_each(artifact);
            } else if let Some(fields) = value.as_object_mut() {
                fields.values_mut().for_each(artifact);
            }
        }
        artifact(filter);
        let edited = printed(&legacy);
        assert!(
            edited.contains("artifact") && edited.contains("you control"),
            "{edited}"
        );
    }
}

#[test]
fn snapshot_captures_cannot_move_across_an_action_or_outlive_their_only_consumer() {
    let reading = compositional(
        "Draw a card. This creature gets +X/+X until end of turn, where X is the number of cards in your hand.",
    );
    let legacy = captured(reading.clone(), 1);
    assert_eq!(canonical_abilities(&legacy).unwrap(), reading);
    let mut early = legacy.clone();
    instructions(&mut early).swap(0, 1);
    validate_abilities(&early).unwrap();
    let normalized = canonical_abilities(&early).unwrap();
    assert_eq!(bind_count(&normalized), 1);
    assert_ne!(normalized, reading);
    let mut reused = legacy;
    let mut draw = instructions(&mut reused)[0].clone();
    draw["args"]["result"] = json!(43);
    draw["args"]["operation"]["args"]["amount"] = json!({"kind":"BoundValue","args":42});
    instructions(&mut reused).push(draw);
    validate_abilities(&reused).unwrap();
    assert_eq!(bind_count(&canonical_abilities(&reused).unwrap()), 1);
}

#[test]
fn live_effects_and_nonlocal_values_keep_their_capture() {
    let reading = compositional(
        "This creature gets +X/+X until end of turn, where X is the number of creatures you control.",
    );
    let legacy = captured(reading, 0);
    for evaluation in ["Live", "LiveObjects"] {
        let mut live = legacy.clone();
        instructions(&mut live)[1]["args"]["operation"]["args"]["evaluation"] = json!(evaluation);
        assert_eq!(bind_count(&canonical_abilities(&live).unwrap()), 1);
    }
    for object in ["Affected", "Candidate"] {
        let mut invalid = legacy.clone();
        instructions(&mut invalid)[0]["args"]["operation"]["args"]["value"] = json!({
            "kind":"ObjectProperty","args":{"object":{"kind":object},"property":"Power",
            "information":"CurrentOrLastKnown"}});
        assert!(validate_abilities(&invalid).is_err());
        let normalized = canonical_abilities(&invalid).unwrap();
        assert_eq!(bind_count(&normalized), 1);
        assert!(validate_abilities(&normalized).is_err());
    }
}

#[test]
fn extra_operands_are_preserved_for_validation_instead_of_repaired() {
    let reading = compositional(
        "This creature gets +X/+X until end of turn, where X is the number of creatures you control.",
    );
    let legacy = captured(reading, 0);
    for suffix in [
        "/0",
        "/0/args/operation/args",
        "/1/args/operation/args/changes/0/args",
    ] {
        let mut invalid = legacy.clone();
        invalid.pointer_mut(&format!("{PROGRAM}{suffix}")).unwrap()["extra"] = json!(true);
        assert!(validate_abilities(&invalid).is_err());
        let normalized = canonical_abilities(&invalid).unwrap();
        assert_eq!(bind_count(&normalized), 1);
        assert!(validate_abilities(&normalized).is_err());
    }
}

#[test]
fn public_normalization_does_not_trust_forged_internal_origin_annotations() {
    let reading = compositional(
        "This creature gets +X/+X until end of turn, where X is the number of creatures you control.",
    );
    let mut invalid = captured(reading, 0);
    instructions(&mut invalid)[0]["args"]["result"] = json!({
        "$mtgish_id":42,"$site_origins":[0]});
    assert!(validate_abilities(&invalid).is_err());
    let normalized = canonical_abilities(&invalid).unwrap();
    assert_eq!(bind_count(&normalized), 1);
    assert!(validate_abilities(&normalized).is_err());
}

#[test]
fn snapshot_normalization_maps_cloned_query_ids_and_deleted_capture_sites() {
    use crate::model::{canonical, ir};
    let reading = compositional(
        "This creature gets +X/+X until end of turn, where X is the number of cards in target player's hand.",
    );
    let legacy = captured(reading, 0);
    let card: ir::UnifiedCard = serde_json::from_value(json!({
        "schema_version":ir::SCHEMA_VERSION,"layout":"Card","data":[],"faces":[{
            "schema_version":ir::SCHEMA_VERSION,"name":"Context example",
            "characteristics":{"supertypes":[],"card_types":["Sorcery"],"subtypes":[],
                "mana_cost":null,"color_indicator":[],"power_toughness":null,"loyalty":null,
                "defense":null},"abilities":legacy
        }]
    }))
    .unwrap();
    let mapped = canonical::mapped_unified(&card).unwrap();
    let prefix = "/faces/0/abilities/0/args/body/instructions";
    let query = "/args/args/scope/Players/players/args/args/slot";
    assert_eq!(
        mapped.map.sites[&format!("{prefix}/0/args/operation/args/value{query}")],
        [
            format!("{prefix}/0/args/operation/args/changes/0/args/power{query}"),
            format!("{prefix}/0/args/operation/args/changes/0/args/toughness{query}")
        ]
    );
    for suffix in [
        "/0/args/result",
        "/1/args/operation/args/changes/0/args/power/args",
        "/1/args/operation/args/changes/0/args/toughness/args",
    ] {
        assert!(mapped.map.sites[&format!("{prefix}{suffix}")].is_empty());
    }
    assert!(mapped.map.introduced.is_empty());
    assert_eq!(
        serde_json::to_value(mapped.card).unwrap(),
        serde_json::to_value(canonical::unified(&card).unwrap()).unwrap()
    );
}

#[test]
fn delayed_programs_keep_outer_captures_and_every_typed_result_use_is_counted() {
    let reading = compositional(
        "This creature gets +X/+X until end of turn, where X is the number of creatures you control.",
    );
    let legacy = captured(reading, 0);
    let mut delayed = legacy.clone();
    let draw = json!({"kind":"Effect","args":{"result":44,"operation":{
        "kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},
        "amount":{"kind":"BoundValue","args":42}}}}});
    instructions(&mut delayed).push(json!({"kind":"Effect","args":{"result":43,
        "operation":{"kind":"CreateDelayedTrigger","args":{
            "event":{"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}},
            "start":"Immediately","expires":null,"intervening_if":null,
            "body":{"instructions":[draw]}}}}}));
    validate_abilities(&delayed).unwrap();
    let normalized = canonical_abilities(&delayed).unwrap();
    validate_abilities(&normalized).unwrap();
    assert_eq!(bind_count(&normalized), 1);
    let mut invalid = legacy;
    instructions(&mut invalid)[1]["args"]["operation"]["args"]["source"] = json!({
        "kind":"Result","args":{"result":42,"phase":"After","index":0}});
    assert!(validate_abilities(&invalid).is_err());
    let normalized = canonical_abilities(&invalid).unwrap();
    assert_eq!(bind_count(&normalized), 1);
    assert!(validate_abilities(&normalized).is_err());
}

#[test]
fn nested_ability_scopes_are_conservative_and_independent_of_raw_result_labels() {
    let reading = compositional(
        "This creature gets +X/+X until end of turn, where X is the number of creatures you control.",
    );
    let legacy = captured(reading.clone(), 0);
    let child = compositional("Whenever this creature attacks, draw a card.")[0].clone();
    let mut nested = legacy;
    let mut grant = instructions(&mut reading.clone())[0].clone();
    grant["args"]["result"] = json!(43);
    grant["args"]["operation"]["args"]["changes"] = json!([{"kind":"GrantAbility","args":child}]);
    instructions(&mut nested).push(grant);
    validate_abilities(&nested).unwrap();
    let normalized = canonical_abilities(&nested).unwrap();
    assert_eq!(bind_count(&normalized), 1);
    let mut reduced = nested.clone();
    crate::model::canonical::reduced(&mut reduced).unwrap();
    assert_eq!(canonical_abilities(&reduced).unwrap(), normalized);
    let child_result = "/0/args/body/instructions/2/args/operation/args/changes/0/args/args/body/instructions/0/args/result";
    *nested.pointer_mut(child_result).unwrap() = json!(0);
    assert_eq!(canonical_abilities(&nested).unwrap(), normalized);
}

#[test]
fn modal_local_captures_normalize_after_alpha_renaming() {
    let reading = compositional(
        "This creature gets +X/+X until end of turn, where X is the number of creatures you control.",
    );
    let body = captured(reading.clone(), 0)[0]["args"]["body"].clone();
    let mut modal = compositional("Choose one —\n• Draw a card.\n• You gain 2 life.");
    for option in modal[0]["args"]["announcement"]["modes"]["options"]
        .as_array_mut()
        .unwrap()
    {
        option["body"] = body.clone();
    }
    validate_abilities(&modal).unwrap();
    let normalized = canonical_abilities(&modal).unwrap();
    assert_eq!(bind_count(&normalized), 0);
    assert_eq!(canonical_abilities(&normalized).unwrap(), normalized);
    let mut reduced = modal;
    crate::model::canonical::reduced(&mut reduced).unwrap();
    assert_eq!(canonical_abilities(&reduced).unwrap(), normalized);
}

#[test]
fn local_filter_candidates_keep_their_scope_and_constant_negation_is_checked() {
    let reading = compositional(
        "This creature gets +X/-X until end of turn, where X is the number of creatures you control.",
    );
    let legacy = captured(reading, 0);
    let mut filtered = legacy.clone();
    let definition = &mut instructions(&mut filtered)[0]["args"]["operation"]["args"]["value"];
    definition["args"]["args"]["filter"] = json!({"kind":"Property","args":{
        "property":"Power","test":{"kind":"Compare","args":{"comparison":"Greater",
        "value":{"kind":"ObjectProperty","args":{"object":{"kind":"Candidate"},
        "property":"Toughness","information":"CurrentOrLastKnown"}}}}}});
    validate_abilities(&filtered).unwrap();
    let normalized = canonical_abilities(&filtered).unwrap();
    assert_eq!(bind_count(&normalized), 0);
    validate_abilities(&normalized).unwrap();
    for n in [3, i64::MIN] {
        let mut constant = legacy.clone();
        instructions(&mut constant)[0]["args"]["operation"]["args"]["value"] =
            json!({"kind":"Constant","args":n});
        let normalized = canonical_abilities(&constant).unwrap();
        if n == 3 {
            assert_eq!(bind_count(&normalized), 0);
            let operands = stats(&mut instructions(&mut normalized.clone())[0]).clone();
            assert_eq!(operands["power"], json!({"kind":"Constant","args":3}));
            assert_eq!(operands["toughness"], json!({"kind":"Constant","args":-3}));
            printed(&constant);
        } else {
            assert_eq!(bind_count(&normalized), 1);
        }
        assert_eq!(canonical_abilities(&normalized).unwrap(), normalized);
    }
}
