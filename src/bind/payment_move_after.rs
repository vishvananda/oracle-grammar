//! An implicit action after a chosen exile payment follows its actual output.
use super::{Step, optional_exile_target, references, transitions::Introductions};
use crate::model::ir::{
    Cost, CostObjectView, CostPart, Instruction, MovementDestination, ObjectPhase, ObjectRef,
    ObjectSet, Operation, PaymentRef, ResultId,
};
use serde_json::Value;

fn at<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get(key)?,
            Step::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}

fn same(objects: &ObjectSet, reference: &Value) -> Result<bool, String> {
    let ObjectSet::One(object) = objects else {
        return Ok(false);
    };
    Ok(serde_json::to_value(object).map_err(|e| e.to_string())? == *reference)
}

fn output(cost: Cost, payment: u32, reference: &Value) -> Result<Option<ObjectRef>, String> {
    let mut found = None;
    let mut components: Vec<_> = crate::model::cost_history::components(&cost)
        .into_iter()
        .filter_map(|(_, path)| {
            crate::model::cost_history::component(&cost, &path).map(|part| (part, path))
        })
        .collect();
    // PlayerAction exile has an instruction result, not a cost-history entry.
    for (index, mut part) in cost.parts.iter().enumerate() {
        while let CostPart::Constrained { part: inner, .. } = part {
            part = inner;
        }
        if matches!(part, CostPart::PlayerAction { .. })
            && crate::model::cost_history::action(part).is_none()
        {
            components.push((
                part,
                vec![u32::try_from(index).map_err(|_| "payment component index")?],
            ));
        }
    }
    if components.len() > 4096 {
        return Err("payment move component limit".into());
    }
    for (part, component) in components {
        let candidate = match part {
            CostPart::Move {
                selections,
                destination: MovementDestination::Exile { .. },
            } if crate::model::cost_history::singular(part) => {
                let [selection] = selections.as_slice() else {
                    continue;
                };
                if !same(&selection.objects, reference)? {
                    continue;
                }
                ObjectRef::Query {
                    objects: Box::new(ObjectSet::CostObjects {
                        payment: PaymentRef::Resolution(ResultId(payment)),
                        component,
                        view: CostObjectView::Performed,
                        phase: ObjectPhase::After,
                    }),
                    index: 0,
                }
            }
            CostPart::PlayerAction { body, .. } => {
                let [
                    Instruction::Effect {
                        result,
                        operation:
                            Operation::Move {
                                objects,
                                destination: MovementDestination::Exile { .. },
                                ..
                            },
                    },
                ] = body.instructions.as_slice()
                else {
                    continue;
                };
                if !same(objects, reference)? {
                    continue;
                }
                ObjectRef::Result {
                    result: ResultId(result.0),
                    phase: ObjectPhase::After,
                    index: 0,
                }
            }
            _ => continue,
        };
        if found.is_some() {
            return Err("implicit return has multiple matching exile payment results".into());
        }
        found = Some(candidate);
    }
    Ok(found)
}

/// The exact adjacent payment and its chosen arm are required. Explicit literal
/// references, other branches and another event's scope acquire no projection.
pub(super) fn reference(
    root: &Value,
    path: &[Step],
    introductions: &Introductions<'_>,
    reference: &Value,
) -> Result<Option<Value>, String> {
    if !optional_exile_target::implicit_action(root, path)? {
        return Ok(None);
    }
    let mut found = None;
    for depth in (1..path.len()).rev() {
        if !matches!(path.get(depth), Some(Step::Key(key)) if key == "args")
            || !matches!(path.get(depth + 1), Some(Step::Key(key)) if key == "then")
        {
            continue;
        }
        let Some(branch) = at(root, &path[..depth]) else {
            continue;
        };
        if branch["kind"] != "If" {
            continue;
        }
        if (depth + 2..path.len()).any(|child| {
            at(root, &path[..child]).is_some_and(|node| {
                matches!(
                    node["kind"].as_str(),
                    Some(
                        "CreateTrigger" | "CreateReplacement" | "Triggered" | "Activated" | "Spell"
                    )
                ) || node["kind"] == "CreateDelayedTrigger"
                    && node["args"]["event"].get("Turn").is_none()
            })
        }) {
            continue;
        }
        let Step::Index(index) = path[depth - 1] else {
            continue;
        };
        let Some(previous) = index
            .checked_sub(1)
            .and_then(|index| at(root, &path[..depth - 1])?.get(index))
        else {
            continue;
        };
        let Some(payment) = previous["args"]["result"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
        else {
            continue;
        };
        if previous["kind"] != "Effect"
            || previous["args"]["operation"]["kind"] != "PayCost"
            || branch["args"]["condition"]["kind"] != "PaymentChosen"
            || branch["args"]["condition"]["args"].as_u64() != Some(u64::from(payment))
        {
            continue;
        }
        let raw = &previous["args"]["operation"]["args"]["cost"];
        crate::functions::branch_bounds(raw)?;
        let mut raw = references::bound_reference(raw, introductions.keys, introductions.base)?;
        super::filters::strip(&mut raw);
        let Ok(cost) = serde_json::from_value::<Cost>(raw) else {
            continue;
        };
        if let Some(value) = output(cost, payment, reference)? {
            let value = serde_json::to_value(value).map_err(|e| e.to_string())?;
            if found.as_ref().is_some_and(|previous| *previous != value) {
                return Err("implicit return has multiple enclosing exile payments".into());
            }
            found = Some(value);
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cost(action: bool) -> Cost {
        let objects = json!({"kind":"One","args":{"kind":"EventSubject"}});
        let exile = json!({"Exile":{"visibility":"FaceUp","arrival":{"counters":[],"states":[]}}});
        let part = if action {
            json!({"PlayerAction":{"player":{"kind":"AbilityController"},"body":{"instructions":[
                {"kind":"Effect","args":{"result":13,"operation":{"kind":"Move","args":{
                    "objects":objects,"destination":exile,"qualifiers":[]
                }}}}
            ]}}})
        } else {
            json!({"Move":{"selections":[{"objects":objects,"choose":null}],"destination":exile}})
        };
        serde_json::from_value(json!({"parts":[part]})).unwrap()
    }

    #[test]
    fn native_and_action_costs_name_their_actual_distinct_outputs() {
        let input = json!({"kind":"EventSubject"});
        let native =
            serde_json::to_value(output(cost(false), 7, &input).unwrap().unwrap()).unwrap();
        assert_eq!(
            native["args"]["objects"]["args"]["payment"],
            json!({"Resolution":7})
        );
        assert_eq!(native["args"]["objects"]["args"]["phase"], "After");
        let action = serde_json::to_value(output(cost(true), 7, &input).unwrap().unwrap()).unwrap();
        assert_eq!(
            action,
            json!({"kind":"Result","args":{"result":13,"phase":"After","index":0}})
        );
        assert!(
            output(cost(false), 7, &json!({"kind":"Source"}))
                .unwrap()
                .is_none()
        );
        let mut ambiguous = cost(false);
        ambiguous.parts.push(cost(false).parts.remove(0));
        assert!(output(ambiguous, 7, &input).is_err());
    }

    fn fixture() -> (Value, Vec<Step>) {
        let root = json!({"kind":"CreateTrigger","args":{"body":{"instructions":[
            {"kind":"Effect","args":{"result":7,"operation":{"kind":"PayCost","args":{
                "player":{"kind":"AbilityController"},"optional":true,"cost":cost(false)
            }}}},
            {"kind":"If","args":{"condition":{"kind":"PaymentChosen","args":7},
                "then":{"instructions":[{"kind":"Effect","args":{"result":8,
                    "operation":{"kind":"Move","args":{"objects":{"$sym":"exile_followup","at":20,"args":[
                        {"kind":"One","args":{"$sym":"it","args":[],"at":20}}
                    ]}}}}}]},"otherwise":{"instructions":[]}}}
        ]}}});
        let path = "/args/body/instructions/1/args/then/instructions/0/args/operation/args/objects/args/0/args"
            .split('/').skip(1).map(|part| part.parse().map_or_else(
                |_| Step::Key(part.into()), Step::Index)).collect();
        (root, path)
    }

    #[test]
    fn projection_requires_the_exact_adjacent_paid_arm_and_implicit_wording() {
        let introductions = Introductions {
            slots: &[],
            base: 0,
            keys: &[],
        };
        let (mut root, path) = fixture();
        let object = json!({"kind":"EventSubject"});
        assert!(
            reference(&root, &path, &introductions, &object)
                .unwrap()
                .is_some()
        );
        root["args"]["body"]["instructions"][1]["args"]["condition"]["args"] = json!(6);
        assert!(
            reference(&root, &path, &introductions, &object)
                .unwrap()
                .is_none()
        );
        root["args"]["body"]["instructions"][1]["args"]["condition"]["args"] = json!(7);
        let objects = &mut root["args"]["body"]["instructions"][1]["args"]["then"]["instructions"]
            [0]["args"]["operation"]["args"]["objects"];
        *objects = json!({"kind":"One","args":{"kind":"EventSubject"}});
        assert!(
            reference(&root, &path[..path.len() - 2], &introductions, &object)
                .unwrap()
                .is_none()
        );
    }
}
