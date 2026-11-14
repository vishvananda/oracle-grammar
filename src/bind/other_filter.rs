//! Preserve ordinary other-reference binding and plural created-batch identity.
use super::{Scope, qualified};
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

pub(super) fn resolve(scope: &Scope, filter: &Value) -> Result<Value, String> {
    let Some(parts) = filter["args"]
        .as_array()
        .filter(|_| filter["kind"] == "AllOf")
    else {
        return Ok(filter.clone());
    };
    if parts.len() > 64 {
        return Err("other filter conjunct bound".into());
    }
    let Some((result, _)) = scope.created_result else {
        return Ok(filter.clone());
    };
    let mut changed = filter.clone();
    for (index, part) in parts.iter().enumerate() {
        if part["kind"] != "Not" || part["args"]["kind"] != "SameObject" {
            continue;
        }
        let domain = crate::functions::conjunction(
            parts
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != index)
                .map(|(_, part)| part.clone())
                .collect(),
        );
        if qualified::created_matches(scope, result, &domain) {
            changed["args"][index] = json!({"kind":"Not","args":{
                "kind":"InSet","args":{"kind":"Created","args":result}
            }});
        }
    }
    Ok(changed)
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [filter] = args else { return Vec::new() };
    let mut projected = value.clone();
    if value["kind"] == "AllOf"
        && let Some(parts) = projected["args"].as_array_mut()
    {
        if parts.len() > 64 {
            return Vec::new();
        }
        for part in parts {
            if part["kind"] == "Not"
                && part["args"]["kind"] == "InSet"
                && part["args"]["args"]["kind"] == "Created"
            {
                part["args"] = json!({"kind":"SameObject","args":{"kind":"Source"}});
            }
        }
    }
    filter.unify_made(&projected, &bindings)
}
