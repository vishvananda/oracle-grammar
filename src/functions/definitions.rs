//! Explicit defining power/toughness and creature-type words retain their
//! declaration context until binding (CR604.3).
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

const MARK: &str = "$stat_definition";

fn definable(changes: &Value) -> bool {
    changes.as_array().is_some_and(|items| {
        !items.is_empty()
            && items.len() <= 8
            && items.iter().all(|c| {
                matches!(
                    c["kind"].as_str(),
                    Some("SetPower" | "SetToughness" | "SetBasePowerToughness" | "CreatureTypes")
                )
            })
    })
}

pub(crate) fn mark(name: &str, value: &Value) -> Result<Value, String> {
    super::branch_bounds(value)?;
    let mut value = value.clone();
    if name == "defining_static" {
        if value["kind"] != "Static" || !definable(&value["args"]["changes"]) {
            return Err(
                "a defining static needs explicit power/toughness or creature-type setters".into(),
            );
        }
        value[MARK] = json!(true);
    } else {
        if !definable(&value) {
            return Err(
                "defining changes need explicit power/toughness or creature-type setters".into(),
            );
        }
        for change in value.as_array_mut().expect("checked list") {
            change[MARK] = json!(true);
        }
    }
    Ok(value)
}

pub(crate) fn unify<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else { return Vec::new() };
    if super::branch_bounds(value).is_err() {
        return Vec::new();
    }
    if name == "defining_changes" {
        return if definable(value) {
            inner.unify(value, bindings)
        } else {
            Vec::new()
        };
    }
    if value["kind"] != "Static" || !definable(&value["args"]["changes"]) {
        return Vec::new();
    }
    let mut prototype = value.clone();
    prototype["args"]["kind"] = json!("Ordinary");
    inner.unify_made(&prototype, &bindings)
}

#[derive(Clone, Copy)]
struct Context {
    intrinsic: bool,
    conditional: bool,
    token_definition: bool,
}

impl Context {
    fn intrinsic() -> Self {
        Self {
            intrinsic: true,
            conditional: false,
            token_definition: false,
        }
    }
}

/// Resolve only grammar-marked definitions. Numeric operands and references are
/// already bound, and are never changed by this classification (CR604.3a).
pub(crate) fn resolve(value: &mut Value) -> Result<(), String> {
    super::branch_bounds(value)?;
    walk(value, Context::intrinsic(), 0, &mut 131_072)
}

fn walk(value: &mut Value, context: Context, depth: usize, work: &mut usize) -> Result<(), String> {
    if depth > 128 || *work == 0 {
        return Err("defining-stat context limit".into());
    }
    *work -= 1;
    let kind = value["kind"].as_str().unwrap_or("").to_owned();
    let marked = value[MARK] == true
        || value["args"]["changes"]
            .as_array()
            .is_some_and(|items| items.iter().any(|change| change[MARK] == true));
    if let Some(fields) = value.as_object_mut() {
        fields.remove(MARK);
        for (key, child) in fields {
            let mut next = context;
            next.token_definition = key == "Defined" && child.get("abilities").is_some();
            if context.token_definition && key == "abilities"
                || matches!(kind.as_str(), "AddAbilities" | "AddAbilityIfAbsent")
            {
                next = Context::intrinsic();
            } else if matches!(kind.as_str(), "GrantAbility" | "CreateEmblem") {
                next.intrinsic = false;
                next.conditional = false;
            } else if kind == "Conditional" {
                next.conditional = true;
            }
            walk(child, next, depth + 1, work)?;
        }
    } else if let Some(items) = value.as_array_mut() {
        for child in items {
            walk(child, context, depth + 1, work)?;
        }
    }
    if kind == "Static" && marked {
        let own =
            value["args"]["affected"]["Objects"] == json!({"kind":"One","args":{"kind":"Source"}});
        value["args"]["kind"] = if own
            && context.intrinsic
            && !context.conditional
            && definable(&value["args"]["changes"])
        {
            json!({"CharacteristicDefining":{"keyword":null,"contributes_to_color_identity":true}})
        } else {
            json!("Ordinary")
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(object: &str) -> Value {
        let value = json!({"kind":"Static","args":{"kind":"Ordinary",
            "affected":{"Objects":{"kind":"One","args":{"kind":object}}},
            "changes":[{"kind":"SetPower","args":{"kind":"Constant","args":7}}]}});
        mark("defining_static", &value).unwrap()
    }

    #[test]
    fn intrinsic_holder_token_copy_and_conditional_contexts_stay_distinct() {
        let mut values = json!([definition("Source"), definition("GrantingObject"),
            {"kind":"GrantAbility","args":definition("Source")},
            {"kind":"Conditional","args":{"then":[definition("Source")]}},
            {"Defined":{"abilities":[definition("Source")]}},
            {"kind":"AddAbilities","args":[definition("Source")]}]);
        resolve(&mut values).unwrap();
        for (index, pointer, intrinsic) in [
            (0, "", true),
            (1, "", false),
            (2, "/args", false),
            (3, "/args/then/0", false),
            (4, "/Defined/abilities/0", true),
            (5, "/args/0", true),
        ] {
            let value = values[index].pointer(pointer).unwrap();
            assert_eq!(
                value["args"]["kind"]
                    .get("CharacteristicDefining")
                    .is_some(),
                intrinsic
            );
            assert_eq!(value["args"]["changes"][0]["args"]["args"], 7);
            assert!(value.get(MARK).is_none());
        }
    }

    #[test]
    fn marked_mined_changes_and_owned_definitions_share_one_classification() {
        let mut value = definition("Source");
        value.as_object_mut().unwrap().remove(MARK);
        value["args"]["changes"] = mark("defining_changes", &value["args"]["changes"]).unwrap();
        resolve(&mut value).unwrap();
        assert!(
            value["args"]["kind"]
                .get("CharacteristicDefining")
                .is_some()
        );
        assert!(value["args"]["changes"][0].get(MARK).is_none());
    }
}
