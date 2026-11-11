//! Implicit exile-card action operands retain every card from one input object.
//! Only grammar annotations authorize this operation; indexed reads stay scalar.
use crate::model::ir::{
    Cost, CostObjectView, CostPart, MovementDestination, ObjectPhase, ObjectRef, ObjectSet,
    Operation, PaymentRef,
};
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

const MARKER: &str = "exile_followup";

/// Action-only implicit card, holder and ordinary pronoun spellings retain
/// their lexical provenance. Explicit indexed/selected queries stay scalar;
/// the bound operand still needs the exact earlier exile receipt proof.
pub(crate) fn lexical(objects: &Value, at: usize) -> Value {
    let reference = &objects["args"];
    // The post-move grant grammar guards its return pronoun with return_ref.
    // Inspect that one known annotation without changing the bound operand.
    let reference = if super::symbol_name(reference) == Some("return_ref") {
        &reference["args"][0]
    } else {
        reference
    };
    let ordinary_it = super::symbol_name(reference) == Some("member_count_it")
        && super::symbol_name(&reference["args"][0]) == Some("it");
    if objects["kind"] == "One"
        && (matches!(
            super::symbol_name(reference),
            Some("current_named_object" | "self" | "it" | "current_it")
        ) || ordinary_it)
    {
        json!({"$sym":MARKER,"args":[objects],"at":at})
    } else {
        objects.clone()
    }
}

#[derive(Clone, Default)]
struct Context {
    moves: BTreeSet<u32>,
    action_moves: BTreeSet<(u32, u32)>,
    payments: BTreeSet<(u32, Vec<u32>)>,
    paid: BTreeSet<u32>,
    activation: BTreeSet<Vec<u32>>,
    single_targets: Arc<BTreeSet<u16>>,
}

impl Context {
    fn record(&mut self, effect: &Value) -> Result<(), String> {
        if effect["kind"] != "Effect" {
            return Ok(());
        }
        let Some(result) = effect["args"]["result"]
            .as_u64()
            .and_then(|id| u32::try_from(id).ok())
        else {
            return Ok(());
        };
        let value = &effect["args"]["operation"];
        if !matches!(value["kind"].as_str(), Some("Move" | "Counter" | "PayCost")) {
            return Ok(());
        }
        crate::functions::branch_bounds(value)?;
        let Ok(operation) = serde_json::from_value::<Operation>(value.clone()) else {
            return Ok(());
        };
        match operation {
            Operation::Move {
                objects: ObjectSet::One(_),
                destination: MovementDestination::Exile { .. },
                ..
            }
            | Operation::Counter {
                objects: ObjectSet::One(_),
                destination: MovementDestination::Exile { .. },
                destination_when: None,
                ..
            } => {
                self.moves.insert(result);
            }
            Operation::Move {
                objects: ObjectSet::Targets(slot),
                destination: MovementDestination::Exile { .. },
                ..
            } if self.single_targets.contains(&slot.0) => {
                self.moves.insert(result);
            }
            Operation::PayCost { cost, .. } => {
                for mut part in &cost.parts {
                    while let CostPart::Constrained { part: inner, .. } = part {
                        part = inner;
                    }
                    if let CostPart::PlayerAction { body, .. } = part
                        && let [
                            crate::model::ir::Instruction::Effect {
                                result: moved,
                                operation:
                                    Operation::Move {
                                        objects: ObjectSet::One(_),
                                        destination: MovementDestination::Exile { .. },
                                        ..
                                    },
                            },
                        ] = body.instructions.as_slice()
                    {
                        self.action_moves.insert((moved.0, result));
                    }
                }
                for (_, component) in crate::model::cost_history::components(&cost) {
                    let Some(part) = crate::model::cost_history::component(&cost, &component)
                    else {
                        continue;
                    };
                    if matches!(
                        part,
                        CostPart::Move {
                            destination: MovementDestination::Exile { .. },
                            ..
                        }
                    ) && crate::model::cost_history::singular(part)
                    {
                        self.payments.insert((result, component));
                    }
                }
            }
            _ => {}
        }
        if self.moves.len() + self.payments.len() + self.action_moves.len() > 4096 {
            return Err("exile followup receipt limit".into());
        }
        Ok(())
    }

    fn own_activation(value: &Value) -> Result<Self, String> {
        let mut context = Self {
            single_targets: Arc::new(super::optional_exile_target::slots(value)?),
            ..Self::default()
        };
        if value["kind"] != "Activated" {
            return Ok(context);
        }
        let cost = &value["args"]["cost"];
        crate::functions::branch_bounds(cost)?;
        let Ok(cost) = serde_json::from_value::<Cost>(cost.clone()) else {
            return Ok(context);
        };
        for (_, component) in crate::model::cost_history::components(&cost) {
            if let Some(part) = crate::model::cost_history::component(&cost, &component)
                && matches!(
                    part,
                    CostPart::Move {
                        destination: MovementDestination::Exile { .. },
                        ..
                    }
                )
                && crate::model::cost_history::singular(part)
            {
                context.activation.insert(component);
            }
        }
        if context.activation.len() > 4096 {
            return Err("exile followup receipt limit".into());
        }
        Ok(context)
    }

    fn group(&self, value: &Value) -> Option<Value> {
        let objects = if value["kind"] == "InZone" && value["args"]["zone"] == "Exile" {
            &value["args"]["objects"]
        } else {
            value
        };
        crate::functions::branch_bounds(objects).ok()?;
        let ObjectSet::One(reference) = serde_json::from_value(objects.clone()).ok()? else {
            return None;
        };
        let group = match reference {
            ObjectRef::Result {
                result,
                phase: ObjectPhase::After,
                index: 0,
            } if self.moves.contains(&result.0)
                || self
                    .action_moves
                    .iter()
                    .filter(|(moved, payment)| *moved == result.0 && self.paid.contains(payment))
                    .count()
                    == 1 =>
            {
                ObjectSet::Transition {
                    result,
                    phase: ObjectPhase::After,
                }
            }
            ObjectRef::Query { objects, index: 0 } => {
                let ObjectSet::CostObjects {
                    ref payment,
                    ref component,
                    view: CostObjectView::Performed,
                    phase: ObjectPhase::After,
                } = *objects
                else {
                    return None;
                };
                let proved = match payment {
                    PaymentRef::Activation => self.activation.contains(component),
                    PaymentRef::Resolution(result) => {
                        self.paid.contains(&result.0)
                            && self.payments.contains(&(result.0, component.clone()))
                    }
                    _ => false,
                };
                if !proved {
                    return None;
                }
                *objects
            }
            _ => return None,
        };
        Some(json!({"kind":"InZone","args":{"objects":group,"zone":"Exile"}}))
    }
}

/// Run after binding/payment resolution, before schema-based selection lowering.
/// Captures use native result IDs, never token offsets or target slots.
pub(super) fn lower(value: &mut Value) -> Result<(), String> {
    // Unrelated values retain their original path and resource behavior.
    if !has_marker(value) {
        return Ok(());
    }
    visit(value, &Context::default(), 0, &mut 131_072)
}

fn has_marker(value: &Value) -> bool {
    let mut pending = vec![value];
    let mut work = 131_072;
    while let Some(value) = pending.pop() {
        if super::symbol_name(value) == Some(MARKER) {
            return true;
        }
        if work == 0 {
            // The existing semantic-input bounds own rejection of larger IR.
            return false;
        }
        work -= 1;
        match value {
            Value::Object(fields) if pending.len() + fields.len() <= 131_072 => {
                pending.extend(fields.values())
            }
            Value::Array(items) if pending.len() + items.len() <= 131_072 => pending.extend(items),
            Value::Object(_) | Value::Array(_) => return false,
            _ => {}
        }
        if pending.len() > 131_072 {
            return false;
        }
    }
    false
}

fn visit(
    value: &mut Value,
    context: &Context,
    depth: usize,
    work: &mut usize,
) -> Result<(), String> {
    if depth > 128 || *work == 0 {
        return Err("exile followup work/depth limit".into());
    }
    *work -= 1;
    if super::symbol_name(value) == Some(MARKER) {
        let mut objects = value["args"]
            .get_mut(0)
            .ok_or("exile followup needs one ObjectSet")?
            .take();
        visit(&mut objects, context, depth + 1, work)?;
        *value = context.group(&objects).unwrap_or(objects);
        return Ok(());
    }
    let fresh;
    let context = if matches!(
        crate::traverse::boundary(value),
        crate::traverse::Boundary::Ability
            | crate::traverse::Boundary::Mode
            | crate::traverse::Boundary::Holder
            | crate::traverse::Boundary::Lexical
    ) || matches!(
        value["kind"].as_str(),
        Some("CreateTrigger" | "CreateReplacement")
    ) {
        fresh = Context::own_activation(value)?;
        &fresh
    } else {
        context
    };
    if value["kind"] == "If" {
        visit(&mut value["args"]["condition"], context, depth + 1, work)?;
        let mut positive = context.clone();
        if value["args"]["condition"]["kind"] == "PaymentChosen"
            && let Some(id) = value["args"]["condition"]["args"]
                .as_u64()
                .and_then(|id| u32::try_from(id).ok())
        {
            positive.paid.insert(id);
        }
        visit(&mut value["args"]["then"], &positive, depth + 1, work)?;
        visit(&mut value["args"]["otherwise"], context, depth + 1, work)?;
        return Ok(());
    }
    if let Some(instructions) = value.get_mut("instructions").and_then(Value::as_array_mut) {
        let mut local = context.clone();
        for instruction in instructions {
            visit(instruction, &local, depth + 1, work)?;
            local.record(instruction)?;
        }
        return Ok(());
    }
    match value {
        Value::Object(fields) => {
            for child in fields.values_mut() {
                visit(child, context, depth + 1, work)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                visit(child, context, depth + 1, work)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn scalar(value: &Value) -> Option<Value> {
    crate::functions::branch_bounds(value).ok()?;
    let group: ObjectSet = serde_json::from_value(value.clone()).ok()?;
    let reference = match group {
        ObjectSet::Transition {
            result,
            phase: ObjectPhase::After,
        } => ObjectRef::Result {
            result,
            phase: ObjectPhase::After,
            index: 0,
        },
        objects @ ObjectSet::CostObjects {
            view: CostObjectView::Performed,
            phase: ObjectPhase::After,
            ..
        } => ObjectRef::Query {
            objects: Box::new(objects),
            index: 0,
        },
        ObjectSet::InZone { objects, zone } => {
            return scalar(&serde_json::to_value(objects).ok()?)
                .map(|objects| json!({"kind":"InZone","args":{"objects":objects,"zone":zone}}));
        }
        _ => return None,
    };
    Some(json!({"kind":"One","args":reference}))
}

/// Offer only projections of the current operands. The forward proof and the
/// complete semantic reparse decide whether the implicit spelling is valid.
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [argument] = args else {
        return Vec::new();
    };
    if crate::functions::branch_bounds(value).is_err() {
        return Vec::new();
    }
    let mut out = argument.unify(value, bindings.clone());
    if let Some(projected) = scalar(value) {
        out.extend(argument.unify_made(&projected, &bindings));
        if projected["kind"] == "InZone" && projected["args"]["zone"] == "Exile" {
            out.extend(argument.unify_made(&projected["args"]["objects"], &bindings));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ir_schema::Ty;

    fn one(result: u32, index: u32) -> Value {
        json!({"kind":"One","args":{"kind":"Result","args":{
            "result":result,"phase":"After","index":index}}})
    }
    fn marked(objects: Value) -> Value {
        json!({"$sym":MARKER,"args":[objects],"at":99})
    }
    fn movement(result: u32, objects: Value, destination: Value) -> Value {
        json!({"kind":"Effect","args":{"result":result,"operation":{"kind":"Move","args":{
            "objects":objects,"destination":super::super::movement::test_destination(destination),"qualifiers":[]}}}})
    }
    fn producer(result: u32) -> Value {
        movement(
            result,
            json!({"kind":"One","args":{"kind":"Source"}}),
            json!("Exile"),
        )
    }
    fn returned(program: &Value) -> &Value {
        &program["instructions"][1]["args"]["operation"]["args"]["objects"]
    }
    fn group(result: u32) -> Value {
        json!({"kind":"InZone","args":{"objects":{"kind":"Transition","args":{
            "result":result,"phase":"After"}},"zone":"Exile"}})
    }

    #[test]
    fn annotated_single_input_receipt_keeps_all_cards_and_the_exile_guard() {
        for objects in [
            one(7, 0),
            json!({"kind":"InZone","args":{"objects":one(7,0),"zone":"Exile"}}),
        ] {
            let mut program = json!({"instructions":[producer(7),movement(8,marked(objects),json!("OwnersBattlefield"))]});
            lower(&mut program).unwrap();
            assert_eq!(returned(&program), &group(7));
        }
    }

    #[test]
    fn unannotated_indexed_other_result_plural_input_and_other_zone_stay_scalar() {
        let mut literal =
            json!({"instructions":[producer(7),movement(8,one(7,0),json!("OwnersBattlefield"))]});
        let old = literal.clone();
        lower(&mut literal).unwrap();
        assert_eq!(literal, old);
        for (mut source, objects) in [
            (producer(7), one(7, 1)),
            (producer(7), one(6, 0)),
            (
                producer(7),
                json!({"kind":"InZone","args":{"objects":one(7,0),"zone":"Graveyard"}}),
            ),
        ] {
            let mut program = json!({"instructions":[source.take(),movement(8,marked(objects.clone()),json!("OwnersBattlefield"))]});
            lower(&mut program).unwrap();
            assert_eq!(returned(&program), &objects);
        }
        for destination in ["Exile", "OwnersHand"] {
            let mut source = producer(7);
            source["args"]["operation"]["args"]["destination"] =
                super::super::movement::test_destination(json!(destination));
            source["args"]["operation"]["args"]["objects"] = json!({"kind":"Targets","args":0});
            let mut program = json!({"instructions":[source,movement(8,marked(one(7,0)),json!("OwnersBattlefield"))]});
            lower(&mut program).unwrap();
            assert_eq!(returned(&program), &one(7, 0));
        }
    }

    #[test]
    fn proofs_do_not_escape_branches_or_independent_events_but_delayed_capture_keeps_them() {
        let followup = movement(8, marked(one(7, 0)), json!("OwnersBattlefield"));
        let mut branch = json!({"instructions":[{"kind":"Optional","args":{"decider":{"kind":"AbilityController"},"body":{"instructions":[producer(7)]}}},followup]});
        lower(&mut branch).unwrap();
        assert_eq!(returned(&branch), &one(7, 0));
        for (kind, expected) in [
            ("CreateDelayedTrigger", group(7)),
            ("CreateTrigger", one(7, 0)),
            ("ReflexiveTrigger", group(7)),
        ] {
            let mut program = json!({"instructions":[producer(7),{"kind":"Effect","args":{"result":9,"operation":{"kind":kind,"args":{"body":{"instructions":[movement(8,marked(one(7,0)),json!("OwnersBattlefield"))]}}}}}]});
            lower(&mut program).unwrap();
            assert_eq!(
                program["instructions"][1]["args"]["operation"]["args"]["body"]["instructions"][0]
                    ["args"]["operation"]["args"]["objects"],
                expected,
                "{kind}"
            );
        }
    }

    #[test]
    fn paid_component_requires_its_actual_identity_and_positive_guard() {
        let receipt = json!({"kind":"CostObjects","args":{"payment":{"Resolution":17},"component":[0],"view":"Performed","phase":"After"}});
        let scalar =
            json!({"kind":"One","args":{"kind":"Query","args":{"objects":receipt,"index":0}}});
        let move_part = producer(17)["args"]["operation"]["args"].clone();
        let payment = json!({"kind":"Effect","args":{"result":17,"operation":{"kind":"PayCost","args":{"player":{"kind":"AbilityController"},"optional":true,"cost":{"parts":[{"Move":{"selections":[{"objects":move_part["objects"],"choose":null}],"destination":{"Exile":{"visibility":"FaceUp","arrival":move_part["destination"]["Exile"]["arrival"]}}}}]}}}}});
        for id in [17, 18] {
            let mut program = json!({"instructions":[payment,{"kind":"If","args":{"condition":{"kind":"PaymentChosen","args":id},"then":{"instructions":[movement(18,marked(scalar.clone()),json!("OwnersBattlefield"))]},"otherwise":{"instructions":[movement(19,marked(scalar.clone()),json!("OwnersBattlefield"))]}}}]});
            lower(&mut program).unwrap();
            let expected = if id == 17 {
                json!({"kind":"InZone","args":{"objects":receipt,"zone":"Exile"}})
            } else {
                scalar.clone()
            };
            assert_eq!(
                program["instructions"][1]["args"]["then"]["instructions"][0]["args"]["operation"]
                    ["args"]["objects"],
                expected
            );
            assert_eq!(
                program["instructions"][1]["args"]["otherwise"]["instructions"][0]["args"]["operation"]
                    ["args"]["objects"],
                scalar
            );
        }
    }

    #[test]
    fn inverse_projection_uses_current_result_and_lexical_marker_is_narrow() {
        let pattern = [Pattern::Hole(
            "objects".into(),
            Ty::Named("ObjectSet".into()),
        )];
        let value = group(41);
        let proposals = unify(&pattern, &value, Bindings::new());
        assert!(
            proposals
                .iter()
                .any(|b| b.get("objects") == Some(&one(41, 0)))
        );
        for source in ["Source", "GrantingObject", "EventSubject"] {
            let objects = json!({"kind":"One","args":{"kind":source}});
            assert_eq!(lexical(&objects, 5), objects);
        }
        let card = json!({"kind":"One","args":{"$sym":"current_named_object","args":[],"at":5}});
        assert_eq!(lexical(&card, 5)["$sym"], MARKER);
    }
}

#[cfg(test)]
mod phase_tests {
    use super::*;
    use crate::bind::{exile_returns, objects};

    fn one(result: u32) -> Value {
        json!({"kind":"One","args":{"kind":"Result","args":{"result":result,"phase":"After","index":0}}})
    }
    fn marker(objects: Value) -> Value {
        json!({"$sym":MARKER,"args":[objects],"at":10})
    }
    fn movement(result: u32, objects: Value, destination: Value) -> Value {
        json!({"kind":"Effect","args":{"result":result,"operation":{"kind":"Move","args":{
            "objects":objects,"destination":super::super::movement::test_destination(destination),"qualifiers":[]}}}})
    }
    fn source() -> Value {
        json!({"kind":"One","args":{"kind":"Source"}})
    }
    fn objects_at(value: &Value, index: usize) -> &Value {
        &value["instructions"][index]["args"]["operation"]["args"]["objects"]
    }

    #[test]
    fn old_guard_shape_and_final_receipt_phases_preserve_single_and_plural_contracts() {
        for single in [true, false] {
            let input = if single {
                source()
            } else {
                json!({"kind":"Targets","args":0})
            };
            let mut program = json!({"instructions":[movement(7,input,json!("Exile")),movement(8,
                json!({"$sym":"zone_members","args":[marker(one(7))]}),json!("OwnersBattlefield"))]});
            exile_returns::guard(&mut program);
            objects::resolve_shapes(&mut program).unwrap();
            assert_eq!(objects_at(&program, 1)["$sym"], MARKER);
            assert_eq!(objects_at(&program, 1)["args"][0]["kind"], "InZone");
            lower(&mut program).unwrap();
            let returned = objects_at(&program, 1);
            assert_eq!(returned["args"]["zone"], "Exile");
            assert_eq!(
                returned["args"]["objects"]["kind"],
                if single { "Transition" } else { "One" }
            );
        }
    }

    #[test]
    fn full_binding_tracks_a_return_through_metadata_into_the_following_action() {
        let future = json!({"kind":"One","args":{"$sym":"current_named_object","args":[],"at":30}});
        let mut ability = json!({"kind":"Spell","args":{"announcement":{"$sym":"announcement","args":[],"at":0},"body":{"instructions":[
            movement(0,source(),json!("Exile")),
            movement(1,json!({"$sym":"zone_members","args":[marker(one(0))]}),json!("OwnersBattlefield")),
            movement(2,future,json!("OwnersHand"))
        ]}}});
        crate::bind::bind_ability(&mut ability, "Receipt Fixture").unwrap();
        assert_eq!(
            ability["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["objects"],
            one(1)
        );
        assert_eq!(
            ability["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["objects"]["args"]
                ["objects"]["kind"],
            "Transition"
        );
    }

    #[test]
    fn activation_receipt_belongs_to_its_cost_and_survives_only_lexical_delayed_capture() {
        let cost = json!({"parts":[{"Move":{"selections":[{"objects":source(),"choose":null}],"destination":{"Exile":{"visibility":"FaceUp","arrival":{"counters":[],"states":[]}}}}}]});
        let receipt = json!({"kind":"CostObjects","args":{"payment":"Activation","component":[0],"view":"Performed","phase":"After"}});
        let scalar =
            json!({"kind":"One","args":{"kind":"Query","args":{"objects":receipt,"index":0}}});
        for (kind, succeeds) in [
            ("CreateDelayedTrigger", true),
            ("CreateTrigger", false),
            ("Activated", false),
        ] {
            let mut ability = json!({"kind":"Activated","args":{"cost":cost,"body":{"instructions":[{"kind":"Effect","args":{"result":0,"operation":{"kind":kind,"args":{"cost":{"parts":[]},"body":{"instructions":[movement(1,marker(scalar.clone()),json!("OwnersLibraryShuffled"))]}}}}}]}}});
            lower(&mut ability).unwrap();
            let actual = &ability["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["body"]
                ["instructions"][0]["args"]["operation"]["args"]["objects"];
            assert_eq!(
                *actual,
                if succeeds {
                    json!({"kind":"InZone","args":{"objects":receipt,"zone":"Exile"}})
                } else {
                    scalar.clone()
                },
                "{kind}"
            );
        }
    }

    #[test]
    fn only_ordinary_action_it_is_annotated_and_unrelated_ir_keeps_its_resource_behavior() {
        let it = json!({"$sym":"member_count_it","args":[{"$sym":"it","args":[],"at":2}],"at":2});
        assert_eq!(lexical(&json!({"kind":"One","args":it}), 2)["$sym"], MARKER);
        let itself =
            json!({"kind":"One","args":{"$sym":"literal_damage_itself","args":[it],"at":2}});
        assert_eq!(lexical(&itself, 2), itself);
        let mut unrelated = Value::Null;
        for _ in 0..140 {
            unrelated = json!([unrelated]);
        }
        assert!(lower(&mut unrelated).is_ok());
        let pattern = [Pattern::Hole(
            "objects".into(),
            crate::model::ir_schema::Ty::Named("ObjectSet".into()),
        )];
        assert!(unify(&pattern, &unrelated, Bindings::new()).is_empty());
    }

    fn assert_complete_group_actions(value: &Value) {
        if value["kind"] == "Move" && value["args"]["destination"].get("Battlefield").is_some()
            || value["kind"] == "CreatePerpetualEffect"
        {
            let objects = &value["args"]["objects"];
            assert_eq!(objects["kind"], "InZone", "{value}");
            assert_eq!(objects["args"]["zone"], "Exile");
            assert!(
                matches!(
                    objects["args"]["objects"]["kind"].as_str(),
                    Some("Transition" | "CostObjects")
                ),
                "{value}"
            );
        }
        match value {
            Value::Object(fields) => fields.values().for_each(assert_complete_group_actions),
            Value::Array(items) => items.iter().for_each(assert_complete_group_actions),
            _ => {}
        }
    }

    #[test]
    fn four_exact_export_native_frames_are_supported_without_scalar_return_readings() {
        use crate::engine::semantic_abilities;
        let engine = crate::test_support::engine();
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/exile-followup-native.json"
        ))
        .unwrap();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let expected = semantic_abilities(&case["native"]).unwrap();
            let actual = engine.parse(name, case["text"].as_str().unwrap()).unwrap();
            assert!(
                actual.readings.contains(&expected),
                "{name}: actual native frame missing"
            );
            for reading in &actual.readings {
                assert_complete_group_actions(reading);
            }
        }
    }

    #[test]
    fn implicit_self_and_plain_it_aliases_require_a_bound_exile_receipt() {
        for name in ["self", "it", "current_it"] {
            let action = json!({"kind":"One","args":{"$sym":name,"args":[],"at":2}});
            assert_eq!(lexical(&action, 2)["$sym"], MARKER, "{name}");
        }
        for reference in [
            json!({"kind":"Source"}),
            json!({"kind":"GrantingObject"}),
            json!({"kind":"Result","args":{"result":0,"phase":"After","index":0}}),
            json!({"kind":"Query","args":{"objects":{"kind":"Selected","args":0},"index":0}}),
        ] {
            let objects = json!({"kind":"One","args":reference});
            assert_eq!(lexical(&objects, 2), objects);
        }
        for role in ["Source", "GrantingObject"] {
            let objects = json!({"kind":"One","args":{"kind":role}});
            let property = json!({"kind":"ObjectProperty","args":{"object":{"kind":role},"property":"Power","information":"CurrentOrLastKnown"}});
            let mut program = json!({"instructions":[movement(1,marker(objects.clone()),json!("OwnersBattlefield"))],"property":property});
            lower(&mut program).unwrap();
            assert_eq!(objects_at(&program, 0), &objects);
            assert_eq!(program["property"], property);
        }
    }

    #[test]
    fn saltskitter_and_sundering_keep_source_correct_frames_and_preserve_properties() {
        use crate::engine::semantic_abilities;
        let engine = crate::test_support::engine();
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/exile-followup-aliases-native.json"
        ))
        .unwrap();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let expected =
                semantic_abilities(case.get("source_expected").unwrap_or(&case["native"])).unwrap();
            let actual = engine.parse(name, case["text"].as_str().unwrap()).unwrap();
            assert_eq!(
                actual.readings,
                vec![expected],
                "{name}: complete source-correct frame and no old scalar reading"
            );
        }
    }

    #[test]
    fn post_move_return_pronoun_keeps_its_exact_guarded_constructor() {
        let it = json!({"$sym":"member_count_it","args":[{"$sym":"it","args":[],"at":4}],"at":4});
        let objects = json!({"kind":"One","args":{"$sym":"return_ref","args":[it],"at":4}});
        let annotated = lexical(&objects, 4);
        assert_eq!(annotated["$sym"], MARKER);
        assert_eq!(annotated["args"][0], objects);
        for reference in [
            json!({"kind":"Source"}),
            json!({"kind":"GrantingObject"}),
            json!({"kind":"Target","args":{"slot":0,"index":0}}),
            json!({"kind":"Result","args":{"result":0,"phase":"After","index":0}}),
        ] {
            let explicit =
                json!({"kind":"One","args":{"$sym":"return_ref","args":[reference],"at":4}});
            assert_eq!(lexical(&explicit, 4), explicit);
        }
    }

    #[test]
    fn granted_death_return_defaults_preserve_holder_and_named_granter_from_expected_ir() {
        use crate::engine::canonical_abilities;
        let engine = crate::test_support::engine();
        let family: Value =
            serde_json::from_str(include_str!("../../families/event-card-views.json")).unwrap();
        for index in [8, 9] {
            let case = &family["cases"][index];
            let name = case["name"].as_str().unwrap();
            let expected = canonical_abilities(&case["expected"]).unwrap();
            let types: Vec<String> = serde_json::from_value(case["types"].clone()).unwrap();
            let printed = engine
                .print_keyed(name, &expected, &types, &[])
                .unwrap()
                .unwrap_or_else(|| {
                    panic!("Viewforge case{index}: no verified default from expected IR")
                });
            assert!(
                engine
                    .parse(name, &printed)
                    .unwrap()
                    .readings
                    .contains(&expected),
                "{index}: {printed}"
            );
        }
    }
}
