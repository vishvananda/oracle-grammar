//! Opt-in scalar focus for a single top-library card's actual exile receipt.
use super::{Scope, Step, at_path, transitions};
use crate::model::ir::{
    EventZone, Expr, MovementDestination, ObjectFilter, ObjectKind, ObjectPhase, ObjectRef,
    ObjectSet, Operation, PlayerSet, ResultId, SequenceEnd, ZoneScope,
};
use serde_json::Value;

#[derive(Clone)]
pub(super) struct Receipt {
    result: ResultId,
    sequence: Vec<Step>,
    instruction: usize,
}

impl Receipt {
    fn scalar(&self) -> Value {
        serde_json::to_value(ObjectRef::Result {
            result: self.result.clone(),
            phase: ObjectPhase::After,
            index: 0,
        })
        .expect("typed reference serializes")
    }

    fn visible(&self, path: &[Step]) -> bool {
        let Some(next) = self.instruction.checked_add(1) else {
            return false;
        };
        path.starts_with(&self.sequence)
            && matches!(path.get(self.sequence.len()), Some(Step::Index(index)) if *index == next)
            && path[self.sequence.len() + 1..].windows(2).all(|pair| {
                !matches!((&pair[0], &pair[1]), (Step::Key(key), Step::Index(index)) if key == "instructions" && *index != 0)
            })
    }
}

/// This intentionally excludes multi-library, plural, filtered and unknown-count
/// selectors. Missing/unsuccessful movement leaves the native After set empty.
fn single_top_card(operation: Operation) -> bool {
    let Operation::Move {
        objects,
        destination: MovementDestination::Exile { .. },
        ..
    } = operation
    else {
        return false;
    };
    let ObjectSet::Take {
        objects,
        count: Expr::Constant(1),
        from: SequenceEnd::Top,
    } = objects
    else {
        return false;
    };
    matches!(
        *objects,
        ObjectSet::Zone {
            scope: ZoneScope::Players {
                zone: EventZone::Library,
                players: PlayerSet::One(_)
            },
            kind: ObjectKind::Card,
            filter: ObjectFilter::Any
        }
    )
}

pub(super) fn marker(
    root: &mut Value,
    scope: &mut Scope,
    path: &[Step],
    name: &str,
    introduced: &[super::references::TargetIntroduction],
) -> Result<bool, String> {
    if name != "exiled_card_result" {
        return Ok(false);
    }
    let result = transitions::effect_result(root, path)?;
    let wrapped = at_path(root, path).ok_or("exiled card result path")?["args"]
        .get(0)
        .ok_or("exiled card result needs an operation")?;
    // First target occurrences stay symbolic until finish_scope. Probe their
    // already assigned addresses without materializing or editing the source.
    let plain = super::references::bound_reference(wrapped, introduced, scope.target_base)?;
    let operation: Operation =
        serde_json::from_value(plain).map_err(|why| format!("exiled card operation: {why}"))?;
    if !single_top_card(operation) {
        return Err("exiled card needs one top card of one library moved to exile".into());
    }
    let (depth, instruction) = path.iter().enumerate().rev().find_map(|(depth, step)| {
        if let Step::Index(index) = step
            && matches!(depth.checked_sub(1).and_then(|i| path.get(i)), Some(Step::Key(key)) if key == "instructions")
        {
            Some((depth, *index))
        } else { None }
    }).ok_or("exiled card needs a lexical instruction sequence")?;
    let receipt = Receipt {
        result: ResultId(u32::try_from(result).map_err(|_| "exiled card result address")?),
        sequence: path[..depth].to_vec(),
        instruction,
    };
    let reference = receipt.scalar();
    scope.exiled_card = Some(receipt);
    scope.objects.push(reference.clone());
    scope.named_object = Some(reference);
    scope.others.clear();
    scope.pronoun = None;
    Ok(true)
}

pub(super) fn card(scope: &Scope) -> Option<Value> {
    let reference = scope.exiled_card.as_ref()?.scalar();
    (scope.preceding() == Some(reference.clone())).then_some(reference)
}

pub(super) fn reference(scope: &Scope, path: &[Step]) -> Result<Value, String> {
    let receipt = scope
        .exiled_card
        .as_ref()
        .ok_or("no preceding scalar exile receipt")?;
    let reference = card(scope).ok_or("the exile receipt is not the current card")?;
    if !receipt.visible(path) {
        return Err("the exile receipt is outside this lexical continuation".into());
    }
    Ok(reference)
}

pub(super) fn check_scalar(value: &Value, path: &[Step], scope: &Scope) -> Result<(), String> {
    if let Some(receipt) = &scope.exiled_card
        && *value == receipt.scalar()
        && !receipt.visible(path)
    {
        return Err("the exile receipt is outside this lexical continuation".into());
    }
    Ok(())
}

/// Preserve the producer/receipt dependency during inverse search. Otherwise
/// unmarked but shape-equal exile spellings can fill every default beam.
/// This adds only metadata around the exact current operation; reparsing still
/// verifies the complete current program, player and After operands.
pub(super) fn surface(ability: &mut Value) -> bool {
    let Some(instructions) = ability
        .get_mut("args")
        .and_then(|args| args.get_mut("body"))
        .and_then(|body| body.get_mut("instructions"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for index in 0..instructions.len().saturating_sub(1) {
        let producer = &instructions[index];
        if producer["kind"] != "Effect" {
            continue;
        }
        let Some(result) = producer["args"]["result"]
            .as_u64()
            .and_then(|id| u32::try_from(id).ok())
        else {
            continue;
        };
        let operation = &producer["args"]["operation"];
        if operation["kind"] != "Move"
            || !serde_json::from_value::<Operation>(operation.clone()).is_ok_and(single_top_card)
        {
            continue;
        }
        let reference = Receipt {
            result: ResultId(result),
            sequence: Vec::new(),
            instruction: index,
        }
        .scalar();
        let next = &instructions[index + 1];
        let condition = &next["args"]["condition"];
        if next["kind"] != "If"
            || condition["kind"] != "ObjectMatches"
            || condition["args"]["object"] != reference
        {
            continue;
        }
        let operation = &mut instructions[index]["args"]["operation"];
        *operation =
            serde_json::json!({"$sym":"exiled_card_result","args":[operation.take()],"at":0});
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn operation(count: i64, players: Value) -> Value {
        json!({"kind":"Move","args":{"objects":{"kind":"Take","args":{"objects":{"kind":"Zone","args":{"scope":{"Players":{"zone":"Library","players":players}},"kind":"Card","filter":{"kind":"Any"}}},"count":{"kind":"Constant","args":count},"from":"Top"}},"destination":{"Exile":{"visibility":"FaceUp","arrival":{"counters":[],"states":[]}}},"qualifiers":[]}})
    }

    #[test]
    fn cardinality_proof_never_selects_an_arbitrary_group_member() {
        for (count, players, expected) in [
            (
                1,
                json!({"kind":"One","args":{"kind":"EventSubject"}}),
                true,
            ),
            (
                2,
                json!({"kind":"One","args":{"kind":"EventSubject"}}),
                false,
            ),
            (1, json!({"kind":"All"}), false),
        ] {
            assert_eq!(
                single_top_card(serde_json::from_value(operation(count, players)).unwrap()),
                expected
            );
        }
    }

    #[test]
    fn actual_after_receipt_does_not_bind_the_player_or_escape_its_branch() {
        let mut scope = Scope::default();
        scope.players.push(json!({"kind":"EventSubject"}));
        let sequence = vec![Step::Key("instructions".into())];
        scope.exiled_card = Some(Receipt {
            result: ResultId(7),
            sequence: sequence.clone(),
            instruction: 0,
        });
        let reference = scope.exiled_card.as_ref().unwrap().scalar();
        scope.objects.push(reference.clone());
        let mut following = sequence.clone();
        following.push(Step::Index(1));
        assert_eq!(super::reference(&scope, &following).unwrap(), reference);
        assert_eq!(scope.player().unwrap(), json!({"kind":"EventSubject"}));
        assert!(super::reference(&Scope::default(), &following).is_err());
        assert!(super::reference(&scope, &sequence).is_err());
        let nested = vec![
            Step::Key("body".into()),
            Step::Key("instructions".into()),
            Step::Index(1),
        ];
        assert!(super::reference(&scope, &nested).is_err());
    }
    #[test]
    fn marker_uses_the_actual_definition_address_and_after_phase() {
        let operation = operation(1, json!({"kind":"One","args":{"kind":"EventSubject"}}));
        let mut root = json!({"instructions":[{"kind":"Effect","args":{"result":41,
            "operation":{"$sym":"exiled_card_result","args":[operation],"at":9999}}}]});
        let path = vec![
            Step::Key("instructions".into()),
            Step::Index(0),
            Step::Key("args".into()),
            Step::Key("operation".into()),
        ];
        let mut scope = Scope::default();
        scope.players.push(json!({"kind":"EventSubject"}));
        assert!(marker(&mut root, &mut scope, &path, "exiled_card_result", &[]).unwrap());
        let next = vec![Step::Key("instructions".into()), Step::Index(1)];
        assert_eq!(
            reference(&scope, &next).unwrap(),
            json!({"kind":"Result","args":{
            "result":41,"phase":"After","index":0}})
        );
        assert_eq!(scope.player().unwrap(), json!({"kind":"EventSubject"}));
        for reference in [json!({"kind":"Source"}), json!({"kind":"GrantingObject"})] {
            assert!(check_scalar(&reference, &[], &scope).is_ok());
        }
    }

    #[test]
    fn branch_receipts_cannot_escape_or_skip_other_instructions() {
        let sequence = vec![
            Step::Key("instructions".into()),
            Step::Index(0),
            Step::Key("args".into()),
            Step::Key("then".into()),
            Step::Key("instructions".into()),
        ];
        let receipt = Receipt {
            result: ResultId(2),
            sequence: sequence.clone(),
            instruction: 0,
        };
        let mut local = sequence;
        local.push(Step::Index(1));
        assert!(receipt.visible(&local));
        *local.last_mut().unwrap() = Step::Index(2);
        assert!(!receipt.visible(&local));
        assert!(!receipt.visible(&[Step::Key("instructions".into()), Step::Index(1)]));
        let mut sibling = local;
        sibling[3] = Step::Key("otherwise".into());
        assert!(!receipt.visible(&sibling));
    }
    #[test]
    fn inverse_marker_preserves_current_operands_and_exact_receipt_dependency() {
        let movement = operation(1, json!({"kind":"One","args":{"kind":"EventSubject"}}));
        let card = json!({"kind":"Result","args":{"result":7,"phase":"After","index":0}});
        let mut ability = json!({"kind":"Triggered","args":{"body":{"instructions":[
            {"kind":"Effect","args":{"result":7,"operation":movement.clone()}},
            {"kind":"If","args":{"condition":{"kind":"ObjectMatches","args":{
                "object":card,"filter":{"kind":"CardType","args":"Artifact"}}},"then":{"instructions":[]},"otherwise":{"instructions":[]}}}
        ]}}});
        let expected = ability.clone();
        assert!(surface(&mut ability));
        assert!(!surface(&mut ability));
        assert_eq!(
            ability["args"]["body"]["instructions"][0]["args"]["operation"]["args"][0],
            movement
        );
        super::super::filters::strip(&mut ability);
        assert_eq!(ability, expected);
        let mut unrelated = expected;
        unrelated["args"]["body"]["instructions"][1]["args"]["condition"]["args"]["object"]["args"]
            ["result"] = json!(8);
        assert!(!surface(&mut unrelated));
    }
    #[test]
    fn pending_target_is_probed_without_editing_the_unconsumed_operation() {
        let spec = json!({"slot":{"$sym":"slot","args":[],"at":3},"kind":"Player",
            "count":{"Exactly":{"kind":"Constant","args":1}},"filter":{"kind":"Any"},
            "players":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}}});
        let target = json!({"kind":"Target","args":{"slot":{"$sym":"target","args":[spec.clone()],"at":3},"index":0}});
        let op = operation(1, json!({"kind":"One","args":target}));
        let mut root = json!({"instructions":[{"kind":"Effect","args":{"result":41,
            "operation":{"$sym":"exiled_card_result","args":[op],"at":3}}}]});
        let before = root.clone();
        let path = vec![
            Step::Key("instructions".into()),
            Step::Index(0),
            Step::Key("args".into()),
            Step::Key("operation".into()),
        ];
        let mut scope = Scope {
            target_base: 2,
            ..Scope::default()
        };
        let introduced = vec![((3, spec.to_string()), 0)];
        assert!(
            marker(
                &mut root,
                &mut scope,
                &path,
                "exiled_card_result",
                &introduced
            )
            .unwrap()
        );
        assert_eq!(root, before);
        let next = vec![Step::Key("instructions".into()), Step::Index(1)];
        assert_eq!(reference(&scope, &next).unwrap()["args"]["result"], 41);
        let mut unavailable = Scope::default();
        assert!(
            marker(
                &mut root,
                &mut unavailable,
                &path,
                "exiled_card_result",
                &[]
            )
            .is_err()
        );
        assert!(unavailable.exiled_card.is_none());
        assert_eq!(root, before);
    }
}
