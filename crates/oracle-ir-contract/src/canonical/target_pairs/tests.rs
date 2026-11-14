use super::*;
use crate::{canonical, validate};

fn target(slot: u16) -> Value {
    json!({"kind":"Target","args":{"slot":slot,"index":0}})
}
fn spec(slot: u16, filter: Value) -> Value {
    json!({"slot":slot,"kind":{"Object":{"kind":"Permanent","zones":["Battlefield"]}},
        "count":{"Exactly":{"kind":"Constant","args":1}},"filter":filter,
        "players":null,"distribute":null,"constraints":[],"chooser":null})
}
fn sample() -> Value {
    let filter = json!({"kind":"CardType","args":"Creature"});
    json!([{"kind":"Spell","args":{"announcement":{"modes":null,"targets":[
    spec(7,filter.clone()), spec(9,json!({"kind":"AllOf","args":[filter,
        {"kind":"Not","args":{"kind":"SameObject","args":target(7)}}]}))]},
    "body":{"instructions":[{"kind":"Effect","args":{"result":13,"operation":{
        "kind":"Keyword","args":{"actor":{"kind":"One","args":{"kind":"AbilityController"}},
            "action":{"ExchangeControl":{"first":target(7),"second":target(9)}}}
    }}}]}}}])
}
fn valid(value: &Value) -> bool {
    serde_json::from_value::<Vec<Ability>>(value.clone())
        .unwrap()
        .iter()
        .all(|ability| validate::validate_ability(ability).is_ok())
}
fn targets(value: &mut Value) -> &mut Value {
    &mut value[0]["args"]["announcement"]["targets"]
}
fn second(value: &mut Value) -> &mut Value {
    &mut value[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["action"]["ExchangeControl"]
        ["second"]
}

#[test]
fn a_plain_distinct_exchange_pair_has_one_valid_idempotent_target_group() {
    let mut split = sample();
    assert!(valid(&split));
    let mut grouped = sample();
    targets(&mut grouped).as_array_mut().unwrap().truncate(1);
    targets(&mut grouped)[0]["count"] = json!({"Exactly":{"kind":"Constant","args":2}});
    *second(&mut grouped) = json!({"kind":"Target","args":{"slot":7,"index":1}});
    canonical::abilities(&mut split).unwrap();
    canonical::abilities(&mut grouped).unwrap();
    assert_eq!(split, grouped);
    assert!(valid(&split));
    let once = split.clone();
    canonical::abilities(&mut split).unwrap();
    assert_eq!(split, once);
}

#[test]
fn different_filters_optional_slots_and_cross_slot_metadata_keep_their_shapes() {
    for variant in 0..5 {
        let mut value = sample();
        match variant {
            0 => {
                targets(&mut value)[1]["filter"]["args"][0] =
                    json!({"kind":"CardType","args":"Artifact"})
            }
            1 => targets(&mut value)[1]["count"] = json!({"UpTo":{"kind":"Constant","args":1}}),
            2 => targets(&mut value)[1]["filter"] = json!({"kind":"CardType","args":"Creature"}),
            3 => targets(&mut value)[0]["constraints"] = json!(["DifferentControllers"]),
            _ => targets(&mut value)[0]["chooser"] = json!({"kind":"Active"}),
        }
        canonical::abilities(&mut value).unwrap();
        assert_eq!(
            targets(&mut value).as_array().unwrap().len(),
            2,
            "variant {variant}"
        );
    }
}

#[test]
fn extra_instructions_and_target_dependent_filters_do_not_supply_the_proof() {
    let mut value = sample();
    let repeat = value[0]["args"]["body"]["instructions"][0].clone();
    value[0]["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .push(repeat);
    canonical::abilities(&mut value).unwrap();
    assert_eq!(targets(&mut value).as_array().unwrap().len(), 2);
    let mut value = sample();
    let dependent = json!({"kind":"SameObject","args":target(7)});
    targets(&mut value)[0]["filter"] = dependent.clone();
    targets(&mut value)[1]["filter"]["args"][0] = dependent;
    canonical::abilities(&mut value).unwrap();
    assert_eq!(targets(&mut value).as_array().unwrap().len(), 2);
}

#[test]
fn unknown_fields_and_invalid_target_addresses_cannot_be_repaired() {
    let mut value = sample();
    value[0]["args"]["extra"] = json!(true);
    canonical::abilities(&mut value).unwrap();
    assert_eq!(targets(&mut value).as_array().unwrap().len(), 2);
    let mut value = sample();
    targets(&mut value)[1]["slot"] = json!(7);
    assert!(!valid(&value));
    canonical::abilities(&mut value).unwrap();
    assert!(!valid(&value));
    assert_eq!(targets(&mut value).as_array().unwrap().len(), 2);
}

#[test]
fn independent_spree_abilities_normalize_in_their_own_namespaces() {
    let ability = sample()[0].clone();
    let mut value = json!([{"kind":"Spree","args":{"modes":[
        {"cost":{"parts":[{"Mana":[{"Generic":1}]}]},"ability":ability},
        {"cost":{"parts":[{"Mana":[{"Generic":2}]}]},"ability":ability}
    ]}}]);
    assert!(valid(&value));
    canonical::abilities(&mut value).unwrap();
    assert!(valid(&value));
    for mode in value[0]["args"]["modes"].as_array().unwrap() {
        assert_eq!(
            mode["ability"]["args"]["announcement"]["targets"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn mapped_target_declarations_coalesce_without_losing_the_second_operand_origin() {
    let source:crate::ir::UnifiedCard=serde_json::from_value(json!({
        "schema_version":crate::ir::SCHEMA_VERSION,"layout":"Card","data":[],"faces":[{
            "schema_version":crate::ir::SCHEMA_VERSION,"name":"Exchange example",
            "characteristics":{"supertypes":[],"card_types":["Sorcery"],"subtypes":[],
                "mana_cost":[],"color_indicator":[],"power_toughness":null,"loyalty":null,"defense":null},
            "abilities":sample()
        }]
    })).unwrap();
    let mapped = canonical::mapped_unified(&source).unwrap();
    assert_eq!(
        serde_json::to_value(&mapped.card).unwrap(),
        serde_json::to_value(canonical::unified(&source).unwrap()).unwrap()
    );
    let base = "/faces/0/abilities/0/args/announcement/targets/";
    assert_eq!(
        mapped.map.sites[&format!("{base}1/slot")],
        vec![format!("{base}0/slot")]
    );
    let reference = "/faces/0/abilities/0/args/body/instructions/0/args/operation/args/action/ExchangeControl/second/args/slot";
    assert_eq!(mapped.map.sites[reference], vec![reference.to_string()]);
    assert!(canonical::MappedCard::from_json(&serde_json::to_string(&mapped).unwrap()).is_ok());
}
