//! Opt-in scalar focus for the actual exile receipt of a single top-library card or of
//! one chosen card.
use super::{Scope, Step, at_path, transitions};
use crate::model::ir::{
    Count, EventZone, Expr, MovementDestination, ObjectFilter, ObjectKind, ObjectPhase, ObjectRef,
    ObjectSet, Operation, PlayerSet, ResultId, SequenceEnd, ZoneScope,
};
use serde_json::Value;

#[derive(Clone)]
pub(super) struct Receipt {
    result: ResultId,
    sequence: Vec<Step>,
    instruction: usize,
    /// "Exile cards ... until you exile a nonland card": the one exiled card the reveal
    /// stopped on, among every card that exile moved.
    until: Option<Value>,
}

impl Receipt {
    fn scalar(&self) -> Value {
        if let Some(filter) = &self.until {
            return serde_json::json!({"kind":"Query","args":{"objects":{"kind":"Matching","args":{
                "objects":{"kind":"Transition","args":{"result":self.result.0,"phase":"After"}},
                "filter":filter,
            }},"index":0}});
        }
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
        let in_window = if self.until.is_some() {
            matches!(path.get(self.sequence.len()), Some(Step::Index(index)) if *index >= next)
        } else {
            matches!(path.get(self.sequence.len()), Some(Step::Index(index)) if *index == next)
        };
        path.starts_with(&self.sequence)
            && in_window
            && path[self.sequence.len() + 1..].windows(2).all(|pair| {
                !matches!((&pair[0], &pair[1]), (Step::Key(key), Step::Index(index)) if key == "instructions" && *index != 0)
            })
    }
}

/// This intentionally excludes multi-library, plural, filtered and unknown-count
/// selectors. Missing/unsuccessful movement leaves the native After set empty.
pub(super) fn single_top_card(operation: &Operation) -> bool {
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
        objects.as_ref(),
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
    let (depth, instruction) = path.iter().enumerate().rev().find_map(|(depth, step)| {
        if let Step::Index(index) = step
            && matches!(depth.checked_sub(1).and_then(|i| path.get(i)), Some(Step::Key(key)) if key == "instructions")
        {
            Some((depth, *index))
        } else { None }
    }).ok_or("exiled card needs a lexical instruction sequence")?;
    let operation: Operation = serde_json::from_value(plain.clone())
        .map_err(|why| format!("exiled card operation: {why}"))?;
    let (sequence, instruction, until) = if single_top_card(&operation)
        || single_choice_exile(root, &path[..depth], instruction, &plain)
    {
        (path[..depth].to_vec(), instruction, None)
    } else {
        let filter = until_exiled(root, &path[..depth], instruction, &plain)
            .ok_or("exiled card needs one top card of one library moved to exile")?;
        (path[..depth].to_vec(), instruction, Some(filter))
    };
    let receipt = Receipt {
        result: ResultId(u32::try_from(result).map_err(|_| "exiled card result address")?),
        sequence,
        instruction,
        until,
    };
    let reference = receipt.scalar();
    scope.exiled_card = Some(receipt);
    scope.objects.push(reference.clone());
    scope.named_object = Some(reference);
    scope.others.clear();
    scope.pronoun = None;
    Ok(true)
}

/// "Target player exiles a card from their hand": one player's choice of at most one
/// card, then the exile of what was chosen, in the sequence that holds both.
fn single_choice_exile(
    root: &Value,
    sequence: &[Step],
    instruction: usize,
    operation: &Value,
) -> bool {
    let mut node = root;
    for step in sequence {
        let next = match step {
            Step::Key(key) => node.get(key),
            Step::Index(index) => node.get(*index),
        };
        let Some(next) = next else { return false };
        node = next;
    }
    let Some(choice) = instruction
        .checked_sub(1)
        .and_then(|index| node.as_array()?.get(index))
    else {
        return false;
    };
    let chosen = &choice["args"]["operation"];
    let selected = &operation["args"]["objects"];
    let one = serde_json::from_value::<Count>(chosen["args"]["count"].clone()).is_ok_and(|count| {
        matches!(
            count,
            Count::Exactly(Expr::Constant(1)) | Count::AsManyAsCan(Expr::Constant(1))
        )
    });
    choice["kind"] == "Effect"
        && chosen["kind"] == "ChooseObjects"
        && chosen["args"]["constraints"] == serde_json::json!([])
        && one
        && operation["kind"] == "Move"
        && operation["args"]["destination"].get("Exile").is_some()
        && selected["kind"] == "Selected"
        && (selected["args"] == choice["args"]["result"] || !selected["args"].is_u64())
}

/// "Exile cards from the top of your library until you exile a nonland card": one reveal
/// that stops on one card, then an exile of every card it observed. The receipt follows
/// the exile, in the sequence that holds both.
fn until_exiled(
    root: &Value,
    sequence: &[Step],
    instruction: usize,
    operation: &Value,
) -> Option<Value> {
    let mut node = root;
    for step in sequence {
        node = match step {
            Step::Key(key) => node.get(key)?,
            Step::Index(index) => node.get(*index)?,
        };
    }
    let reveal = node.as_array()?.get(instruction.checked_sub(1)?)?;
    let until = &reveal["args"]["operation"];
    let observed = &operation["args"]["objects"];
    (until["kind"] == "RevealUntil"
        && until["args"]["count"] == serde_json::json!({"kind":"Constant","args":1})
        && until["args"]["library"].is_object()
        && operation["kind"] == "Move"
        && operation["args"]["destination"].get("Exile").is_some()
        && observed["kind"] == "Observed"
        && (observed["args"] == reveal["args"]["result"] || !observed["args"].is_u64()))
    .then(|| until["args"]["filter"].clone())
}

/// "Exile cards ... until you exile a nonland card. You may cast that card": with the
/// receipt for the card the reveal stopped on, the exile's first result names no
/// particular card of those it moved.
pub(super) fn names_first_exiled(ability: &Value, scope: &Scope) -> bool {
    fn mentions(value: &Value, wanted: &Value) -> bool {
        value == wanted
            || match value {
                Value::Object(fields) => fields.values().any(|value| mentions(value, wanted)),
                Value::Array(items) => items.iter().any(|value| mentions(value, wanted)),
                _ => false,
            }
    }
    scope
        .exiled_card
        .as_ref()
        .filter(|receipt| receipt.until.is_some())
        .is_some_and(|receipt| {
            let first = serde_json::json!({"kind":"Result","args":{
                "result":receipt.result.0,"phase":"After","index":0}});
            mentions(ability, &first)
        })
}

/// Only this syntactic shape opts in before context binding. The strict typed
/// producer check above proves it again once the library's player is bound.
pub(crate) fn annotate(operation: &Value, at: usize) -> Result<Value, String> {
    crate::functions::branch_bounds(operation)?;
    let mut positional = operation.clone();
    super::ordered_zones::lower(&mut positional);
    let objects = &positional["args"]["objects"];
    let zone = &objects["args"]["objects"];
    if operation["kind"] == "Move"
        && operation["args"]["destination"].get("Exile").is_some()
        && objects["kind"] == "Take"
        && objects["args"]["count"] == serde_json::json!({"kind":"Constant","args":1})
        && objects["args"]["from"] == "Top"
        && zone["kind"] == "Zone"
        && zone["args"]["scope"]["Players"]["zone"] == "Library"
        && zone["args"]["scope"]["Players"]["players"]["kind"] == "One"
        && zone["args"]["kind"] == "Card"
        && zone["args"]["filter"]["kind"] == "Any"
    {
        Ok(serde_json::json!({"$sym":"exiled_card_result","args":[positional],"at":at}))
    } else {
        Ok(operation.clone())
    }
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

/// A known actual exile arrival cannot supply a graveyard-only cast permission.
pub(super) fn check_permission(
    root: &Value,
    value: &Value,
    path: &[Step],
    scope: &Scope,
) -> Result<(), String> {
    if !scope
        .exiled_card
        .as_ref()
        .is_some_and(|receipt| *value == receipt.scalar())
    {
        return Ok(());
    }
    if path.len() > 128 {
        return Err("card permission role depth".into());
    }
    let mut parent = root;
    for step in path {
        match step {
            Step::Key(key) => {
                if key == "cards"
                    && parent.get("action").is_some()
                    && parent.get("from").is_some()
                    && parent["from"] != "Exile"
                {
                    return Err("an actual exile receipt needs permission from exile".into());
                }
                parent = &parent[key.as_str()];
            }
            Step::Index(index) => parent = &parent[*index],
        }
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
            || !serde_json::from_value::<Operation>(operation.clone())
                .is_ok_and(|operation| single_top_card(&operation))
        {
            continue;
        }
        let reference = Receipt {
            result: ResultId(result),
            sequence: Vec::new(),
            instruction: index,
            until: None,
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
                single_top_card(&serde_json::from_value(operation(count, players)).unwrap()),
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
            until: None,
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
            until: None,
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
