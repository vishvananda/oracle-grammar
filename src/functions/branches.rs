//! Inverse of native kicker specialization: shared prefix/suffix and one test.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

pub(crate) fn branch_bounds(value: &Value) -> Result<(), String> {
    let mut pending = vec![(value, 0)];
    let mut nodes = 0;
    let mut bytes = 0;
    while let Some((value, depth)) = pending.pop() {
        nodes += 1;
        if nodes > 16_384 || depth > 128 || bytes > 1024 * 1024 {
            return Err("branch semantic work bound".into());
        }
        match value {
            Value::Object(items) => {
                if pending.len() + nodes + items.len() > 16_384 {
                    return Err("branch semantic work bound".into());
                }
                for (key, child) in items {
                    bytes += key.len();
                    pending.push((child, depth + 1));
                }
            }
            Value::Array(items) => {
                if pending.len() + nodes + items.len() > 16_384 {
                    return Err("branch semantic work bound".into());
                }
                pending.extend(items.iter().map(|v| (v, depth + 1)));
            }
            Value::String(text) => bytes += text.len(),
            _ => {}
        }
        if bytes > 1024 * 1024 {
            return Err("branch semantic work bound".into());
        }
    }
    Ok(())
}

// Effect definitions receive fresh ids when each branch is canonicalized. The
// inverse may share their spelling; the final reparse still checks every use.
fn same_spelling(left: &Value, right: &Value) -> bool {
    if left["$sym"] == "target" && right["$sym"] == "target" {
        let mut a = left["args"][0].clone();
        let mut b = right["args"][0].clone();
        if let (Some(a), Some(b)) = (a.as_object_mut(), b.as_object_mut()) {
            a.remove("slot");
            b.remove("slot");
        }
        return a == b;
    }
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            let effect = left["kind"] == "Effect" && right["kind"] == "Effect";
            a.len() == b.len()
                && a.iter().all(|(key, value)| {
                    b.get(key).is_some_and(|other| {
                        if effect && key == "args" {
                            same_spelling(&value["operation"], &other["operation"])
                        } else {
                            same_spelling(value, other)
                        }
                    })
                })
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_spelling(a, b))
        }
        _ => left == right,
    }
}

fn spell(value: &Value) -> Option<&[Value]> {
    (value["kind"] == "Spell" && value["args"]["announcement"]["modes"].is_null())
        .then(|| {
            value["args"]["body"]["instructions"]
                .as_array()
                .map(Vec::as_slice)
        })
        .flatten()
}

pub(crate) fn unify_kicker_branch<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [cost, source] = args else {
        return Vec::new();
    };
    if value["kind"] != "CostBranch"
        || value["args"]["keyword"] != "Kicker"
        || branch_bounds(value).is_err()
    {
        return Vec::new();
    }
    let Some(paid) = spell(&value["args"]["paid"]) else {
        return Vec::new();
    };
    let Some(unpaid) = spell(&value["args"]["unpaid"]) else {
        return Vec::new();
    };
    if paid.len() + unpaid.len() > 128 {
        return Vec::new();
    }
    let prefix = paid
        .iter()
        .zip(unpaid)
        .take_while(|(p, u)| same_spelling(p, u))
        .count();
    let suffix = paid
        .iter()
        .rev()
        .zip(unpaid.iter().rev())
        .take_while(|(p, u)| same_spelling(p, u))
        .count();
    let mut found = Vec::new();
    let mut candidates = 0;
    for start in (0..=prefix).rev() {
        for end in (0..=suffix.min(paid.len().min(unpaid.len()) - start)).rev() {
            if candidates == 64 {
                return found;
            }
            candidates += 1;
            let mut instructions = paid[..start].to_vec();
            instructions.push(json!({"kind":"If", "args":{
                "condition":{"kind":"ObjectMatches", "args":{"object":{"kind":"Source"}, "filter":{"kind":"State", "args":"Kicked"}}},
                "then":{"instructions":&paid[start..paid.len()-end]},
                "otherwise":{"instructions":&unpaid[start..unpaid.len()-end]}
            }}));
            instructions.extend_from_slice(&paid[paid.len() - end..]);
            let made = json!({"kind":"Spell", "args":{
                "announcement":{"targets":[], "modes":null}, "body":{"instructions":instructions}
            }});
            for bound in cost.unify(&value["args"]["cost"], bindings.clone()) {
                found.extend(source.unify_made(&made, &bound));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_bounds_cover_last_strings_wide_arrays_and_depth() {
        assert!(branch_bounds(&json!("x".repeat(1024 * 1024 + 1))).is_err());
        assert!(branch_bounds(&json!(vec![0; 16_384])).is_err());
        let mut nested = json!(0);
        for _ in 0..129 {
            nested = json!([nested]);
        }
        assert!(branch_bounds(&nested).is_err());
        assert!(branch_bounds(&json!({"instructions":[]})).is_ok());
    }
}
