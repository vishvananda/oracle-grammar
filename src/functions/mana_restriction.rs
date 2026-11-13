//! One trailing spending sentence qualifies every alternative of a mana clause.
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

fn fields(value: &Value, names: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|v| v.len() == names.len() && names.iter().all(|n| v.contains_key(*n)))
}

fn program(value: &Value, path: &str, out: &mut Vec<String>, depth: usize) -> Result<(), String> {
    if !fields(value, &["instructions"]) {
        return Err("mana clause program fields".into());
    }
    single(
        &value["instructions"],
        &format!("{path}/instructions"),
        out,
        depth,
    )
}

fn single(value: &Value, path: &str, out: &mut Vec<String>, depth: usize) -> Result<(), String> {
    let Some([instruction]) = value.as_array().map(Vec::as_slice) else {
        return Err("spending qualification needs one explicit mana clause".into());
    };
    walk(instruction, &format!("{path}/0"), out, depth + 1)
}

fn walk(value: &Value, path: &str, out: &mut Vec<String>, depth: usize) -> Result<(), String> {
    if depth > 32 || out.len() >= 32 {
        return Err("mana clause depth/candidate limit".into());
    }
    if !fields(value, &["kind", "args"]) {
        return Err("mana clause instruction fields".into());
    }
    let args = &value["args"];
    match value["kind"].as_str() {
        Some("Effect") if fields(args, &["result", "operation"]) => {
            let operation = &args["operation"];
            if !fields(operation, &["kind", "args"])
                || operation["kind"] != "AddMana"
                || !fields(&operation["args"], &["players", "mana", "restriction"])
            {
                return Err("a spending sentence needs only explicit mana producers".into());
            }
            out.push(format!("{path}/args/operation/args/restriction"));
        }
        Some("Optional") if fields(args, &["decider", "body"]) => {
            program(&args["body"], &format!("{path}/args/body"), out, depth + 1)?;
        }
        Some("If") if fields(args, &["condition", "then", "otherwise"]) => {
            for arm in ["then", "otherwise"] {
                program(&args[arm], &format!("{path}/args/{arm}"), out, depth + 1)?;
            }
        }
        _ => return Err("spending qualification cannot cross other instructions or scopes".into()),
    }
    Ok(())
}

fn sites(value: &Value) -> Result<Vec<String>, String> {
    super::branch_bounds(value)?;
    let mut out = Vec::new();
    single(value, "", &mut out, 0)?;
    Ok(out)
}

pub(crate) fn attach(value: &Value, restriction: &Value) -> Result<Value, String> {
    super::branch_bounds(restriction)?;
    let sites = sites(value)?;
    if restriction.is_null()
        || sites
            .iter()
            .any(|p| value.pointer(p).is_none_or(|v| !v.is_null()))
    {
        return Err("spending qualification needs unqualified mana and an explicit use".into());
    }
    let mut result = value.clone();
    for site in sites {
        *result.pointer_mut(&site).ok_or("mana qualification path")? = restriction.clone();
    }
    super::branch_bounds(&result)?;
    Ok(result)
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [body, restriction] = args else {
        return Vec::new();
    };
    let Ok(sites) = sites(value) else {
        return Vec::new();
    };
    let Some(first) = sites.first().and_then(|p| value.pointer(p)) else {
        return Vec::new();
    };
    if first.is_null() || sites.iter().any(|p| value.pointer(p) != Some(first)) {
        return Vec::new();
    }
    let mut plain = value.clone();
    for site in sites {
        let Some(field) = plain.pointer_mut(&site) else {
            return Vec::new();
        };
        *field = Value::Null;
    }
    body.unify_made(&plain, &bindings)
        .into_iter()
        .flat_map(|b| restriction.unify(first, b))
        .collect()
}
