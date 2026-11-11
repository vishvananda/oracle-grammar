//! Lexically implicit constant counter/tap actions on every actual returned card.
//! Temporary metadata leaves If visible to source-placement and target capture.
mod forward;
mod inverse;
mod operands;
#[cfg(test)]
mod tests;
use super::{Scope, return_grants::Context};
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};
use std::collections::BTreeSet;
const META: &str = "$returned_conditional";

fn implicit(value: &Value) -> bool {
    if value["$sym"] == "it" && value["args"] == json!([]) {
        return true;
    }
    matches!(
        value["$sym"].as_str(),
        Some("member_count_it" | "move_after_current_type" | "remember_object")
    ) && value["args"]
        .as_array()
        .is_some_and(|v| matches!(v.as_slice(), [inner] if implicit(inner)))
}
fn raw_actions(body: &Value) -> bool {
    body["instructions"].as_array().is_some_and(|items| {
        items.len() <= 4096
            && items.iter().all(|i| {
                if i["kind"] != "Effect" {
                    return false;
                }
                let op = &i["args"]["operation"];
                match op["kind"].as_str() {
                    Some("SetTapped") => {
                        op["args"]["objects"]["kind"] == "One"
                            && implicit(&op["args"]["objects"]["args"])
                    }
                    Some("PutCounters") => op["args"]["placements"].as_array().is_some_and(|p| {
                        !p.is_empty()
                            && p.len() <= 4096
                            && p.iter().all(|p| {
                                p["recipients"]["kind"] == "Objects"
                                    && p["recipients"]["args"]["kind"] == "One"
                                    && implicit(&p["recipients"]["args"]["args"])
                                    && p["amount"]["kind"] == "Constant"
                                    && p["selection"].is_null()
                            })
                    }),
                    _ => false,
                }
            })
    })
}
fn literal_reference(value: &Value) -> bool {
    value["kind"] == "Result"
        && value["args"]["phase"] == "After"
        && value["args"]["index"] == 0
        && matches!(
            value["args"]["result"]["$sym"].as_str(),
            Some("last_result" | "prior" | "back")
        )
}
fn literal_actions(body: &Value) -> bool {
    body["instructions"].as_array().is_some_and(|items| {
        items.len() <= 4096
            && items.iter().all(|i| {
                if i["kind"] != "Effect" {
                    return false;
                }
                let op = &i["args"]["operation"];
                match op["kind"].as_str() {
                    Some("SetTapped") => {
                        op["args"]["objects"]["kind"] == "One"
                            && literal_reference(&op["args"]["objects"]["args"])
                    }
                    Some("PutCounters") => op["args"]["placements"].as_array().is_some_and(|p| {
                        !p.is_empty()
                            && p.len() <= 4096
                            && p.iter().all(|p| {
                                p["recipients"]["kind"] == "Objects"
                                    && p["recipients"]["args"]["kind"] == "One"
                                    && literal_reference(&p["recipients"]["args"]["args"])
                                    && p["amount"]["kind"] == "Constant"
                                    && p["selection"].is_null()
                            })
                    }),
                    _ => false,
                }
            })
    })
}
fn literal_if_mut(value: &mut Value) -> Option<&mut Value> {
    if matches!(value["$sym"].as_str(), Some("leading_if" | "resolution")) {
        let args = value["args"].as_array_mut()?;
        let [inner] = args.as_mut_slice() else {
            return None;
        };
        return literal_if_mut(inner);
    }
    (value["kind"] == "If").then_some(value)
}
/// The literal mode is owned only by an exact EBNF recipe whose pronoun words
/// otherwise disappear into native result constructors. It is not public input.
pub(crate) fn annotate(value: &Value, literal: bool) -> Result<Value, String> {
    crate::functions::branch_bounds(value)?;
    let mut value = value.clone();
    let Some(items) = value.as_array_mut() else {
        return Err("returned conditional needs instructions".into());
    };
    let index = if literal {
        if items.len() != 3 || items[0]["kind"] != "Effect" || items[1]["kind"] != "Effect" {
            return Ok(value);
        }
        2
    } else {
        if items.len() != 1 {
            return Ok(value);
        }
        0
    };
    let only = &mut items[index];
    let Some(instruction) = literal_if_mut(only) else {
        return Ok(value);
    };
    let args = &instruction["args"];
    let eligible = args["condition"]["kind"] == "ObjectMatches"
        && (if literal {
            literal_reference(&args["condition"]["args"]["object"])
                && literal_actions(&args["then"])
                && literal_actions(&args["otherwise"])
        } else {
            implicit(&args["condition"]["args"]["object"])
                && raw_actions(&args["then"])
                && raw_actions(&args["otherwise"])
        });
    if eligible {
        instruction[META] = json!(true);
    }
    Ok(value)
}
fn marked(value: &Value) -> Result<bool, String> {
    let mut pending = vec![value];
    let mut work = 131_072;
    while let Some(v) = pending.pop() {
        if work == 0 {
            return Err("returned conditional annotation work".into());
        }
        work -= 1;
        if v.get(META).is_some() {
            return Ok(true);
        }
        match v {
            Value::Object(o) => pending.extend(o.values()),
            Value::Array(a) => pending.extend(a),
            _ => {}
        }
    }
    Ok(false)
}
fn definitions(value: &Value) -> Result<BTreeSet<u32>, String> {
    crate::functions::branch_bounds(value)?;
    let mut out = BTreeSet::new();
    let mut pending = vec![value];
    while let Some(v) = pending.pop() {
        if v["kind"] == "Effect"
            && let Some(n) = v["args"]["result"]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
        {
            out.insert(n);
        }
        match v {
            Value::Object(o) => pending.extend(o.values()),
            Value::Array(a) => pending.extend(a),
            _ => {}
        }
    }
    Ok(out)
}
pub(super) fn lower(value: &mut Value, scope: &mut Scope) -> Result<(), String> {
    if !marked(value)? {
        return Ok(());
    }
    crate::functions::branch_bounds(value)?;
    let mut used = definitions(value)?;
    let mut allocate = || {
        for _ in 0..4096 {
            let id: u32 = serde_json::from_value(
                serde_json::to_value(scope.results.allocate_generated()?)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            if used.insert(id) {
                return Ok(id);
            }
        }
        Err("returned conditional definition count".into())
    };
    forward::visit(value, &Context::default(), 0, &mut 131_072, &mut allocate)?;
    if marked(value)? {
        return Err("unconsumed returned conditional annotation".into());
    }
    Ok(())
}
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
    literal: bool,
) -> Vec<Bindings<'p, 'v>> {
    let [argument] = args else {
        return Vec::new();
    };
    let mut found = argument.unify(value, bindings.clone());
    if let Some(projected) = inverse::project_with_prefix(value, literal) {
        found.extend(argument.unify_made(&projected, &bindings));
    }
    found
}
