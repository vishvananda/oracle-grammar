//! Receipt scope and finite spending contracts, independent of a game engine.
use crate::{canonical, ir::*, migrate, validate};
use serde_json::{Value, json};

fn spell(instructions: Value) -> Value {
    json!({"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},
        "body":{"instructions":instructions}}})
}

fn valid(value: &Value) -> bool {
    let ability: Ability = serde_json::from_value(value.clone()).unwrap();
    validate::validate_ability(&ability).is_ok()
}

fn effect(result: u32, operation: Value) -> Value {
    json!({"kind":"Effect","args":{"result":result,"operation":operation}})
}

fn destroy() -> Value {
    effect(
        7,
        json!({"kind":"Destroy","args":{
        "objects":{"kind":"One","args":{"kind":"Source"}},"regeneration":false}}),
    )
}

fn constrained(from: Value, to: Value) -> Value {
    json!({"kind":"TransitionWhere","args":{"result":7,"phase":"Before",
        "from":from,"to":to}})
}

fn count(objects: Value) -> Value {
    effect(
        8,
        json!({"kind":"GainLife","args":{
        "players":{"kind":"One","args":{"kind":"AbilityController"}},
        "amount":{"kind":"CountObjects","args":objects}}}),
    )
}

#[test]
fn zone_constrained_receipts_require_a_producer_and_at_least_one_zone() {
    let receipt = constrained(json!("Battlefield"), json!("Graveyard"));
    assert!(valid(&spell(json!([destroy(), count(receipt.clone())]))));
    assert!(!valid(&spell(json!([count(receipt)]))));
    assert!(!valid(&spell(json!([
        destroy(),
        count(constrained(Value::Null, Value::Null))
    ]))));
    let wrong = effect(
        7,
        json!({"kind":"GainLife","args":{
        "players":{"kind":"All"},"amount":{"kind":"Constant","args":1}}}),
    );
    assert!(!valid(&spell(json!([
        wrong,
        count(constrained(json!("Battlefield"), Value::Null))
    ]))));
}

fn shield() -> Value {
    effect(
        7,
        json!({"kind":"CreateReplacement","args":{
        "event":{"Damage":{"source_kind":"Any","sources":{"kind":"Any"},
            "recipients":{"Players":{"kind":"All"}},"kind":"Any",
            "grouping":"SourceRecipient","subject":"Recipient"}},
        "limit":{"TotalDamage":{"kind":"Constant","args":3}},
        "duration":"EndOfTurn","body":{"instructions":[effect(8,json!({
            "kind":"ModifyEvent","args":{"change":"PreventDamage"}}))]}}}),
    )
}

fn delayed(result: u32) -> Value {
    effect(
        9,
        json!({"kind":"CreateDelayedTrigger","args":{
        "event":{"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}},
        "start":"Immediately","body":{"instructions":[effect(10,json!({
            "kind":"GainLife","args":{"players":{"kind":"All"},
                "amount":{"kind":"DamagePreventedBy","args":result}}}))]}}}),
    )
}

#[test]
fn cumulative_prevention_captures_only_an_available_instantiated_shield() {
    assert!(valid(&spell(json!([shield(), delayed(7)]))));
    assert!(
        !valid(&spell(json!([shield(), delayed(8)]))),
        "callback-local receipt must not escape"
    );
    assert!(!valid(&spell(json!([delayed(7), shield()]))));
    assert!(!valid(&spell(json!([destroy(), delayed(7)]))));
    let mut not_damage = shield();
    not_damage["args"]["operation"]["args"]["event"] =
        json!({"ProduceMana":{"sources":{"kind":"Any"},"by":null}});
    not_damage["args"]["operation"]["args"]["body"] = json!({"instructions":[]});
    assert!(valid(&spell(json!([not_damage.clone()]))));
    assert!(!valid(&spell(json!([not_damage, delayed(7)]))));
}

fn spending(uses: u32, component: ManaPaymentComponent, on: ManaPayment) -> Operand {
    Operand::Ability(Box::new(Ability::Static {
        kind: StaticKind::Ordinary,
        affected: EffectSubjects::Players(PlayerSet::One(PlayerRef::AbilityController)),
        changes: vec![Change::PlayerRule(PlayerRule::SpendManaAsLimited {
            spend_as: SpendAs::AnyType,
            on,
            component,
            uses,
        })],
    }))
}

#[test]
fn limited_spending_checks_uses_payment_scope_filters_and_both_codecs() {
    for component in [
        ManaPaymentComponent::ManaCost,
        ManaPaymentComponent::EntirePayment,
    ] {
        let value = spending(1, component, ManaPayment::Spells(ObjectFilter::Any));
        validate::operand(&value).unwrap();
        let json = serde_json::to_value(&value).unwrap();
        let decoded: Operand = serde_json::from_value(json.clone()).unwrap();
        validate::operand(&decoded).unwrap();
        let bytes = bincode::encode_to_vec(&value, bincode::config::standard()).unwrap();
        let (decoded, used): (Operand, usize) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(serde_json::to_value(decoded).unwrap(), json);
    }
    assert!(
        validate::operand(&spending(
            0,
            ManaPaymentComponent::ManaCost,
            ManaPayment::Spells(ObjectFilter::Any)
        ))
        .is_err()
    );
    for on in [
        ManaPayment::Anything,
        ManaPayment::Abilities(ObjectFilter::Any),
    ] {
        assert!(validate::operand(&spending(1, ManaPaymentComponent::ManaCost, on)).is_err());
    }
    let invalid = ObjectFilter::SameObject(ObjectRef::Bound(LoopBinding(999)));
    assert!(
        validate::operand(&spending(
            1,
            ManaPaymentComponent::ManaCost,
            ManaPayment::Spells(invalid)
        ))
        .is_err()
    );
}

#[test]
fn nested_matching_comparison_is_idempotent_and_retains_both_arrival_predicates() {
    let receipt = json!({"kind":"Transition","args":{"result":7,"phase":"After"}});
    let creature = json!({"kind":"CardType","args":"Creature"});
    let exile = json!({"kind":"InZone","args":{"Shared":"Exile"}});
    let matching = |objects: Value, filter: Value| json!({"kind":"Matching","args":{"objects":objects,"filter":filter}});
    let nested = spell(json!([
        destroy(),
        count(matching(
            matching(receipt.clone(), creature.clone()),
            exile.clone()
        ))
    ]));
    let flat = spell(json!([
        destroy(),
        count(matching(
            receipt,
            json!({"kind":"AllOf","args":[creature,exile]})
        ))
    ]));
    let mut left = json!([nested]);
    let mut right = json!([flat]);
    canonical::semantic_abilities(&mut left).unwrap();
    canonical::semantic_abilities(&mut right).unwrap();
    assert_eq!(left, right);
    let once = left.clone();
    canonical::semantic_abilities(&mut left).unwrap();
    assert_eq!(left, once);
    assert!(left.to_string().contains("Exile"));
}

#[test]
fn schema_192_migrates_unchanged_but_rejects_future_receipt_variants() {
    let packet: Value =
        serde_json::from_str(include_str!("../fixtures/extraction-190.json")).unwrap();
    let mut card = migrate::read_json(&packet["records"][0]["card"].to_string()).unwrap();
    card.schema_version = 192;
    for face in &mut card.faces {
        face.schema_version = 192;
    }
    let legacy = serde_json::to_string(&card).unwrap();
    let mapped = migrate::mapped_read_json(&legacy).unwrap();
    assert_eq!(mapped.source_version, 192);
    assert!(mapped.map.introduced.is_empty());
    assert_eq!(mapped.card.schema_version, 193);
    card.faces[0].abilities =
        vec![serde_json::from_value(spell(json!([shield(), delayed(7)]))).unwrap()];
    let error = migrate::read_json(&serde_json::to_string(&card).unwrap()).unwrap_err();
    assert!(error.contains("DamagePreventedBy"), "{error}");
}

#[test]
fn new_receipts_survive_both_codecs_and_count_as_observed_result_uses() {
    let transition = spell(json!([
        destroy(),
        count(constrained(json!("Battlefield"), json!("Graveyard")))
    ]));
    let prevention = spell(json!([shield(), delayed(7)]));
    for value in [transition, prevention] {
        let ability: Ability = serde_json::from_value(value).unwrap();
        validate::validate_ability(&ability).unwrap();
        let bytes = bincode::encode_to_vec(&ability, bincode::config::standard()).unwrap();
        let (decoded, used): (Ability, usize) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
        assert_eq!(used, bytes.len());
        validate::validate_ability(&decoded).unwrap();
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            serde_json::to_value(ability).unwrap()
        );
    }
    let instruction: Instruction =
        serde_json::from_value(count(constrained(json!("Battlefield"), json!("Graveyard"))))
            .unwrap();
    assert!(crate::result_reads::ResultReads::instructions(&[instruction], &ResultId(7)).objects);
}
