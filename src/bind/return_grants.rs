//! An implicit keyword grant follows every actual entrant of a proven return.
//! Predicate/property/indexed reads and unrelated Source/Granter roles stay scalar.
use crate::model::ir::{
    EventZone, MovementDestination, ObjectPhase, ObjectRef, ObjectSet, Operation,
};
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const MARKER: &str = "return_grant_set";

/// Preserve word provenance only for bare it and the current that-card spelling.
fn implicit(objects: &Value) -> bool {
    let reference = &objects["args"];
    let ordinary=reference["$sym"]=="member_count_it"
        && reference["args"].as_array().is_some_and(|args|matches!(args.as_slice(),[inner] if inner["$sym"]=="it" && inner["args"]==json!([])));
    objects["kind"] == "One"
        && (ordinary
            || reference["$sym"] == "current_named_object" && reference["args"] == json!([]))
}

/// Only keyword snapshots enter the new contract. Other continuous effects
/// retain their exact old constructor and do not acquire binding metadata.
pub(crate) fn annotate(operation: &Value, at: usize, lexical: bool) -> Result<Value, String> {
    if !keyword_grant(operation) || lexical && !implicit(&operation["args"]["affected"]["Objects"])
    {
        return Ok(operation.clone());
    }
    crate::functions::branch_bounds(operation)?;
    let mut marked = operation.clone();
    let objects = &mut marked["args"]["affected"]["Objects"];
    *objects = json!({"$sym":MARKER,"args":[objects.take()],"at":at});
    Ok(marked)
}

#[derive(Clone, Default)]
pub(super) struct Context {
    exiled: BTreeSet<u32>,
    returned: BTreeSet<u32>,
    originals: Arc<BTreeMap<u32, Arc<Value>>>,
    latest_return: Option<(u32, Arc<Value>)>,
}

impl Context {
    pub(super) fn record(&mut self, instruction: &Value) -> Result<(), String> {
        if instruction["kind"] != "Effect" {
            return Ok(());
        }
        let Some(result) = instruction["args"]["result"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
        else {
            return Ok(());
        };
        // A new definition never inherits an old definition's proof by number.
        self.exiled.remove(&result);
        self.returned.remove(&result);
        if self.originals.contains_key(&result) {
            Arc::make_mut(&mut self.originals).remove(&result);
        }
        if self
            .latest_return
            .as_ref()
            .is_some_and(|(r, _)| *r == result)
        {
            self.latest_return = None;
        }
        let operation = &instruction["args"]["operation"];
        if !matches!(operation["kind"].as_str(), Some("Move" | "Counter")) {
            return Ok(());
        }
        self.latest_return = None;
        crate::functions::branch_bounds(operation)?;
        let original = operation["args"]["objects"]["args"].clone();
        let Ok(operation) = serde_json::from_value::<Operation>(operation.clone()) else {
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
                self.exiled.insert(result);
                Arc::make_mut(&mut self.originals).insert(result, Arc::new(original));
            }
            Operation::Move {
                objects:
                    ObjectSet::InZone {
                        objects,
                        zone: EventZone::Exile,
                    },
                destination: MovementDestination::Battlefield { .. },
                ..
            } => {
                if let ObjectSet::Transition {
                    result: origin,
                    phase: ObjectPhase::After,
                } = *objects
                    && self.exiled.contains(&origin.0)
                {
                    self.returned.insert(result);
                    self.latest_return =
                        self.originals.get(&origin.0).cloned().map(|o| (result, o));
                }
            }
            _ => {}
        }
        if self.exiled.len() + self.returned.len() > 4096 {
            return Err("return grant receipt limit".into());
        }
        Ok(())
    }

    /// The nearest actual return has the same exact single-input proof as grants.
    pub(super) fn current_return(&self) -> Option<(u32, Arc<Value>)> {
        self.latest_return.clone()
    }

    pub(super) fn scalar_branch(&self) -> Self {
        let mut child = self.clone();
        child.returned.clear();
        child.latest_return = None;
        child
    }

    fn group(&self, objects: &Value) -> Option<Value> {
        crate::functions::branch_bounds(objects).ok()?;
        let ObjectSet::One(ObjectRef::Result {
            result,
            phase: ObjectPhase::After,
            index: 0,
        }) = serde_json::from_value(objects.clone()).ok()?
        else {
            return None;
        };
        self.returned
            .contains(&result.0)
            .then(|| json!({"kind":"Transition","args":{"result":result,"phase":"After"}}))
    }
}

fn keyword_grant(value: &Value) -> bool {
    value["kind"] == "CreateContinuousEffect"
        && value["args"]["evaluation"] == "Snapshot"
        && value["args"]["changes"].as_array().is_some_and(|changes| {
            !changes.is_empty()
                && changes.len() <= 4096
                && changes.iter().all(|c| c["kind"] == "GrantKeyword")
        })
}

fn has_marker(value: &Value) -> bool {
    let mut pending = vec![value];
    let mut work = 131_072;
    while let Some(value) = pending.pop() {
        if super::symbol_name(value) == Some(MARKER) {
            return true;
        }
        if work == 0 {
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
    }
    false
}

/// Run after exile-followup binding, so return inputs carry their complete
/// guarded exile receipt. Unannotated values retain their old resource path.
pub(super) fn lower(value: &mut Value) -> Result<(), String> {
    if !has_marker(value) {
        return Ok(());
    }
    visit(value, &Context::default(), 0, &mut 131_072)
}

fn visit(
    value: &mut Value,
    context: &Context,
    depth: usize,
    work: &mut usize,
) -> Result<(), String> {
    if depth > 128 || *work == 0 {
        return Err("return grant work/depth limit".into());
    }
    *work -= 1;
    if super::symbol_name(value) == Some(MARKER) {
        let [inner] = value["args"]
            .as_array_mut()
            .ok_or("return grant arguments")?
            .as_mut_slice()
        else {
            return Err("return grant arity".into());
        };
        let mut inner = inner.take();
        visit(&mut inner, context, depth + 1, work)?;
        *value = inner;
        return Ok(());
    }
    let fresh;
    let context = if matches!(
        crate::traverse::boundary(value),
        crate::traverse::Boundary::Ability
            | crate::traverse::Boundary::Mode
            | crate::traverse::Boundary::Holder
            | crate::traverse::Boundary::Lexical
            | crate::traverse::Boundary::Reflexive
    ) || matches!(
        value["kind"].as_str(),
        Some("CreateTrigger" | "CreateReplacement" | "ReflexiveTrigger")
    ) {
        fresh = Context::default();
        &fresh
    } else {
        context
    };
    if keyword_grant(value) {
        let objects = &mut value["args"]["affected"]["Objects"];
        if super::symbol_name(objects) == Some(MARKER) {
            let [inner] = objects["args"]
                .as_array_mut()
                .ok_or("return grant arguments")?
                .as_mut_slice()
            else {
                return Err("return grant arity".into());
            };
            let inner = inner.take();
            *objects = context.group(&inner).unwrap_or(inner);
        }
    }
    // A predicate outside the return cannot classify its components through
    // one scalar. Conditional per-component grants require a separate contract.
    if matches!(
        value["kind"].as_str(),
        Some(
            "If" | "Optional"
                | "ChooseOne"
                | "ForEachObject"
                | "ForEachPlayer"
                | "RepeatableWithLimit"
        )
    ) {
        let mut branch = context.clone();
        branch.returned.clear();
        branch.latest_return = None;
        match value {
            Value::Object(fields) => {
                for child in fields.values_mut() {
                    visit(child, &branch, depth + 1, work)?;
                }
            }
            Value::Array(items) => {
                for child in items {
                    visit(child, &branch, depth + 1, work)?;
                }
            }
            _ => {}
        }
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

/// Only the current return-shaped set can propose its own scalar spelling.
/// The complete forward receipt proof and semantic reparse remain mandatory.
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [argument] = args else {
        return Vec::new();
    };
    let mut found = argument.unify(value, bindings.clone());
    if !keyword_grant(value) || crate::functions::branch_bounds(value).is_err() {
        return found;
    }
    let objects = &value["args"]["affected"]["Objects"];
    let projected = if super::symbol_name(objects) == Some(MARKER) {
        match objects["args"].as_array().map(Vec::as_slice) {
            Some([inner]) => Some(inner.clone()),
            _ => None,
        }
    } else if let Ok(ObjectSet::Transition {
        result,
        phase: ObjectPhase::After,
    }) = serde_json::from_value::<ObjectSet>(objects.clone())
    {
        Some(
            json!({"kind":"One","args":{"kind":"Result","args":{"result":result,"phase":"After","index":0}}}),
        )
    } else {
        None
    };
    if let Some(objects) = projected {
        let mut operation = value.clone();
        operation["args"]["affected"]["Objects"] = objects;
        found.extend(argument.unify_made(&operation, &bindings));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(result: u32, index: u32) -> Value {
        json!({"kind":"One","args":{"kind":"Result","args":{"result":result,"phase":"After","index":index}}})
    }
    fn group(result: u32) -> Value {
        json!({"kind":"Transition","args":{"result":result,"phase":"After"}})
    }
    fn movement(result: u32, objects: Value, destination: Value) -> Value {
        json!({"kind":"Effect","args":{"result":result,"operation":{"kind":"Move","args":{"objects":objects,"destination":super::super::movement::test_destination(destination),"qualifiers":[]}}}})
    }
    fn exile(result: u32) -> Value {
        movement(
            result,
            json!({"kind":"One","args":{"kind":"Source"}}),
            json!("Exile"),
        )
    }
    fn returning(result: u32, exiled: u32) -> Value {
        movement(
            result,
            json!({"kind":"InZone","args":{"objects":group(exiled),"zone":"Exile"}}),
            json!("OwnersBattlefield"),
        )
    }
    fn grant(result: u32, objects: Value, source: Value) -> Value {
        let op = json!({"kind":"CreateContinuousEffect","args":{"source":source,"affected":{"Objects":objects},"changes":[{"kind":"GrantKeyword","args":{"kind":"Haste"}}],"duration":"Indefinite","evaluation":"Snapshot"}});
        json!({"kind":"Effect","args":{"result":result,"operation":annotate(&op,20,false).unwrap()}})
    }
    fn spell(instructions: Vec<Value>) -> Value {
        json!({"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},"body":{"instructions":instructions}}})
    }
    fn objects(value: &Value, index: usize) -> &Value {
        &value["args"]["body"]["instructions"][index]["args"]["operation"]["args"]["affected"]["Objects"]
    }

    #[test]
    fn actual_return_receipt_proof_preserves_every_other_operand_and_source_role() {
        for source in [json!({"kind":"Source"}), json!({"kind":"GrantingObject"})] {
            let mut value = spell(vec![
                exile(4),
                returning(7, 4),
                grant(8, one(7, 0), source.clone()),
            ]);
            let mut expected = value.clone();
            expected["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["affected"]
                ["Objects"] = group(7);
            lower(&mut value).unwrap();
            assert_eq!(value, expected);
            assert_eq!(
                value["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["source"],
                source
            );
            assert_eq!(
                value["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["objects"]["args"]
                    ["zone"],
                "Exile"
            );
        }
        for returned in [
            one(7, 1),
            one(9, 0),
            json!({"kind":"One","args":{"kind":"Result","args":{"result":7,"phase":"Before","index":0}}}),
        ] {
            let mut value = spell(vec![
                exile(4),
                returning(7, 4),
                grant(8, returned.clone(), json!({"kind":"Source"})),
            ]);
            lower(&mut value).unwrap();
            assert_eq!(objects(&value, 2), &returned);
        }
        for producer in [
            movement(7, group(4), json!("OwnersBattlefield")),
            movement(
                7,
                json!({"kind":"InZone","args":{"objects":group(4),"zone":"Graveyard"}}),
                json!("OwnersBattlefield"),
            ),
            returning(7, 5),
            movement(
                7,
                json!({"kind":"InZone","args":{"objects":group(4),"zone":"Exile"}}),
                json!("OwnersHand"),
            ),
        ] {
            let mut value = spell(vec![
                exile(4),
                producer,
                grant(8, one(7, 0), json!({"kind":"Source"})),
            ]);
            lower(&mut value).unwrap();
            assert_eq!(objects(&value, 2), &one(7, 0));
        }
    }

    #[test]
    fn only_implicit_keyword_action_operands_are_annotated_and_scalar_reads_stay_scalar() {
        let raw = json!({"kind":"One","args":{"$sym":"member_count_it","args":[{"$sym":"it","args":[],"at":3}],"at":3}});
        assert!(implicit(&raw));
        assert!(implicit(
            &json!({"kind":"One","args":{"$sym":"current_named_object","args":[],"at":3}})
        ));
        for reference in [
            json!({"kind":"Source"}),
            json!({"kind":"GrantingObject"}),
            json!({"$sym":"self","args":[],"at":3}),
            json!({"$sym":"named_source","args":[],"at":3}),
            json!({"kind":"Result","args":{"result":7,"phase":"After","index":0}}),
            json!({"kind":"Query","args":{"objects":group(7),"index":0}}),
            json!({"kind":"Query","args":{"objects":{"kind":"Selected","args":7},"index":0}}),
        ] {
            assert!(!implicit(&json!({"kind":"One","args":reference})));
        }
        let mut value = spell(vec![
            exile(4),
            returning(7, 4),
            grant(8, one(7, 0), json!({"kind":"Source"})),
        ]);
        let property = json!({"kind":"ObjectProperty","args":{"object":one(7,0)["args"],"property":"Power","information":"CurrentOrLastKnown"}});
        value["args"]["body"]["instructions"].as_array_mut().unwrap().push(json!({"kind":"Effect","args":{"result":9,"operation":{"kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":property}}}}));
        let untouched = value["args"]["body"]["instructions"][3].clone();
        lower(&mut value).unwrap();
        assert_eq!(value["args"]["body"]["instructions"][3], untouched);
        let mut pump = grant(8, raw, json!({"kind":"Source"}))["args"]["operation"].clone();
        pump["args"]["affected"]["Objects"] = one(7, 0);
        pump["args"]["changes"] = json!([{"kind":"Pump","args":{"power":{"kind":"Constant","args":1},"toughness":{"kind":"Constant","args":1}}}]);
        assert_eq!(annotate(&pump, 3, true).unwrap(), pump);
        let indexed = grant(8, one(7, 0), json!({"kind":"Source"}))["args"]["operation"].clone();
        let mut indexed = indexed;
        indexed["args"]["affected"]["Objects"] = one(7, 0);
        assert_eq!(annotate(&indexed, 3, true).unwrap(), indexed);
    }

    #[test]
    fn delayed_local_return_is_proven_but_independent_holders_modes_events_and_predicates_do_not_capture()
     {
        let steps = vec![
            returning(7, 4),
            grant(8, one(7, 0), json!({"kind":"Source"})),
        ];
        let mut delayed = spell(vec![
            exile(4),
            json!({"kind":"Effect","args":{"result":9,"operation":{"kind":"CreateDelayedTrigger","args":{"body":{"instructions":steps}}}}}),
        ]);
        lower(&mut delayed).unwrap();
        assert_eq!(
            delayed["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["body"]["instructions"]
                [1]["args"]["operation"]["args"]["affected"]["Objects"],
            group(7)
        );
        for inner in [
            spell(steps.clone()),
            json!({"kind":"Triggered","args":{"announcement":{"targets":[],"modes":null},"body":{"instructions":steps}}}),
            json!({"announcement":{"targets":[],"modes":null},"weight":1,"body":{"instructions":steps}}),
            json!({"kind":"CreateTrigger","args":{"announcement":{"targets":[],"modes":null},"body":{"instructions":steps}}}),
        ] {
            let mut outer = spell(vec![
                exile(4),
                json!({"kind":"Effect","args":{"result":9,"operation":{"kind":"CreatePerpetualEffect","args":{"objects":{"kind":"One","args":{"kind":"Source"}},"changes":[{"kind":"GrantAbility","args":inner}]}}}}),
            ]);
            lower(&mut outer).unwrap();
            let inner = &outer["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["changes"]
                [0]["args"];
            let body = if inner.get("kind").is_some() {
                &inner["args"]["body"]
            } else {
                &inner["body"]
            };
            assert_eq!(
                body["instructions"][1]["args"]["operation"]["args"]["affected"]["Objects"],
                one(7, 0)
            );
        }
        let condition = json!({"kind":"ObjectMatches","args":{"object":one(7,0)["args"],"filter":{"kind":"Subtype","args":"Spirit"}}});
        let mut conditional = spell(vec![
            exile(4),
            returning(7, 4),
            json!({"kind":"If","args":{"condition":condition,"then":{"instructions":[grant(8,one(7,0),json!({"kind":"Source"}))]},"otherwise":{"instructions":[]}}}),
        ]);
        lower(&mut conditional).unwrap();
        assert_eq!(
            conditional["args"]["body"]["instructions"][2]["args"]["then"]["instructions"][0]["args"]
                ["operation"]["args"]["affected"]["Objects"],
            one(7, 0)
        );
        let mut local = spell(vec![
            exile(4),
            json!({"kind":"If","args":{"condition":{"kind":"PlayerState","args":{"player":{"kind":"AbilityController"},"state":"FullParty"}},"then":{"instructions":steps},"otherwise":{"instructions":[]}}}),
        ]);
        lower(&mut local).unwrap();
        assert_eq!(
            local["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][1]["args"]["operation"]
                ["args"]["affected"]["Objects"],
            group(7)
        );
    }

    #[test]
    fn own_mode_proof_is_local_and_other_action_roles_never_expand() {
        let steps = vec![
            exile(4),
            returning(7, 4),
            grant(8, one(7, 0), json!({"kind":"GrantingObject"})),
        ];
        let mut mode = json!({"announcement":{"targets":[],"modes":null},"weight":1,"body":{"instructions":steps}});
        lower(&mut mode).unwrap();
        assert_eq!(
            mode["body"]["instructions"][2]["args"]["operation"]["args"]["affected"]["Objects"],
            group(7)
        );
        let mut indexed = spell(vec![
            exile(4),
            returning(7, 4),
            json!({"kind":"Effect","args":{"result":8,"operation":{"kind":"SetTapped","args":{"objects":{"$sym":MARKER,"args":[one(7,0)],"at":4},"tapped":true}}}}),
        ]);
        lower(&mut indexed).unwrap();
        assert_eq!(
            indexed["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["objects"],
            one(7, 0)
        );
        let mut unknown = spell(vec![
            movement(4, json!({"kind":"Targets","args":0}), json!("Exile")),
            returning(7, 4),
            grant(8, one(7, 0), json!({"kind":"Source"})),
        ]);
        lower(&mut unknown).unwrap();
        assert_eq!(objects(&unknown, 2), &one(7, 0));
    }

    #[test]
    fn inverse_projection_uses_the_current_return_only_and_keeps_the_creating_source() {
        use crate::model::ir_schema::Ty;
        let pattern = [Pattern::Hole(
            "operation".into(),
            Ty::Named("Operation".into()),
        )];
        for result in [7, 19] {
            let mut operation =
                grant(8, group(result), json!({"kind":"GrantingObject"}))["args"]["operation"]
                    .clone();
            operation["args"]["affected"]["Objects"] = group(result);
            let found = unify(&pattern, &operation, Bindings::new());
            assert_eq!(found.len(), 2);
            assert!(found.iter().any(|b| {
                b.get("operation").is_some_and(|v| {
                    v["args"]["affected"]["Objects"] == one(result, 0)
                        && v["args"]["source"] == json!({"kind":"GrantingObject"})
                })
            }));
        }
    }

    #[test]
    fn proof_ids_are_shadowed_and_unannotated_values_keep_their_resource_behavior() {
        let draw = json!({"kind":"Effect","args":{"result":7,"operation":{"kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}}}}});
        let mut value = spell(vec![
            exile(4),
            returning(7, 4),
            draw,
            grant(8, one(7, 0), json!({"kind":"Source"})),
        ]);
        lower(&mut value).unwrap();
        assert_eq!(objects(&value, 3), &one(7, 0));
        let mut deep = Value::Null;
        for _ in 0..130 {
            deep = json!([deep]);
        }
        let original = deep.clone();
        lower(&mut deep).unwrap();
        assert_eq!(deep, original);
        let mut marked = json!({"$sym":MARKER,"args":[deep],"at":0});
        assert!(lower(&mut marked).is_err());
    }
}

#[cfg(test)]
mod native_preflight {
    use super::*;
    use crate::engine::{Engine, canonical_abilities};

    #[test]
    fn both_whole_card_grants_have_only_the_correct_full_receipt_and_reject_wrong_native_scalars() {
        let engine = Engine::load().unwrap();
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/returned-keyword-grants.json"
        ))
        .unwrap();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let text = case["text"].as_str().unwrap();
            let expected = canonical_abilities(&case["expected_readings"][0]).unwrap();
            let actual = engine
                .parse(name, text)
                .unwrap_or_else(|why| panic!("{name}: {why:?}"))
                .readings;
            assert_eq!(
                actual,
                vec![expected.clone()],
                "{name}: no stale scalar alternative"
            );
            assert!(
                !actual.contains(&canonical_abilities(&case["native_reference"]).unwrap()),
                "{name}: known wrong native scalar must remain a visible mismatch"
            );
            for previous in case["previous_readings"].as_array().unwrap() {
                assert!(
                    !actual.contains(&canonical_abilities(previous).unwrap()),
                    "{name}: old wrong scalar removed"
                );
            }
            let types: Vec<String> = serde_json::from_value(case["types"].clone()).unwrap();
            let printed = engine
                .print_keyed(name, &expected, &types, &[])
                .unwrap()
                .expect("source-correct default");
            assert_eq!(
                engine.parse(name, &printed).unwrap().readings,
                vec![expected.clone()],
                "{name}: {printed}"
            );
            let trail = engine
                .trail_of(name, &expected, text)
                .unwrap()
                .expect("source-correct original trail");
            assert_eq!(
                engine
                    .print_trail(name, &expected, &trail)
                    .unwrap()
                    .as_deref(),
                Some(text),
                "{name}: exact preservation"
            );
        }
    }

    #[test]
    fn owned_exile_return_grant_composition_and_edited_duration_keep_the_current_receipt() {
        let engine = Engine::load().unwrap();
        let case: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/returned-keyword-grants.json"
        ))
        .unwrap();
        let case = case
            .iter()
            .find(|c| c["name"] == "Justiciar's Portal")
            .unwrap();
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let expected = canonical_abilities(&case["expected_readings"][0]).unwrap();
        let off = engine.rule_set(|n| n.starts_with("m_") || n.starts_with("t_"));
        assert_eq!(
            engine.parse_without(name, text, &off).unwrap().readings,
            vec![expected.clone()]
        );
        let mut edited = expected.clone();
        let effect = &mut edited[0]["args"]["body"]["instructions"][2]["args"]["operation"];
        assert_eq!(effect["args"]["affected"]["Objects"]["kind"], "Transition");
        effect["args"]["duration"] = json!("Indefinite");
        let edited = canonical_abilities(&edited).unwrap();
        assert!(!engine.parse(name, text).unwrap().readings.contains(&edited));
        let printed = engine
            .print_keyed(name, &edited, &["Instant".to_string()], &[])
            .unwrap()
            .expect("edited duration");
        assert_ne!(printed, text);
        assert!(
            engine
                .parse(name, &printed)
                .unwrap()
                .readings
                .contains(&edited)
        );
    }
}
