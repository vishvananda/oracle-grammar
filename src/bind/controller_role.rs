//! Lexically introduced controller roles are separate from ordinary player focus.
use super::{Scope, Step, per_recipient, references, symbol_name};
use crate::model::ir::{ObjectRef, PlayerRef};
use serde_json::{Value, json};

mod literal;

pub(super) fn default_player(root: &Value, scope: &Scope) -> bool {
    literal::seed(root, scope)
}

pub(super) fn literal_reference(
    root: &Value,
    path: &[Step],
    introduced: &[references::TargetIntroduction],
    scope: &Scope,
    contexts: &references::Contexts,
) -> Result<Value, String> {
    literal::reference(root, path, introduced, scope, contexts)
}

#[cfg(test)]
mod literal_tests;

#[derive(Clone)]
pub(super) struct Role {
    object: ObjectRef,
    /// The player stack is append-only within this lexical scope. A newer
    /// explicit player introduction invalidates this indefinite role.
    player_generation: usize,
    literal: bool,
}

pub(super) fn introduce(scope: &mut Scope, reference: &Value, spec: &Value) -> Result<(), String> {
    let domain = &spec["kind"]["Object"];
    if domain["kind"] != "Permanent"
        || domain["zones"] != json!(["Battlefield"])
        || !["Exactly", "UpTo"]
            .iter()
            .any(|tag| spec["count"][tag] == json!({"kind":"Constant","args":1}))
        || !proven(&spec["filter"], &mut 131_072, 0)?
    {
        return Ok(());
    }
    let object = serde_json::from_value(reference.clone())
        .map_err(|why| format!("indefinite controller object: {why}"))?;
    scope.controller_role = Some(Role {
        object,
        player_generation: scope.players.len(),
        literal: literal::scalar_spec(spec),
    });
    Ok(())
}

/// Only positive conjunctions assert the lexical relationship. Negation,
/// alternatives and nested object predicates cannot introduce this role.
pub(super) fn proven(filter: &Value, work: &mut usize, depth: usize) -> Result<bool, String> {
    if depth > 128 || *work == 0 {
        return Err("controller provenance work/depth limit".into());
    }
    *work -= 1;
    if symbol_name(filter) == Some("indefinite_opponent_controller") {
        return Ok(filter["args"][0]
            == json!({"kind":"ControlledBy","args":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}}}));
    }
    if filter["kind"] == "AllOf"
        && let Some(parts) = filter["args"].as_array()
    {
        let mut found = false;
        for part in parts {
            found |= proven(part, work, depth + 1)?;
        }
        return Ok(found);
    }
    Ok(false)
}

pub(super) fn reference(
    root: &Value,
    path: &[Step],
    introduced: &[references::TargetIntroduction],
    scope: &Scope,
    contexts: &references::Contexts,
) -> Result<Value, String> {
    // Per-recipient/loop references own their lexical region before ordinary
    // antecedents are considered, just as they do for “that player”.
    if let Some(player) = per_recipient::player(root, path, introduced, scope.target_base)? {
        return Ok(json!(player));
    }
    let prior = contexts.before(path, scope);
    if let Some(player) = loop_player(root, path, prior, contexts)? {
        return Ok(json!(player));
    }
    if let Some(role) = &prior.controller_role
        && role.player_generation == prior.players.len()
    {
        return Ok(json!(PlayerRef::ControllerOf(Box::new(
            role.object.clone()
        ))));
    }
    prior
        .player()
        .ok_or_else(|| "no player for this opponent reference".into())
}

/// A loop's body explicitly introduces its own player. Its population/header
/// still reads the enclosing context. Only this opponent marker uses the hook.
fn loop_player(
    root: &Value,
    path: &[Step],
    scope: &Scope,
    contexts: &references::Contexts,
) -> Result<Option<PlayerRef>, String> {
    if path.len() > 128 {
        return Err("controller loop context depth exceeds 128".into());
    }
    let mut node = root;
    let mut player = None;
    for (index, step) in path.iter().enumerate() {
        if node["kind"] == "ForEachPlayer"
            && node["args"]["players"]["kind"] == "OpponentsOf"
            && matches!(path.get(index), Some(Step::Key(key)) if key == "args")
            && matches!(path.get(index + 1), Some(Step::Key(key)) if key == "body")
        {
            let binding = serde_json::from_value(node["args"]["binding"].clone())
                .map_err(|why| format!("controller loop binding: {why}"))?;
            player = Some(PlayerRef::Bound(binding));
            if let Some(current) = scope.player()
                && current["kind"] == "Target"
                && current["args"]["index"] == 0
                && let Some(slot) = current["args"]["slot"]
                    .as_u64()
                    .and_then(|slot| usize::try_from(slot).ok())
                && contexts.target_actor(&path[..index + 2], slot).is_some()
            {
                player = Some(
                    serde_json::from_value(current)
                        .map_err(|why| format!("local loop player: {why}"))?,
                );
            }
        }
        node = match step {
            Step::Key(key) => node.get(key),
            Step::Index(index) => node.get(*index),
        }
        .ok_or("controller loop context path")?;
    }
    Ok(player)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Value {
        json!({"kind":{"Object":{"kind":"Permanent","zones":["Battlefield"]}},"count":{"Exactly":{"kind":"Constant","args":1}},"filter":{"kind":"AllOf","args":[{"kind":"CardType","args":"Creature"},
                {"$sym":"indefinite_opponent_controller","args":[
                    {"kind":"ControlledBy","args":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}}}
                ],"at":3}]}})
    }

    fn object(slot: u64) -> Value {
        json!({"kind":"Target","args":{"slot":slot,"index":0}})
    }

    fn resolve(scope: &Scope) -> Result<Value, String> {
        reference(
            &Value::Null,
            &[],
            &[],
            scope,
            &references::Contexts::default(),
        )
    }

    #[test]
    fn role_uses_actual_scalar_target_without_replacing_ordinary_event_player() {
        let player = json!({"kind":"EventSubject"});
        let mut scope = Scope::default();
        scope.players.push(player.clone());
        introduce(&mut scope, &object(7), &spec()).unwrap();
        assert_eq!(scope.player(), Some(player));
        assert_eq!(
            resolve(&scope).unwrap(),
            json!({"kind":"ControllerOf","args":object(7)})
        );
        scope.players.push(json!({"kind":"Bound","args":2}));
        assert_eq!(resolve(&scope).unwrap(), json!({"kind":"Bound","args":2}));
    }

    #[test]
    fn only_lexically_proven_positive_scalar_battlefield_relationships_bind() {
        let original = spec();
        let filter = original["filter"].clone();
        let mut proposals = vec![original.clone(); 5];
        proposals[0]["count"] = json!({"UpTo":{"kind":"Constant","args":2}});
        proposals[1]["kind"]["Object"]["zones"] = json!(["Graveyard"]);
        proposals[2]["filter"] = json!({"kind":"Not","args":filter});
        proposals[3]["filter"] = json!({"kind":"AnyOf","args":[filter]});
        proposals[4]["filter"] = json!({"kind":"ControlledBy","args":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}}});
        for proposal in proposals {
            let mut scope = Scope::default();
            introduce(&mut scope, &object(7), &proposal).unwrap();
            assert!(scope.controller_role.is_none());
        }
    }

    #[test]
    fn target_filter_cannot_capture_its_own_controller_role() {
        let mut scope = Scope::default();
        let mut contexts = references::Contexts::default();
        let target_path = [Step::Key("target".into())];
        contexts.remember(&target_path, &scope, 5);
        introduce(&mut scope, &object(5), &spec()).unwrap();
        let path = [Step::Key("target".into()), Step::Key("filter".into())];
        let root = json!({"target":{"filter":null}});
        assert!(reference(&root, &path, &[], &scope, &contexts).is_err());
        assert_eq!(
            resolve(&scope).unwrap(),
            json!({"kind":"ControllerOf","args":object(5)})
        );
    }

    #[test]
    fn work_and_depth_are_bounded() {
        assert!(proven(&spec()["filter"], &mut 0, 0).is_err());
        assert!(proven(&spec()["filter"], &mut 1000, 129).is_err());
    }

    #[test]
    fn provenance_stays_transparent_until_target_identity_helpers_have_finished() {
        let mut root = json!({"target":{"$sym":"target","at":1,"args":[spec()]}});
        let pending = root["target"].clone();
        let mut found = Vec::new();
        super::super::references::collect(
            &root,
            &mut Vec::new(),
            &mut found,
            super::super::filters::Context::Outside,
            &mut false,
        );
        assert_eq!(found.len(), 1, "annotation is not a reference occurrence");
        let mut scope = Scope {
            target_base: 8,
            ..Scope::default()
        };
        let mut introduced = Vec::new();
        references::introduce(
            &mut root,
            &mut scope,
            &found[0],
            &mut introduced,
            &mut Vec::new(),
            &mut references::Contexts::default(),
        )
        .unwrap();
        assert_eq!(root["target"], pending, "target identity remains immutable");
        assert_eq!(
            references::bound_reference(&root["target"], &introduced, 8).unwrap(),
            json!(8)
        );
        let mut filter = root["target"]["args"][0]["filter"].clone();
        super::super::filters::strip(&mut filter);
        assert!(
            !filter
                .to_string()
                .contains("indefinite_opponent_controller")
        );
        assert_eq!(filter["args"][1]["kind"], "ControlledBy");
    }

    #[test]
    fn optional_scalar_target_proves_a_role_but_optional_group_does_not() {
        let mut optional = spec();
        optional["count"] = json!({"UpTo":{"kind":"Constant","args":1}});
        let mut scope = Scope::default();
        introduce(&mut scope, &object(7), &optional).unwrap();
        assert_eq!(
            resolve(&scope).unwrap(),
            json!({"kind":"ControllerOf","args":object(7)})
        );
        optional["count"] = json!({"UpTo":{"kind":"Constant","args":2}});
        let mut scope = Scope::default();
        introduce(&mut scope, &object(7), &optional).unwrap();
        assert!(scope.controller_role.is_none());
    }

    #[test]
    fn actual_loop_body_overrides_outer_role_but_header_retains_it() {
        let mut scope = Scope::default();
        introduce(&mut scope, &object(7), &spec()).unwrap();
        let root = json!({"kind":"ForEachPlayer","args":{
            "binding":0, "players":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}}, "body":null
        }});
        let context = references::Contexts::default();
        let body = [Step::Key("args".into()), Step::Key("body".into())];
        let header = [Step::Key("args".into()), Step::Key("players".into())];
        assert_eq!(
            reference(&root, &body, &[], &scope, &context).unwrap(),
            json!({"kind":"Bound","args":0})
        );
        assert_eq!(
            reference(&root, &header, &[], &scope, &context).unwrap(),
            json!({"kind":"ControllerOf","args":object(7)})
        );
    }

    #[test]
    fn all_player_loop_keeps_older_opponent_role_and_opponent_loop_prefers_only_local_new_player() {
        let mut scope = Scope::default();
        introduce(&mut scope, &object(7), &spec()).unwrap();
        let mut root = json!({"kind":"ForEachPlayer","args":{
            "binding":0, "players":{"kind":"All"}, "body":{"new_target":null}
        }});
        let body = [Step::Key("args".into()), Step::Key("body".into())];
        let mut contexts = references::Contexts::default();
        assert_eq!(
            reference(&root, &body, &[], &scope, &contexts).unwrap(),
            json!({"kind":"ControllerOf","args":object(7)})
        );
        root["args"]["players"] = json!({"kind":"OpponentsOf","args":{"kind":"AbilityController"}});
        contexts.remember(&[Step::Key("outside".into())], &scope, 8);
        scope.players.push(object(8));
        assert_eq!(
            reference(&root, &body, &[], &scope, &contexts).unwrap(),
            json!({"kind":"Bound","args":0})
        );
        let local = [
            Step::Key("args".into()),
            Step::Key("body".into()),
            Step::Key("new_target".into()),
        ];
        contexts.remember(&local, &scope, 9);
        scope.players.push(object(9));
        assert_eq!(
            reference(&root, &body, &[], &scope, &contexts).unwrap(),
            object(9)
        );
    }

    #[test]
    fn lexical_opponent_contract_has_checked_types_arity_and_native_inverse_values() {
        use crate::model::ir_schema::{Schema, Ty};
        use crate::pattern::{Bindings, elaborate, parse};
        let schema = Schema::of_ir().unwrap();
        let (_, syntax) = parse("@that_opponent").unwrap();
        let player = Ty::Named("PlayerRef".into());
        let pattern = elaborate(&syntax, &player, &schema).unwrap();
        for value in [
            json!({"kind":"AbilityController"}),
            json!({"kind":"Bound","args":2}),
            json!({"kind":"ControllerOf","args":object(7)}),
        ] {
            assert!(!pattern.unify(&value, Bindings::new()).is_empty());
        }
        for value in [json!({"kind":"Source"}), json!(0), Value::Null] {
            assert!(pattern.unify(&value, Bindings::new()).is_empty());
        }
        for ty in ["ObjectRef", "ObjectFilter", "Expr", "Operation"] {
            assert!(elaborate(&syntax, &Ty::Named(ty.into()), &schema).is_err());
        }
        let (_, args) = parse("@that_opponent(AbilityController)").unwrap();
        assert!(elaborate(&args, &player, &schema).is_err());
        let (_, annotation) = parse(
            "@indefinite_opponent_controller(ObjectFilter::ControlledBy(OpponentsOf(AbilityController)))",
        ).unwrap();
        let filter = Ty::Named("ObjectFilter".into());
        let wrapper = elaborate(&annotation, &filter, &schema).unwrap();
        assert!(
            !wrapper
                .unify(&spec()["filter"]["args"][1]["args"][0], Bindings::new())
                .is_empty()
        );
        assert!(elaborate(&annotation, &player, &schema).is_err());
        for text in [
            "@indefinite_opponent_controller",
            "@indefinite_opponent_controller(Any,Any)",
        ] {
            let (_, bad) = parse(text).unwrap();
            assert!(elaborate(&bad, &filter, &schema).is_err());
        }
    }
}
