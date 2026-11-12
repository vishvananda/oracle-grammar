//! A resolving spell is never one of the permanents its `other` names. After an
//! instruction that changed a group, as in In Oketra's Name, `other creatures`
//! contrasts with that group; with no such group the spell keeps its fallback.
use serde_json::{Value, json};

const AFFECTED: &str = "/args/operation/args/affected/Objects";

fn fallback() -> Value {
    json!({"kind":"Not","args":{"kind":"SameObject","args":{"kind":"Source"}}})
}

/// The filter of a battlefield group a continuous effect changes.
fn group(instruction: &Value) -> Option<&Value> {
    let zone = instruction.pointer(AFFECTED)?;
    (instruction["kind"] == "Effect"
        && instruction["args"]["operation"]["kind"] == "CreateContinuousEffect"
        && zone["kind"] == "Zone"
        && zone["args"]["scope"] == json!({"Shared":"Battlefield"})
        && zone["args"]["kind"] == "Permanent")
        .then(|| &zone["args"]["filter"])
}

fn parts(filter: &Value) -> &[Value] {
    match filter["args"].as_array() {
        Some(parts) if filter["kind"] == "AllOf" => parts,
        _ => std::slice::from_ref(filter),
    }
}

/// Rewrite the one `from` conjunct of each later group that follows a group;
/// a group that is itself an `other` contrast supplies nothing.
fn contrast(
    ability: &mut Value,
    from: impl Fn(&Value) -> Value,
    to: impl Fn(&Value) -> Value,
) -> bool {
    if ability["kind"] != "Spell" {
        return false;
    }
    let Some(instructions) = ability
        .pointer_mut("/args/body/instructions")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for index in 1..instructions.len().min(64) {
        let (before, after) = instructions.split_at_mut(index);
        let Some(prior) = group(&before[index - 1])
            .filter(|prior| !parts(prior).contains(&fallback()))
            .cloned()
        else {
            continue;
        };
        if group(&after[0]).is_none() {
            continue;
        }
        let Some(conjuncts) = after[0]
            .pointer_mut(AFFECTED)
            .filter(|zone| zone["args"]["filter"]["kind"] == "AllOf")
            .and_then(|zone| zone["args"]["filter"]["args"].as_array_mut())
        else {
            continue;
        };
        let (old, new) = (from(&prior), to(&prior));
        let mut found = conjuncts.iter_mut().filter(|part| **part == old);
        if let (Some(part), None) = (found.next(), found.next()) {
            *part = new;
            changed = true;
        }
    }
    changed
}

fn excluded(prior: &Value) -> Value {
    json!({"kind":"Not","args":prior})
}

/// Bind a spell's `other` after a changed group to that group.
pub(super) fn lower(ability: &mut Value) {
    contrast(ability, |_| fallback(), excluded);
}

/// The printable form of a lowered contrast: the written `other` it came from.
pub(crate) fn projection(ability: &Value) -> Option<Value> {
    let mut view = ability.clone();
    contrast(&mut view, excluded, |_| fallback()).then_some(view)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect(filter: Value) -> Value {
        json!({"kind":"Effect","args":{"operation":{"kind":"CreateContinuousEffect","args":{
            "affected":{"Objects":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},
            "kind":"Permanent","filter":filter}}}}}}})
    }

    fn spell(kind: &str, first: Value, second: Value) -> Value {
        json!({"kind":kind,"args":{"body":{"instructions":[effect(first),effect(second)]}}})
    }

    #[test]
    fn only_a_spell_s_other_after_a_group_names_that_group() {
        let zombies = json!({"kind":"AllOf","args":[{"kind":"Subtype","args":"Zombie"}]});
        let creature = json!({"kind":"CardType","args":"Creature"});
        let others = json!({"kind":"AllOf","args":[creature, fallback()]});
        let mut bound = spell("Spell", zombies.clone(), others.clone());
        lower(&mut bound);
        let filter = &bound["args"]["body"]["instructions"][1]
            .pointer(AFFECTED)
            .unwrap()["args"]["filter"];
        assert_eq!(filter["args"][1], excluded(&zombies));
        assert_eq!(
            projection(&bound),
            Some(spell("Spell", zombies.clone(), others.clone()))
        );
        for mut kept in [
            spell("Static", zombies.clone(), others.clone()),
            spell("Spell", others.clone(), others.clone()),
        ] {
            let before = kept.clone();
            lower(&mut kept);
            assert_eq!(kept, before);
        }
    }
}
