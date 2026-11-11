//! An explicit new fighter cannot shadow the creature named as its counterpart.
use super::{Scope, Step, qualified, references, symbol_name};
use crate::model::ir::{ObjectKind, ObjectRef};
use serde_json::Value;

pub(super) fn reference(
    root: &Value,
    path: &[Step],
    introduced: &[references::TargetIntroduction],
    scope: &Scope,
    contexts: &references::Contexts,
) -> Result<Option<Value>, String> {
    let [
        operation_path @ ..,
        Step::Key(args),
        Step::Key(action),
        Step::Key(fight),
        Step::Key(other),
        Step::Key(wrapper_args),
        Step::Index(0),
        Step::Key(remember_args),
        Step::Index(0),
    ] = path
    else {
        return Ok(None);
    };
    if args != "args"
        || action != "action"
        || fight != "Fight"
        || other != "second"
        || wrapper_args != "args"
        || remember_args != "args"
    {
        return Ok(None);
    }
    let Some(operation) = at(root, operation_path) else {
        return Ok(None);
    };
    let wrapped = &operation["args"]["action"]["Fight"]["second"];
    let remembered = &wrapped["args"][0];
    let node = &remembered["args"][0];
    if operation["kind"] != "Keyword"
        || operation["args"]["action"].get("Fight").is_none()
        || symbol_name(wrapped) != Some("fight_prior_counterpart")
        || symbol_name(remembered) != Some("remember_operand")
        || symbol_name(node) != Some("qualified_object")
        || node["args"][0] != "Permanent"
        || node["args"][1] != serde_json::json!({"kind":"CardType","args":"Creature"})
    {
        return Ok(None);
    }
    if path.len() > 128 {
        return Err("fight counterpart reference depth exceeds128".into());
    }
    let mut actor = references::bound_reference(
        &operation["args"]["action"]["Fight"]["first"],
        introduced,
        scope.target_base,
    )?;
    super::filters::strip(&mut actor);
    let Ok(ObjectRef::Target { slot, index: 0 }) = serde_json::from_value(actor) else {
        return Ok(None);
    };
    let mut actor_path = operation_path.to_vec();
    actor_path.extend(["args", "action", "Fight", "first"].map(|key| Step::Key(key.into())));
    let Some(prior) = contexts.target_actor(&actor_path, usize::from(slot.0)) else {
        return Ok(None);
    };
    let earlier = qualified::resolve(node, prior, false)?;
    // The existing noun-domain proof must establish one earlier creature.
    // A source/event fallback alone does not justify a changed antecedent.
    if !qualified::known_scalar(prior, &earlier, &ObjectKind::Permanent, &node["args"][1]) {
        return Ok(None);
    }
    let _: ObjectRef = serde_json::from_value(earlier.clone())
        .map_err(|why| format!("fight counterpart must be a bound object: {why}"))?;
    Ok(Some(earlier))
}

fn at<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get(key)?,
            Step::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn target(slot: usize) -> Value {
        json!({"kind":"Target","args":{"slot":slot,"index":0}})
    }

    fn noun() -> Value {
        json!({"$sym":"qualified_object","args":["Permanent",{"kind":"CardType","args":"Creature"}]})
    }

    fn input(actor: Value) -> (Value, Vec<Step>) {
        (
            json!({"kind":"Keyword","args":{"action":{"Fight":{"first":actor,"second":{"$sym":"fight_prior_counterpart","args":[{
                    "$sym":"remember_operand","args":[noun()]}]}}}}}),
            vec![
                Step::Key("args".into()),
                Step::Key("action".into()),
                Step::Key("Fight".into()),
                Step::Key("second".into()),
                Step::Key("args".into()),
                Step::Index(0),
                Step::Key("args".into()),
                Step::Index(0),
            ],
        )
    }

    fn prior(slot: usize) -> Scope {
        let mut scope = Scope {
            named_object: Some(target(slot)),
            objects: vec![target(slot)],
            ..Scope::default()
        };
        qualified::target(
            &mut scope,
            target(slot),
            &json!({
            "kind":{"Object":{"kind":"Permanent","zones":["Battlefield"]}},
            "count":{"Exactly":{"kind":"Constant","args":1}},
            "filter":{"kind":"CardType","args":"Creature"}}),
        )
        .unwrap();
        scope
    }

    fn contexts(scope: &Scope, slot: usize) -> references::Contexts {
        let mut result = references::Contexts::default();
        result.remember(
            &[
                Step::Key("args".into()),
                Step::Key("action".into()),
                Step::Key("Fight".into()),
                Step::Key("first".into()),
                Step::Key("args".into()),
                Step::Key("slot".into()),
            ],
            scope,
            slot,
        );
        result
    }

    #[test]
    fn owning_new_fighter_reads_exact_pre_target_scope_after_offsets() {
        for offset in [0, 1, 6] {
            let scope = prior(offset);
            let (root, path) = input(target(offset + 1));
            assert_eq!(
                reference(&root, &path, &[], &scope, &contexts(&scope, offset + 1)).unwrap(),
                Some(target(offset))
            );
        }
    }

    #[test]
    fn another_operation_or_non_target_actor_does_not_capture_this_context() {
        let scope = prior(0);
        let contexts = contexts(&scope, 1);
        let (mut root, path) = input(target(1));
        root["kind"] = json!("DealDamage");
        assert_eq!(
            reference(&root, &path, &[], &scope, &contexts).unwrap(),
            None
        );
        let (root, path) = input(json!({"kind":"Source"}));
        assert_eq!(
            reference(&root, &path, &[], &scope, &contexts).unwrap(),
            None
        );
        let (root, path) = input(target(2));
        assert_eq!(
            reference(&root, &path, &[], &scope, &contexts).unwrap(),
            None
        );
    }

    #[test]
    fn source_fallback_and_other_noun_domains_do_not_prove_the_rule() {
        let scope = Scope::default();
        let (mut root, path) = input(target(1));
        assert!(reference(&root, &path, &[], &scope, &contexts(&scope, 1)).is_err());
        let scope = prior(0);
        root["args"]["action"]["Fight"]["second"]["args"][0]["args"][0]["args"][1] =
            json!({"kind":"CardType","args":"Artifact"});
        assert_eq!(
            reference(&root, &path, &[], &scope, &contexts(&scope, 1)).unwrap(),
            None
        );
    }

    #[test]
    fn lexical_role_annotations_have_checked_forward_and_inverse_object_types() {
        use crate::model::ir_schema::{Schema, Ty};
        use crate::pattern::{Bindings, elaborate, parse};
        let schema = Schema::of_ir().unwrap();
        for name in ["fight_prior_counterpart", "reflexive_moved_fight_object"] {
            let (_, syntax) = parse(&format!("@{name}(ObjectRef::Source)")).unwrap();
            let pattern = elaborate(&syntax, &Ty::Named("ObjectRef".into()), &schema).unwrap();
            assert!(
                !pattern
                    .unify(&json!({"kind":"Source"}), Bindings::new())
                    .is_empty()
            );
            assert!(pattern.unify(&target(0), Bindings::new()).is_empty());
            for ty in ["Expr", "PlayerRef", "ObjectSet", "Operation"] {
                assert!(elaborate(&syntax, &Ty::Named(ty.into()), &schema).is_err());
            }
            for call in [format!("@{name}"), format!("@{name}(Source,Source)")] {
                let (_, syntax) = parse(&call).unwrap();
                assert!(elaborate(&syntax, &Ty::Named("ObjectRef".into()), &schema).is_err());
            }
        }
    }
}
