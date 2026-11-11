//! The defending player belongs to an attacker, independently of target focus.
use super::{Scope, Step};
use crate::model::ir::ObjectRef;
use serde_json::{Value, json};

pub(super) fn defending(root: &Value, path: &[Step], scope: &Scope) -> Result<Value, String> {
    let attacker = if let Some(attacker) = &scope.defending_attacker {
        attacker.clone()
    } else if affected_attacker(root, path)? {
        ObjectRef::Affected
    } else {
        ObjectRef::Source
    };
    Ok(json!({"kind":"DefendingPlayerOf", "args":attacker}))
}

/// A scalar change on an explicitly attacking population is evaluated for each
/// affected attacker. Its creator is a different object, often a spell.
fn affected_attacker(mut value: &Value, path: &[Step]) -> Result<bool, String> {
    if path.len() > 128 {
        return Err("combat reference depth exceeds128".into());
    }
    for step in path {
        if matches!(step, Step::Key(key) if key == "changes") {
            let mut objects = &value["affected"]["Objects"];
            if super::symbol_name(objects) == Some("distributive_pump_subject") {
                objects = &objects["args"][0];
            }
            if objects["kind"] == "Zone" && attacking(&objects["args"]["filter"], 0)? {
                return Ok(true);
            }
        }
        value = match step {
            Step::Key(key) => value.get(key),
            Step::Index(index) => value.get(*index),
        }
        .ok_or("combat reference path")?;
    }
    Ok(false)
}

fn attacking(filter: &Value, depth: usize) -> Result<bool, String> {
    if depth > 128 {
        return Err("combat population depth exceeds128".into());
    }
    match filter["kind"].as_str() {
        Some("Attacking") => Ok(true),
        Some("AllOf" | "AnyOf") => {
            let parts = filter["args"]
                .as_array()
                .ok_or("combat population filter")?;
            let mut any = false;
            let mut all = !parts.is_empty();
            for part in parts {
                let yes = attacking(part, depth + 1)?;
                any |= yes;
                all &= yes;
            }
            Ok(if filter["kind"] == "AllOf" { any } else { all })
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotated_attacking_population_keeps_its_defending_player() {
        let objects = json!({"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},
            "kind":"Permanent","filter":{"kind":"Attacking"}}});
        let root = json!({"affected":{"Objects":{"$sym":"distributive_pump_subject", "args":[objects]}},
            "changes":[{"kind":"Pump","args":{"power":null}}]});
        let path = [Step::Key("changes".into()), Step::Index(0)];
        assert_eq!(
            defending(&root, &path, &Scope::default()).unwrap(),
            json!({"kind":"DefendingPlayerOf","args":{"kind":"Affected"}})
        );
    }
}
