//! Only a literal reflexive damage recipient names the written damage actor.
use super::{Scope, Step, references::TargetIntroduction, symbol_name};
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

const SCALAR: &str = "literal_damage_itself";
const OBJECTS: &str = "literal_damage_itself_objects";
const INVERSE_VIEW: &str = "proven_damage_actor_recipient_view";

fn at<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get(key)?,
            Step::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}

/// Return the owning operation path length and whether this is an ObjectSet.
fn role(root: &Value, path: &[Step]) -> Option<(usize, bool)> {
    let marker = symbol_name(at(root, path)?)?;
    let (operation, objects) = match (marker, path) {
        (
            SCALAR,
            [
                operation @ ..,
                Step::Key(a),
                Step::Key(r),
                Step::Key(b),
                Step::Key(c),
            ],
        ) if a == "args" && r == "recipients" && b == "args" && c == "args" => (operation, false),
        (OBJECTS, [operation @ .., Step::Key(a), Step::Key(r), Step::Key(b)])
            if a == "args" && r == "recipients" && b == "args" =>
        {
            (operation, true)
        }
        _ => return None,
    };
    let node = at(root, operation)?;
    let recipients = &node["args"]["recipients"];
    (node["kind"] == "DealDamage"
        && recipients["kind"] == "Objects"
        && (objects || recipients["args"]["kind"] == "One"))
        .then_some((operation.len(), objects))
}

/// Restore non-damage uses before holder, target identity and ordinary context
/// capture, preserving their exact previous it/created-reference prototypes.
pub(super) fn prepare(root: &mut Value) -> Result<(), String> {
    fn collect(
        value: &Value,
        path: &mut Vec<Step>,
        out: &mut Vec<Vec<Step>>,
        work: &mut usize,
    ) -> Result<(), String> {
        if *work == 0 || path.len() > 128 {
            return Err("damage reflexive preparation work/depth limit".into());
        }
        *work -= 1;
        if matches!(symbol_name(value), Some(SCALAR | OBJECTS)) {
            if !matches!(value["args"].as_array().map(Vec::as_slice), Some([_])) {
                return Err("damage reflexive annotation arity".into());
            }
            if out.len() >= 4096 {
                return Err("damage reflexive annotation bound".into());
            }
            out.push(path.clone());
        }
        match value {
            Value::Object(fields) => {
                for (key, child) in fields {
                    path.push(Step::Key(key.clone()));
                    collect(child, path, out, work)?;
                    path.pop();
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    path.push(Step::Index(index));
                    collect(child, path, out, work)?;
                    path.pop();
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut annotations = Vec::new();
    collect(root, &mut Vec::new(), &mut annotations, &mut 131_072)?;
    let mut ordinary: Vec<_> = annotations
        .into_iter()
        .filter(|path| role(root, path).is_none())
        .collect();
    ordinary.sort_by_key(|path| std::cmp::Reverse(path.len()));
    for path in ordinary {
        let node = super::at_path(root, &path).ok_or("damage reflexive preparation path")?;
        *node = node["args"][0].take();
    }
    Ok(())
}

/// Mined semantic constructors can place a later written reference in source.
/// Only these provenance-marked recipients wait for ordinary actor bindings;
/// they do not establish pronoun focus or introduce targets themselves.
fn deferred_reference(root: &Value, path: &[Step]) -> bool {
    path.len() <= 128
        && (0..path.len())
            .rev()
            .any(|depth| role(root, &path[..depth]).is_some())
}

/// Keep lexical order within each pass; actual damage actors bind before their
/// provenance-marked reflexive recipients even when a recipe reorders holes.
pub(super) fn sort_references(root: &Value, found: &mut [super::Found]) {
    found.sort_by_key(|f| (deferred_reference(root, &f.path), f.at, f.order));
}

pub(super) fn bind_marker(
    root: &mut Value,
    scope: &Scope,
    path: &[Step],
    name: &str,
    introduced: &[TargetIntroduction],
) -> Result<bool, String> {
    if !matches!(name, "it" | "created_reference_single") {
        return Ok(false);
    }
    let Some(reference) = reference(root, path, introduced, scope)? else {
        return Ok(false);
    };
    // The source already has its owning incarnation. Do not apply ordinary
    // pronoun focus, created-batch expansion or movement rebinding again.
    *super::at_path(root, path).ok_or("damage reflexive binding path")? = reference;
    Ok(true)
}

fn reference(
    root: &Value,
    path: &[Step],
    introduced: &[TargetIntroduction],
    scope: &Scope,
) -> Result<Option<Value>, String> {
    if path.len() > 128 {
        return Err("damage reflexive reference depth limit".into());
    }
    for depth in (0..path.len()).rev() {
        let Some((operation_depth, objects)) = role(root, &path[..depth]) else {
            continue;
        };
        let mut leaf = path[..depth].to_vec();
        leaf.extend([Step::Key("args".into()), Step::Index(0)]);
        let mut node = at(root, &leaf).ok_or("damage reflexive operand")?;
        if !objects && symbol_name(node) == Some("member_count_it") {
            leaf.extend([Step::Key("args".into()), Step::Index(0)]);
            node = at(root, &leaf).ok_or("damage reflexive scalar")?;
        }
        let expected = if objects {
            "created_reference_single"
        } else {
            "it"
        };
        if leaf != path || symbol_name(node) != Some(expected) || node["args"] != json!([]) {
            return Ok(None);
        }
        let source =
            &at(root, &path[..operation_depth]).ok_or("damage reflexive owner")?["args"]["source"];
        crate::functions::branch_bounds(source)?;
        let mut actor = super::references::bound_reference(source, introduced, scope.target_base)?;
        super::filters::strip(&mut actor);
        serde_json::from_value::<crate::model::ir::ObjectRef>(actor.clone())
            .map_err(|why| format!("damage reflexive actor must already be bound: {why}"))?;
        if objects {
            return Ok(Some(json!({"kind":"One","args":actor})));
        }
        return Ok(Some(actor));
    }
    Ok(None)
}

/// A private inverse view preserves the exact scalar actor, including selected
/// result indices. It is never an ObjectSet or a bound-IR replacement.
pub(crate) fn inverse_view(value: &Value) -> bool {
    symbol_name(value) == Some(INVERSE_VIEW)
        && value.as_object().is_some_and(|fields| fields.len() == 2)
        && matches!(value["args"].as_array().map(Vec::as_slice), Some([_]))
        && crate::functions::branch_bounds(value).is_ok()
        && serde_json::from_value::<crate::model::ir::ObjectRef>(value["args"][0].clone()).is_ok()
}

pub(crate) fn inverse_candidate(value: &Value) -> bool {
    value.get("$sym").is_none() || inverse_view(value)
}

/// Add a spelling variant only where the current operation proves the scalar
/// damage recipient is its actual actor. Nested abilities own their own forms.
pub(super) fn surface(root: &mut Value) -> bool {
    if crate::functions::branch_bounds(root).is_err() {
        return false;
    }
    fn walk(value: &mut Value, first: bool, changed: &mut bool) {
        if !first
            && matches!(
                crate::traverse::boundary(value),
                crate::traverse::Boundary::Ability
                    | crate::traverse::Boundary::Holder
                    | crate::traverse::Boundary::Lexical
            )
        {
            return;
        }
        if value["kind"] == "DealDamage" {
            let source = &value["args"]["source"];
            let recipient = &value["args"]["recipients"];
            if recipient["kind"] == "Objects"
                && recipient["args"]["kind"] == "One"
                && recipient["args"]["args"] == *source
                && serde_json::from_value::<crate::model::ir::ObjectRef>(source.clone()).is_ok()
            {
                value["args"]["recipients"]["args"]["args"] =
                    json!({"$sym":INVERSE_VIEW,"args":[source]});
                *changed = true;
            }
            return;
        }
        crate::traverse::children(value, |child| walk(child, false, changed));
    }
    let mut changed = false;
    walk(root, true, &mut changed);
    changed
}

/// Ordinary literal itself uses exactly the ordinary pronoun inverse. Only the
/// operation-proven private view can add the damage actor spelling.
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else {
        return Vec::new();
    };
    if inverse_view(value) {
        return match inner {
            Pattern::Symbol(name, args) if name == "member_count_it" => match args.as_slice() {
                [Pattern::Symbol(name, args)] if name == "it" && args.is_empty() => vec![bindings],
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
    }
    inner.unify(value, bindings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(source: Value, objects: bool) -> (Value, Vec<Step>) {
        let (marker, inner) = if objects {
            (OBJECTS, "created_reference_single")
        } else {
            (SCALAR, "it")
        };
        let annotation = json!({"$sym":marker,"args":[{"$sym":inner,"args":[]}]});
        let recipient = if objects {
            annotation
        } else {
            json!({"kind":"One","args":annotation})
        };
        let root = json!({"kind":"DealDamage","args":{
            "source":source,"recipients":{"kind":"Objects","args":recipient}
        }});
        let mut path = vec![
            Step::Key("args".into()),
            Step::Key("recipients".into()),
            Step::Key("args".into()),
        ];
        if !objects {
            path.push(Step::Key("args".into()));
        }
        path.extend([Step::Key("args".into()), Step::Index(0)]);
        (root, path)
    }

    #[test]
    fn actual_bound_source_and_creator_are_independent_of_ordinary_focus() {
        for kind in ["Source", "GrantingObject", "EventSubject"] {
            let actor = json!({"kind":kind});
            for objects in [false, true] {
                let (root, path) = input(actor.clone(), objects);
                let expected = if objects {
                    json!({"kind":"One","args":actor})
                } else {
                    actor.clone()
                };
                assert_eq!(
                    reference(&root, &path, &[], &Scope::default()).unwrap(),
                    Some(expected)
                );
            }
        }
    }

    #[test]
    fn assigned_actor_target_address_is_not_announced_again() {
        let spec = json!({"slot":{"$sym":"slot","args":[],"at":7}});
        let pending = json!({"kind":"Target","args":{
            "slot":{"$sym":"target","args":[spec],"at":7},"index":0
        }});
        let (root, path) = input(pending, false);
        let introduced = vec![((7, spec.to_string()), 1)];
        let scope = Scope {
            target_base: 3,
            ..Scope::default()
        };
        assert_eq!(
            reference(&root, &path, &introduced, &scope).unwrap(),
            Some(json!({"kind":"Target","args":{"slot":4,"index":0}}))
        );
        assert!(reference(&root, &path, &[], &scope).is_err());
    }

    #[test]
    fn non_damage_and_nonrecipient_annotations_restore_exact_old_prototypes() {
        for original in [
            json!({"$sym":"it","args":[]}),
            json!({"$sym":"created_reference_single","args":[]}),
        ] {
            let marker = if original["$sym"] == "it" {
                SCALAR
            } else {
                OBJECTS
            };
            let mut value = json!({"kind":"Sacrifice","args":{
                "objects":{"$sym":marker,"args":[original]}
            }});
            prepare(&mut value).unwrap();
            assert_eq!(value["args"]["objects"], original);
        }
        let mut value = json!({"kind":"DealDamage","args":{
            "amount":{"$sym":SCALAR,"args":[{"$sym":"it","args":[]}]}
        }});
        prepare(&mut value).unwrap();
        assert_eq!(value["args"]["amount"]["$sym"], "it");
    }

    #[test]
    fn unavailable_oversized_and_nonobject_actors_never_guess() {
        for actor in [
            json!({"$sym":"self","args":[]}),
            json!({"kind":"AbilityController"}),
            json!({"kind":"Source","extra":"x".repeat(1024 * 1024 + 1)}),
        ] {
            let (root, path) = input(actor, false);
            assert!(reference(&root, &path, &[], &Scope::default()).is_err());
        }
    }

    #[test]
    fn scalar_actor_itself_never_expands_to_a_replacement_enlarged_created_batch() {
        let actor = json!({"kind":"Result","args":{"result":2,"phase":"After","index":0}});
        for objects in [false, true] {
            let (mut root, path) = input(actor.clone(), objects);
            let scope = Scope {
                created_result: Some((2, true)),
                ..Scope::default()
            };
            let expected = if objects {
                json!({"kind":"One","args":actor})
            } else {
                actor.clone()
            };
            assert_eq!(
                reference(&root, &path, &[], &scope).unwrap(),
                Some(expected.clone())
            );
            let name = if objects {
                "created_reference_single"
            } else {
                "it"
            };
            assert!(bind_marker(&mut root, &scope, &path, name, &[]).unwrap());
            assert_eq!(at(&root, &path), Some(&expected));
        }
    }

    #[test]
    fn later_symbolic_actor_binds_before_earlier_literal_reflexive_recipient() {
        let (mut root, recipient) = input(json!({"$sym":"it","args":[],"at":24}), true);
        let source = vec![Step::Key("args".into()), Step::Key("source".into())];
        assert!(!deferred_reference(&root, &source));
        assert!(deferred_reference(&root, &recipient));
        assert!(reference(&root, &recipient, &[], &Scope::default()).is_err());
        let actor = json!({"kind":"Target","args":{"slot":0,"index":0}});
        root["args"]["source"] = actor.clone();
        assert_eq!(
            reference(&root, &recipient, &[], &Scope::default()).unwrap(),
            Some(json!({"kind":"One","args":actor}))
        );
    }

    fn reflexive_pattern() -> Pattern {
        Pattern::Symbol(
            SCALAR.into(),
            vec![Pattern::Symbol(
                "member_count_it".into(),
                vec![Pattern::Symbol("it".into(), vec![])],
            )],
        )
    }

    #[test]
    fn inverse_damage_view_preserves_exact_scalar_identity_and_requires_the_current_pair() {
        for actor in [
            json!({"kind":"Source"}),
            json!({"kind":"GrantingObject"}),
            json!({"kind":"Target","args":{"slot":3,"index":2}}),
            json!({"kind":"Result","args":{"result":7,"phase":"After","index":0}}),
            json!({"kind":"Result","args":{"result":7,"phase":"After","index":4}}),
        ] {
            let original = json!({"kind":"DealDamage","args":{"source":actor,
                "recipients":{"kind":"Objects","args":{"kind":"One","args":actor}}}});
            let mut marked = original.clone();
            assert!(surface(&mut marked));
            assert_eq!(marked["args"]["source"], actor);
            let view = &marked["args"]["recipients"]["args"]["args"];
            assert!(inverse_view(view));
            assert_eq!(view["args"][0], actor);
            assert!(!reflexive_pattern().unify(view, Bindings::new()).is_empty());
            for field in ["source", "recipient"] {
                let mut edited = original.clone();
                let other = json!({"kind":"Target","args":{"slot":99,"index":0}});
                if field == "source" {
                    edited["args"]["source"] = other;
                } else {
                    edited["args"]["recipients"]["args"]["args"] = other;
                }
                let before = edited.clone();
                assert!(!surface(&mut edited));
                assert_eq!(edited, before);
            }
        }
        // An ordinary Granter operand is never a proof of reflexive actorhood.
        assert!(
            reflexive_pattern()
                .unify(&json!({"kind":"GrantingObject"}), Bindings::new())
                .is_empty()
        );
        for operation in ["Move", "PutCounters", "GainLife"] {
            let mut other = json!({"kind":operation,"args":{"source":{"kind":"GrantingObject"},
                "recipients":{"kind":"Objects","args":{"kind":"One","args":{"kind":"GrantingObject"}}}}});
            let before = other.clone();
            assert!(!surface(&mut other));
            assert_eq!(other, before);
        }
        let mut batch = json!({"kind":"DealDamage","args":{"source":{"kind":"Result","args":{"result":7,"phase":"After","index":0}},
            "recipients":{"kind":"Objects","args":{"kind":"Transition","args":{"result":7,"phase":"After"}}}}});
        let before = batch.clone();
        assert!(!surface(&mut batch));
        assert_eq!(batch, before);
        assert!(!inverse_view(&json!({"$sym":INVERSE_VIEW,"args":[batch]})));
        let mut deep = Value::Null;
        for _ in 0..130 {
            deep = json!([deep]);
        }
        assert!(!surface(&mut deep));
    }

    #[test]
    fn granted_damage_itself_original_text_remains_available_from_expected_ir() {
        use crate::engine::canonical_abilities;
        let engine = crate::test_support::engine();
        let family: Value = serde_json::from_str(include_str!(
            "../../families/player-event-object-bindings.json"
        ))
        .unwrap();
        let case = &family["cases"][10];
        assert_eq!(case["name"], "Damage Mentor");
        let expected = canonical_abilities(&case["expected"]).unwrap();
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let trail = engine
            .trail_of(name, &expected, text)
            .unwrap()
            .expect("actual creator reflexive is reachable");
        let printed = engine
            .print_trail(name, &expected, &trail)
            .unwrap()
            .expect("actual creator reflexive verifies");
        assert!(printed.contains("itself"));
        assert!(
            engine
                .parse(name, &printed)
                .unwrap()
                .readings
                .contains(&expected)
        );
    }
}
