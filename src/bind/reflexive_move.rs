//! Successful battlefield incarnation for an explicitly marked callback actor.
use super::{Scope, Step, at_path, references};
use crate::model::ir::{MovementDestination, ObjectRef, ObjectSet, Operation, ResultId};
use serde_json::{Value, json};

#[derive(Clone)]
pub(super) struct Moved {
    object: ObjectRef,
    result: ResultId,
}

/// Opt in only for the first direct callback action's explicitly marked actor.
/// No traversal or producer preview is needed for an unrelated callback.
pub(super) fn requested(node: &Value) -> bool {
    let trigger = &node["args"][0];
    let first = &trigger["args"]["body"]["instructions"][0];
    let operation = &first["args"]["operation"];
    trigger["kind"] == "ReflexiveTrigger"
        && first["kind"] == "Effect"
        && operation["kind"] == "Keyword"
        && operation["args"]["action"].get("Fight").is_some()
        && super::symbol_name(&operation["args"]["action"]["Fight"]["first"])
            == Some("reflexive_moved_fight_object")
}

/// Only the actual performed direct sibling can prove this incarnation. The
/// nearest instruction sequence is the lexical boundary; never search through
/// another program, an optional branch, a holder, or a nested callback.
pub(super) fn prove(
    root: &mut Value,
    path: &[Step],
    performed: ResultId,
    scope: &Scope,
    introduced: &[references::TargetIntroduction],
) -> Result<Option<Moved>, String> {
    let Some((depth, index)) = path.iter().enumerate().rev().find_map(|(depth, step)| {
        if let Step::Index(index) = step
            && matches!(depth.checked_sub(1).and_then(|n| path.get(n)), Some(Step::Key(k)) if k == "instructions")
        { Some((depth, *index)) } else { None }
    }) else { return Ok(None) };
    let instructions = at_path(root, &path[..depth])
        .and_then(|node| node.as_array())
        .ok_or("reflexive moved actor needs lexical instructions")?;
    if instructions.len() > 256 || index > instructions.len() {
        return Err("reflexive moved actor instruction bound".into());
    }
    let candidates: Vec<_> = instructions[..index]
        .iter()
        .filter(|instruction| {
            instruction["kind"] == "Effect" && instruction["args"]["result"] == json!(performed)
        })
        .collect();
    let [instruction] = candidates.as_slice() else {
        return Ok(None);
    };
    let mut plain = references::bound_reference(
        &instruction["args"]["operation"],
        introduced,
        scope.target_base,
    )?;
    super::filters::strip(&mut plain);
    super::objects::resolve_shapes(&mut plain)?;
    let Ok(Operation::Move {
        objects: ObjectSet::One(object),
        destination,
        ..
    }) = serde_json::from_value::<Operation>(plain)
    else {
        return Ok(None);
    };
    if !matches!(destination, MovementDestination::Battlefield { .. })
        || scope.object_pronoun(false).ok() != Some(json!(object))
    {
        return Ok(None);
    }
    Ok(Some(Moved {
        object,
        result: performed,
    }))
}

pub(super) fn actor(
    inner: &Value,
    moved: Option<&Moved>,
    introduced: &[references::TargetIntroduction],
    base: usize,
) -> Result<Value, String> {
    // With no proof retain the exact subtree, including pending target paths.
    let Some(moved) = moved else {
        return Ok(inner.clone());
    };
    let mut plain = references::bound_reference(inner, introduced, base)?;
    super::filters::strip(&mut plain);
    let object: ObjectRef = serde_json::from_value(plain)
        .map_err(|why| format!("reflexive moved fight object: {why}"))?;
    Ok(if json!(object) == json!(moved.object) {
        json!({"kind":"Result","args":{
            "result":moved.result,"phase":"After","index":0}})
    } else {
        inner.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_in_requires_the_first_direct_marked_fight_actor() {
        let fight = json!({"kind":"Effect","args":{"operation":{"kind":"Keyword",
            "args":{"action":{"Fight":{"first":{"$sym":"reflexive_moved_fight_object",
                "args":[{"$sym":"object_pronoun"}]}}}}}}});
        let callback = |instructions: Vec<Value>| {
            json!({"$sym":"reflexive","args":[{
            "kind":"ReflexiveTrigger","args":{"body":{"instructions":instructions}}}]})
        };
        assert!(requested(&callback(vec![fight.clone()])));
        assert!(!requested(&callback(vec![
            json!({"kind":"Effect"}),
            fight.clone()
        ])));
        assert!(!requested(&callback(vec![json!({"kind":"Group","args":{
            "instructions":[fight.clone()]}})])));
        let mut unmarked = fight;
        unmarked["args"]["operation"]["args"]["action"]["Fight"]["first"] =
            json!({"kind":"Source"});
        assert!(!requested(&callback(vec![unmarked])));
    }

    fn frame() -> (Value, Vec<Step>, Scope) {
        let object = json!({"kind":"Target","args":{"slot":7,"index":0}});
        let mut scope = Scope::default();
        scope.objects.push(object.clone());
        let root = json!({"instructions":[{"kind":"Effect","args":{"result":12,"operation":{"kind":"Move","args":{"objects":{"kind":"One","args":object},"destination":{"Battlefield":{"controller":"Owner","arrival":{"controller":null,"tapped":false,"counters":[],"states":[]}}},"qualifiers":[]}}}},{"kind":"Effect","args":{"result":13,"operation":null}}]});
        (
            root,
            vec![
                Step::Key("instructions".into()),
                Step::Index(1),
                Step::Key("args".into()),
                Step::Key("operation".into()),
            ],
            scope,
        )
    }

    #[test]
    fn exact_performed_move_changes_only_matching_marked_actor() {
        let (mut root, path, scope) = frame();
        let moved = prove(&mut root, &path, ResultId(12), &scope, &[]).unwrap();
        assert!(moved.is_some());
        assert_eq!(
            actor(&scope.object().unwrap(), moved.as_ref(), &[], 0).unwrap(),
            json!({"kind":"Result","args":{"result":12,"phase":"After","index":0}})
        );
        for object in [
            json!({"kind":"Source"}),
            json!({"kind":"GrantingObject"}),
            json!({"kind":"Target","args":{"slot":8,"index":0}}),
        ] {
            assert_eq!(actor(&object, moved.as_ref(), &[], 0).unwrap(), object);
        }
        assert!(
            prove(&mut root, &path, ResultId(99), &scope, &[])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn grammar_battlefield_move_resolves_deferred_zone_members_only_in_preview() {
        let (mut root, path, scope) = frame();
        let objects = root["instructions"][0]["args"]["operation"]["args"]["objects"].take();
        root["instructions"][0]["args"]["operation"]["args"]["objects"] =
            json!({"$sym":"zone_members","at":5,"args":[objects]});
        let original = root.clone();
        let moved = prove(&mut root, &path, ResultId(12), &scope, &[]).unwrap();
        assert!(moved.is_some());
        assert_eq!(root, original);
        assert_eq!(
            actor(&scope.object().unwrap(), moved.as_ref(), &[], 0).unwrap(),
            json!({"kind":"Result","args":{"result":12,"phase":"After","index":0}})
        );
    }

    #[test]
    fn pending_target_preview_uses_its_assigned_address() {
        let (mut root, path, mut scope) = frame();
        let specification = json!({"slot":{"$sym":"slot"},"kind":"Object"});
        root["instructions"][0]["args"]["operation"]["args"]["objects"]["args"]["args"]["slot"] =
            json!({"$sym":"target","at":41,"args":[specification.clone()]});
        scope.target_base = 5;
        let introduced = vec![((41, specification.to_string()), 2)];
        assert!(
            prove(&mut root, &path, ResultId(12), &scope, &introduced)
                .unwrap()
                .is_some()
        );
        assert!(prove(&mut root, &path, ResultId(12), &scope, &[]).is_err());
    }

    #[test]
    fn nonbattlefield_plural_wrong_focus_nested_and_duplicate_ids_never_prove() {
        let (root, path, scope) = frame();
        let mut nonbattlefield = root.clone();
        nonbattlefield["instructions"][0]["args"]["operation"]["args"]["destination"] =
            super::super::movement::test_destination(json!("Exile"));
        assert!(
            prove(&mut nonbattlefield, &path, ResultId(12), &scope, &[])
                .unwrap()
                .is_none()
        );
        let mut plural = root.clone();
        plural["instructions"][0]["args"]["operation"]["args"]["objects"] =
            json!({"kind":"Selected","args":3});
        assert!(
            prove(&mut plural, &path, ResultId(12), &scope, &[])
                .unwrap()
                .is_none()
        );
        let mut wrong_focus = scope.clone();
        wrong_focus.objects = vec![json!({"kind":"Source"})];
        assert!(
            prove(&mut root.clone(), &path, ResultId(12), &wrong_focus, &[])
                .unwrap()
                .is_none()
        );
        let mut duplicate = root.clone();
        duplicate["instructions"]
            .as_array_mut()
            .unwrap()
            .insert(0, root["instructions"][0].clone());
        let mut duplicate_path = path.clone();
        duplicate_path[1] = Step::Index(2);
        assert!(
            prove(&mut duplicate, &duplicate_path, ResultId(12), &scope, &[])
                .unwrap()
                .is_none()
        );
        let mut nested = json!({"instructions":[{"kind":"Group","args":root}]});
        let nested_path = vec![Step::Key("instructions".into()), Step::Index(1)];
        assert!(
            prove(&mut nested, &nested_path, ResultId(12), &scope, &[])
                .unwrap()
                .is_none()
        );
    }
}
