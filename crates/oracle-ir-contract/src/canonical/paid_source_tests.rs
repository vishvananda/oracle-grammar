use super::*;
use crate::{ir::*, validate};

fn source_cost() -> CostPart {
    CostPart::Sacrifice {
        objects: ObjectSet::One(ObjectRef::Source),
        choose: None,
    }
}
fn query() -> ObjectRef {
    ObjectRef::Query {
        objects: Box::new(ObjectSet::CostObjects {
            payment: PaymentRef::Activation,
            component: vec![0],
            view: CostObjectView::Inputs,
            phase: ObjectPhase::Before,
        }),
        index: 0,
    }
}
fn ability(cost: Vec<CostPart>, source: ObjectRef) -> Value {
    let ability = Ability::Activated {
        origin: ActivationOrigin::Ordinary,
        cost: Cost { parts: cost },
        limits: ActivationLimits::unrestricted(),
        announcement: Announcement::empty(),
        body: Program {
            instructions: vec![Instruction::Effect {
                result: ResultId(7),
                operation: Operation::DealDamage {
                    source,
                    recipients: Recipients::Players(PlayerSet::All),
                    amount: Expr::Constant(2),
                    preventable: true,
                },
            }],
        },
    };
    serde_json::to_value(ability).unwrap()
}
fn compared(value: Value) -> Value {
    let mut value = json!([value]);
    semantic_abilities(&mut value).unwrap();
    value
}
fn direct(cost: Vec<CostPart>) -> Value {
    compared(ability(cost, ObjectRef::Source))
}

#[test]
fn exact_source_input_components_compare_equally_but_keep_authored_print_operands() {
    let exile = CostPart::Move {
        selections: vec![CostObjectSelection {
            objects: ObjectSet::One(ObjectRef::Source),
            choose: None,
        }],
        destination: MovementDestination::Exile {
            visibility: ExileVisibility::FaceUp,
            arrival: ExileArrival::default(),
        },
    };
    for cost in [source_cost(), exile] {
        let value = ability(vec![cost.clone()], query());
        let typed: Ability = serde_json::from_value(value.clone()).unwrap();
        validate::validate_ability(&typed).unwrap();
        let expected = direct(vec![cost]);
        let actual = compared(value.clone());
        assert_eq!(actual, expected);
        let mut twice = actual.clone();
        semantic_abilities(&mut twice).unwrap();
        assert_eq!(actual, twice);
        let mut authored = json!([value]);
        reduced(&mut authored).unwrap();
        assert_eq!(
            authored[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["source"]["kind"],
            "Query"
        );
    }
}

#[test]
fn performed_after_indexed_nested_and_foreign_payments_remain_distinct() {
    let value = ability(vec![source_cost()], query());
    let expected = direct(vec![source_cost()]);
    let path = "/args/body/instructions/0/args/operation/args/source/args";
    for (field, operand) in [
        ("/objects/args/view", json!("Performed")),
        ("/objects/args/phase", json!("After")),
        ("/index", json!(1)),
        ("/objects/args/component", json!([1])),
        ("/objects/args/component", json!([0, 0])),
        ("/objects/args/payment", json!({"Resolution":9})),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(&(path.to_string() + field)).unwrap() = operand;
        assert_ne!(compared(changed), expected, "{field}");
    }
    for field in ["", "/args", "/args/objects", "/args/objects/args"] {
        let mut changed = value.clone();
        let source = changed
            .pointer_mut("/args/body/instructions/0/args/operation/args/source")
            .unwrap();
        let node = if field.is_empty() {
            source
        } else {
            source.pointer_mut(field).unwrap()
        };
        node["unknown"] = json!(true);
        assert_ne!(compared(changed), expected, "unknown {field}");
    }
}

#[test]
fn source_proof_respects_cost_operands_and_its_work_bound() {
    let selected = CostPart::Sacrifice {
        objects: ObjectSet::One(ObjectRef::Source),
        choose: Some(Count::Exactly(Expr::Constant(1))),
    };
    let foreign = CostPart::Sacrifice {
        objects: ObjectSet::One(ObjectRef::EventSubject),
        choose: None,
    };
    for cost in [selected, foreign, CostPart::TapSymbol] {
        assert_ne!(
            compared(ability(vec![cost.clone()], query())),
            direct(vec![cost])
        );
    }
    let mut too_many = vec![CostPart::TapSymbol; 65];
    too_many[0] = source_cost();
    assert_ne!(
        compared(ability(too_many.clone(), query())),
        direct(too_many)
    );
}

#[test]
fn descendant_abilities_cannot_use_their_grantors_activation_payment() {
    let inner = ability(vec![CostPart::TapSymbol], query());
    let mut outer = ability(vec![source_cost()], ObjectRef::Source);
    outer["args"]["body"]["instructions"][0]["args"]["operation"] = json!({"kind":"CreateEmblem","args":{
        "actor":{"kind":"One","args":{"kind":"AbilityController"}},"abilities":[inner]}});
    let actual = compared(outer);
    assert!(actual.to_string().contains("CostObjects"));
}

#[test]
fn source_input_identity_is_captured_in_delayed_programs_without_changing_the_event() {
    let mut value = ability(vec![source_cost()], query());
    let body = value["args"]["body"].clone();
    value["args"]["body"]["instructions"][0]["args"]["operation"] = json!({
        "kind":"CreateDelayedTrigger","args":{
            "event":{"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}},
            "start":"Immediately","expires":null,"intervening_if":null,"body":body}});
    let typed: Ability = serde_json::from_value(value.clone()).unwrap();
    validate::validate_ability(&typed).unwrap();
    let actual = compared(value);
    assert!(!actual.to_string().contains("CostObjects"));
    assert_eq!(
        actual[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["event"],
        json!({"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}})
    );
}

#[test]
fn mapped_source_proof_preserves_every_target_and_result_origin() {
    let packet: Value =
        serde_json::from_str(include_str!("../fixtures/extraction-190.json")).unwrap();
    let mut card = crate::migrate::read_json(&packet["records"][0]["card"].to_string()).unwrap();
    let mut value = ability(vec![source_cost()], query());
    value["args"]["announcement"]["targets"] = json!([{
        "slot":11,"kind":"DamageRecipient","count":{"Exactly":{"kind":"Constant","args":1}},
        "filter":{"kind":"Any"},"players":null,"distribute":null,"constraints":[],"chooser":null}]);
    value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["recipients"] =
        json!({"kind":"Target","args":{"slot":11,"index":0}});
    card.faces[0].abilities = vec![serde_json::from_value(value.clone()).unwrap()];
    let mapped = mapped_unified(&card).unwrap();
    value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["source"] =
        json!({"kind":"Source"});
    card.faces[0].abilities = vec![serde_json::from_value(value).unwrap()];
    let direct = mapped_unified(&card).unwrap();
    assert_eq!(
        serde_json::to_value(mapped.card).unwrap(),
        serde_json::to_value(direct.card).unwrap()
    );
    assert_eq!(
        serde_json::to_value(mapped.map).unwrap(),
        serde_json::to_value(direct.map).unwrap()
    );
}
