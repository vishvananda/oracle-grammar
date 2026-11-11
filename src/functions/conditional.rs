//! One available ability, preserving any zones in which it functions.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

pub(crate) fn wrap(condition: Value, mut ability: Value) -> Value {
    if ability["kind"] == "Scoped" {
        let inner = ability["args"]["ability"].take();
        ability["args"]["ability"] = wrap(condition, inner);
        ability
    } else {
        json!({"kind":"Conditional","args":{
            "condition":condition,"then":[ability],"otherwise":[]
        }})
    }
}

fn split(value: &Value) -> Option<(Value, Value)> {
    if value["kind"] == "Scoped" {
        let (condition, inner) = split(&value["args"]["ability"])?;
        let mut scoped = value.clone();
        scoped["args"]["ability"] = inner;
        return Some((condition, scoped));
    }
    if value["kind"] != "Conditional" || !value["args"]["otherwise"].as_array()?.is_empty() {
        return None;
    }
    let [ability] = value["args"]["then"].as_array()?.as_slice() else {
        return None;
    };
    Some((value["args"]["condition"].clone(), ability.clone()))
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    if super::branch_bounds(value).is_err() {
        return Vec::new();
    }
    let [condition, ability] = args else {
        return Vec::new();
    };
    let Some((test, inner)) = split(value) else {
        return Vec::new();
    };
    if wrap(test.clone(), inner.clone()) != *value {
        return Vec::new();
    }
    condition
        .unify_made(&test, &bindings)
        .into_iter()
        .flat_map(|binding| ability.unify_made(&inner, &binding))
        .collect()
}
