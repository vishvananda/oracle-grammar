//! Reject an unproved legacy holder/event alias after an explicitly revealed card.
//! No operand is retargeted: the complete proper reading owns its reveal receipt
//! and later value snapshot, not a fresh read of the next library card.
use super::{Scope, Step, qualified, symbol_name};
use serde_json::{Value, json};

fn at<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get(key)?,
            Step::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}
fn keys(path: &[Step], names: &[&str]) -> bool {
    path.len() == names.len()
        && path
            .iter()
            .zip(names)
            .all(|(step, name)| matches!(step, Step::Key(key) if key.as_str() == *name))
}
fn scalar_suffix(path: &[Step], objects: &Value) -> bool {
    if objects["kind"] == "One" {
        return keys(path, &["args"]);
    }
    symbol_name(objects) == Some("exile_followup")
        && objects["args"]
            .as_array()
            .is_some_and(|a| a.len() == 1 && a[0]["kind"] == "One")
        && path.len() == 3
        && keys(&path[..1], &["args"])
        && path[1] == Step::Index(0)
        && keys(&path[2..], &["args"])
}
struct Proof {
    result: Option<u32>,
}
fn eligible(root: &Value, path: &[Step]) -> Result<Option<Proof>, String> {
    if !matches!(
        root["kind"].as_str(),
        Some("Spell" | "Activated" | "Triggered")
    ) || path.len() < 9
        || !keys(&path[..3], &["args", "body", "instructions"])
        || !keys(&path[4..8], &["args", "operation", "args", "objects"])
    {
        return Ok(None);
    }
    let Step::Index(index) = path[3] else {
        return Ok(None);
    };
    if index == 0 {
        return Ok(None);
    }
    let Some(instructions) = root
        .pointer("/args/body/instructions")
        .and_then(Value::as_array)
    else {
        return Ok(None);
    };
    let Some(effect) = instructions.get(index) else {
        return Ok(None);
    };
    if effect["kind"] != "Effect"
        || effect["args"]["operation"]["kind"] != "Move"
        || !scalar_suffix(&path[8..], &effect["args"]["operation"]["args"]["objects"])
    {
        return Ok(None);
    }
    let Some(reference) = at(root, path) else {
        return Ok(None);
    };
    if symbol_name(reference) != Some("current_named_object") || reference["args"] != json!([]) {
        return Ok(None);
    }
    let previous = &instructions[index - 1];
    if previous["kind"] != "Effect" || previous["args"]["operation"]["kind"] != "Reveal" {
        return Ok(None);
    }
    let operation = &previous["args"]["operation"];
    crate::functions::branch_bounds(operation)?;
    if operation["args"]["objects"]
        != json!({"kind":"Take","args":{
        "objects":{"kind":"Zone","args":{"scope":{"Players":{"zone":"Library","players":{"kind":"One","args":{"kind":"AbilityController"}}}},"kind":"Card","filter":{"kind":"Any"}}},
        "count":{"kind":"Constant","args":1},"from":"Top"}})
    {
        return Ok(None);
    }
    Ok(Some(Proof {
        result: previous["args"]["result"]
            .as_u64()
            .and_then(|id| u32::try_from(id).ok()),
    }))
}

pub(super) fn resolve(
    root: &Value,
    path: &[Step],
    own: bool,
    scope: &Scope,
) -> Result<Value, String> {
    // Decide ownership/shape before invoking the existing reference clone path.
    let proof = if own { eligible(root, path)? } else { None };
    let reference = qualified::card(scope)?;
    if let Some(proof) = proof {
        let same = proof.result.is_some_and(|result| {
            reference
                == json!({"kind":"Query","args":{
            "objects":{"kind":"Observed","args":result},"index":0}})
        });
        if !same {
            return Err(
                "that card after revealing one library card needs that actual reveal receipt"
                    .into(),
            );
        }
    }

    Ok(reference)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Value, Vec<Step>, Scope) {
        let root = json!({"kind":"Triggered","args":{"body":{"instructions":[{"kind":"Effect","args":{"result":17,"operation":{"kind":"Reveal","args":{"objects":{"kind":"Take","args":{"objects":{"kind":"Zone","args":{"scope":{"Players":{"zone":"Library","players":{"kind":"One","args":{"kind":"AbilityController"}}}},"kind":"Card","filter":{"kind":"Any"}}},"count":{"kind":"Constant","args":1},"from":"Top"}}}}}},{"kind":"Effect","args":{"result":18,"operation":{"kind":"Move","args":{"objects":{"kind":"One","args":{"$sym":"current_named_object","args":[],"at":10}},"destination":{"OwnersHand":{"counters":[],"states":[]}},"qualifiers":[]}}}}]}}});
        (
            root,
            steps("/args/body/instructions/1/args/operation/args/objects/args"),
            Scope {
                objects: vec![json!({"kind":"Source"})],
                root_card_scope: true,
                ..Scope::default()
            },
        )
    }
    fn steps(pointer: &str) -> Vec<Step> {
        pointer
            .trim_start_matches('/')
            .split('/')
            .map(|s| s.parse().map_or_else(|_| Step::Key(s.into()), Step::Index))
            .collect()
    }
    #[test]
    fn holder_and_permanent_event_fallbacks_refuse_without_operand_rewrite() {
        let (root, path, mut scope) = fixture();
        let unchanged = root.clone();
        for kind in ["Source", "EventSubject", "GrantingObject"] {
            scope.objects = vec![json!({"kind":kind})];
            assert!(resolve(&root, &path, true, &scope).is_err());
        }
        assert_eq!(root, unchanged);
    }
    #[test]
    fn exact_unary_action_envelope_retains_lexical_card_proof() {
        let (mut root, _, scope) = fixture();
        let objects = root
            .pointer_mut("/args/body/instructions/1/args/operation/args/objects")
            .unwrap();
        *objects = json!({"$sym":"exile_followup","args":[objects.clone()],"at":9});
        let path = steps("/args/body/instructions/1/args/operation/args/objects/args/0/args");
        assert!(resolve(&root, &path, true, &scope).is_err());
        root.pointer_mut("/args/body/instructions/1/args/operation/args/objects/args")
            .unwrap()
            .as_array_mut()
            .unwrap()
            .push(json!({"kind":"Any"}));
        assert_eq!(
            resolve(&root, &path, true, &scope).unwrap(),
            json!({"kind":"Source"})
        );
    }
    #[test]
    fn only_the_exact_previous_reveal_receipt_proves_marker_identity() {
        let (root, path, mut scope) = fixture();
        let right =
            json!({"kind":"Query","args":{"objects":{"kind":"Observed","args":17},"index":0}});
        scope.objects = vec![right.clone()];
        assert_eq!(resolve(&root, &path, true, &scope).unwrap(), right);
        for reference in [
            json!({"kind":"Query","args":{"objects":{"kind":"Observed","args":16},"index":0}}),
            json!({"kind":"Target","args":{"slot":2,"index":0}}),
            json!({"kind":"EventSubject"}),
        ] {
            scope.objects = vec![reference.clone()];
            scope.references.push(qualified::event(
                reference,
                &json!({"kind":"Any"}),
                true,
                false,
            ));
            assert!(resolve(&root, &path, true, &scope).is_err());
        }
        let mut explicit = root.clone();
        let target = json!({"kind":"Target","args":{"slot":2,"index":0}});
        *explicit
            .pointer_mut("/args/body/instructions/1/args/operation/args/objects/args")
            .unwrap() = target.clone();
        scope.objects = vec![target.clone()];
        assert_eq!(resolve(&explicit, &path, true, &scope).unwrap(), target);
    }
    #[test]
    fn unsupported_producers_and_explicit_words_keep_old_behavior() {
        let (root, path, scope) = fixture();
        for pointer in [
            "/args/body/instructions/0/args/operation/args/objects/args/count/args",
            "/args/body/instructions/0/args/operation/args/objects/args/objects/args/scope/Players/players/args/kind",
        ] {
            let mut variant = root.clone();
            *variant.pointer_mut(pointer).unwrap() = if pointer.ends_with("kind") {
                json!("EventSubject")
            } else {
                json!(2)
            };
            assert_eq!(
                resolve(&variant, &path, true, &scope).unwrap(),
                json!({"kind":"Source"})
            );
        }
        let mut variant = root.clone();
        variant
            .pointer_mut("/args/body/instructions/1/args/operation/args/objects/args/$sym")
            .unwrap()
            .clone_from(&json!("self"));
        assert_eq!(
            resolve(&variant, &path, true, &scope).unwrap(),
            json!({"kind":"Source"})
        );
        assert_eq!(
            resolve(&root, &path, false, &scope).unwrap(),
            json!({"kind":"Source"})
        );
        assert!(!scope.nested().root_card_scope);
    }
    #[test]
    fn intervening_action_and_nested_programs_cannot_borrow_the_proof() {
        let (mut root, _, scope) = fixture();
        let instructions = root
            .pointer_mut("/args/body/instructions")
            .unwrap()
            .as_array_mut()
            .unwrap();
        instructions.insert(
            1,
            json!({"kind":"Effect","args":{"result":99,"operation":{"kind":"GainLife"}}}),
        );
        let path = steps("/args/body/instructions/2/args/operation/args/objects/args");
        assert_eq!(
            resolve(&root, &path, true, &scope).unwrap(),
            json!({"kind":"Source"})
        );
        let (inner, _, _) = fixture();
        let wrapped = json!({"kind":"Triggered","args":{"body":{"instructions":[{"kind":"If","args":{"then":inner["args"]["body"]}}]}}});
        let path = steps(
            "/args/body/instructions/0/args/then/instructions/1/args/operation/args/objects/args",
        );
        assert_eq!(
            resolve(&wrapped, &path, true, &scope).unwrap(),
            json!({"kind":"Source"})
        );
    }
    #[test]
    fn oversized_candidate_producer_fails_closed_before_reference_clone() {
        let (mut root, path, scope) = fixture();
        let mut deep = json!(null);
        for _ in 0..130 {
            deep = json!([deep]);
        }
        root["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["extra"] = deep;
        assert!(resolve(&root, &path, true, &scope).is_err());
    }
}
