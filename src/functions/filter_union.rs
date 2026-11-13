//! A binary union of current filters, with every nonempty inverse partition.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

pub(crate) fn join(parts: Vec<Value>) -> Result<Value, String> {
    let mut items = Vec::new();
    for part in parts {
        super::branch_bounds(&part)?;
        if part["kind"] == "AnyOf" {
            let Some(inner) = part["args"].as_array() else {
                return Err("filter union needs a filter list".into());
            };
            items.extend(inner.iter().cloned());
        } else {
            items.push(part);
        }
    }
    let result = if items.len() == 1 {
        items.remove(0)
    } else {
        json!({"kind":"AnyOf","args":items})
    };
    super::branch_bounds(&result)?;
    Ok(result)
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [left, right] = args else {
        return Vec::new();
    };
    if super::branch_bounds(value).is_err()
        || !value.as_object().is_some_and(|v| v.len() == 2)
        || value["kind"] != "AnyOf"
    {
        return Vec::new();
    }
    let Some(items) = value["args"]
        .as_array()
        .filter(|v| (2..=10).contains(&v.len()))
    else {
        return Vec::new();
    };
    let mut solutions = Vec::new();
    for mask in 1..(1usize << items.len()) - 1 {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for (index, item) in items.iter().enumerate() {
            if mask >> index & 1 == 1 {
                a.push(item.clone());
            } else {
                b.push(item.clone());
            }
        }
        let (Ok(a), Ok(b)) = (join(a), join(b)) else {
            return Vec::new();
        };
        for found in left.unify_made(&a, &bindings) {
            solutions.extend(right.unify_made(&b, &found));
        }
    }
    solutions
}
