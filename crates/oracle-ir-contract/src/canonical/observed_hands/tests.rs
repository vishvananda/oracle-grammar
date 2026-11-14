use super::*;
use crate::{canonical, ir::Ability, validate};

fn hand() -> Value {
    json!({"kind":"Zone","args":{"scope":{"Players":{"zone":"Hand",
        "players":{"kind":"One","args":{"kind":"AbilityController"}}}},
        "kind":"Card","filter":{"kind":"Any"}}})
}
fn effect(result: u32, operation: Value) -> Value {
    json!({"kind":"Effect","args":{"result":result,"operation":operation}})
}
fn sample() -> Value {
    json!([{"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},
    "body":{"instructions":[
        effect(17,json!({"kind":"Reveal","args":{"objects":hand()}})),
        effect(29,json!({"kind":"ChooseObjects","args":{
            "player":{"kind":"AbilityController"},"objects":hand(),
            "count":{"AsManyAsCan":{"kind":"Constant","args":1}},"constraints":[]}})),
        effect(31,json!({"kind":"DiscardObjects","args":{"objects":{"kind":"Selected","args":29}}}))
    ]}}}])
}
fn valid(value: &Value) -> bool {
    serde_json::from_value::<Vec<Ability>>(value.clone())
        .unwrap()
        .iter()
        .all(|a| validate::validate_ability(a).is_ok())
}
fn objects(value: &mut Value) -> &mut Value {
    &mut value[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["objects"]
}

#[test]
fn immediate_whole_hand_queries_share_the_observation_and_stay_valid() {
    let mut live = sample();
    assert!(valid(&live));
    let mut observed = live.clone();
    *objects(&mut observed) = json!({"kind":"Matching","args":{
        "objects":{"kind":"InZone","args":{"objects":{"kind":"Observed","args":17},"zone":"Hand"}},
        "filter":{"kind":"OwnedBy","args":{"kind":"One","args":{"kind":"AbilityController"}}}
    }});
    canonical::abilities(&mut live).unwrap();
    canonical::abilities(&mut observed).unwrap();
    assert_eq!(live, observed);
    assert!(valid(&live));
    assert_eq!(
        objects(&mut live)["args"]["objects"]["args"]["objects"]["args"],
        0
    );
    let once = live.clone();
    canonical::abilities(&mut live).unwrap();
    assert_eq!(once, live);
}

#[test]
fn partial_foreign_or_nonadjacent_hands_do_not_supply_the_proof() {
    let mut partial = sample();
    partial[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"]["args"]
        ["filter"] = json!({"kind":"CardType","args":"Creature"});
    canonical::abilities(&mut partial).unwrap();
    assert_eq!(objects(&mut partial)["kind"], "Zone");
    let mut foreign = sample();
    objects(&mut foreign)["args"]["scope"]["Players"]["players"] =
        json!({"kind":"One","args":{"kind":"Active"}});
    canonical::abilities(&mut foreign).unwrap();
    assert_eq!(objects(&mut foreign)["kind"], "Zone");
    let mut delayed = sample();
    delayed[0]["args"]["body"]["instructions"].as_array_mut().unwrap().insert(1,
        effect(5,json!({"kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}}})));
    canonical::abilities(&mut delayed).unwrap();
    assert_eq!(
        delayed[0]["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["objects"]["kind"],
        "Zone"
    );
}

#[test]
fn independent_abilities_do_not_borrow_a_hand_observation() {
    let mut independent = sample();
    let mut second = independent[0].clone();
    second["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    independent[0]["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    independent.as_array_mut().unwrap().push(second);
    canonical::abilities(&mut independent).unwrap();
    assert_eq!(
        independent[1]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"]["kind"],
        "Zone"
    );
}

#[test]
fn unknown_fields_and_duplicate_receipts_cannot_be_repaired() {
    let mut value = sample();
    let items = value[0]["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap();
    items[1]["args"]["operation"]["args"]["objects"]["extra"] = json!(true);
    let before = items.clone();
    let mut uses = Uses::default();
    uses.definitions.insert(17, 1);
    assert!(!normalize(items, &uses, false));
    assert_eq!(*items, before);
    let mut duplicate = sample();
    duplicate[0]["args"]["body"]["instructions"][1]["args"]["result"] = json!(17);
    assert!(!valid(&duplicate));
    canonical::abilities(&mut duplicate).unwrap();
    assert!(!valid(&duplicate));
    assert_eq!(objects(&mut duplicate)["kind"], "Zone");
}

#[test]
fn canonical_mapping_retains_added_receipt_and_original_player_origins() {
    let source:crate::ir::UnifiedCard=serde_json::from_value(json!({
        "schema_version":crate::ir::SCHEMA_VERSION,"layout":"Card","data":[],
        "faces":[{"schema_version":crate::ir::SCHEMA_VERSION,"name":"Hand example",
            "characteristics":{"supertypes":[],"card_types":["Sorcery"],"subtypes":[],
                "mana_cost":[],"color_indicator":[],"power_toughness":null,"loyalty":null,"defense":null},
            "abilities":sample()}]
    })).unwrap();
    let mapped = canonical::mapped_unified(&source).unwrap();
    assert_eq!(
        serde_json::to_value(&mapped.card).unwrap(),
        serde_json::to_value(canonical::unified(&source).unwrap()).unwrap()
    );
    assert!(canonical::MappedCard::from_json(&serde_json::to_string(&mapped).unwrap()).is_ok());
}

#[test]
fn token_ability_results_cannot_shadow_the_enclosing_hand_receipt() {
    let mut value = sample();
    let nested = json!({"kind":"Activated","args":{
        "origin":"Ordinary","cost":{"parts":["TapSymbol"]},
        "limits":{"once":false,"once_each_turn":false,"sorcery":false,"condition":null,"activators":null,"keyword":null,"modifiers":[]},
        "announcement":{"targets":[],"modes":null},
        "body":{"instructions":[effect(17,json!({"kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}}}))]}
    }});
    let token = json!({"kind":"CreateTokens","args":{
        "players":{"kind":"One","args":{"kind":"AbilityController"}},
        "tokens":[{"amount":{"kind":"Constant","args":1},"words":"CreatingEffect",
            "recipe":{"Defined":{"name":"FromSubtypes","colors":["White"],"chosen_color":false,"chosen_creature_type":false,
                "supertypes":[],"card_types":["Creature"],"subtypes":["Spirit"],
                "power_toughness":[{"kind":"Constant","args":1},{"kind":"Constant","args":1}],
                "abilities":[nested],"abilities_of":null}}}],
        "entry":{"controller":null,"tapped":false,"counters":[],"states":[]}
    }});
    value[0]["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .push(effect(40, token));
    assert!(valid(&value));
    canonical::abilities(&mut value).unwrap();
    assert!(valid(&value));
    assert_eq!(objects(&mut value)["kind"], "Matching");
    assert_eq!(
        objects(&mut value)["args"]["objects"]["args"]["objects"]["args"],
        0
    );
    let copied = value.clone();
    canonical::abilities(&mut value).unwrap();
    assert_eq!(value, copied);
}
