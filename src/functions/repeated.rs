//! An operand repeated by an explanatory phrase must have the same typed value.
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

fn same(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            let positioned = left.contains_key("$sym") && right.contains_key("$sym");
            let keep = |key: &&String| !positioned || key.as_str() != "at";
            left.keys().filter(keep).count() == right.keys().filter(keep).count()
                && left.iter().all(|(key, value)| {
                    (positioned && key == "at")
                        || right.get(key).is_some_and(|other| same(value, other))
                })
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(a, b)| same(a, b))
        }
        _ => left == right,
    }
}

pub(crate) fn agreed(first: &Value, repeated: &Value) -> Result<Value, String> {
    if !same(first, repeated) {
        return Err("a repeated operand disagrees with its first mention".into());
    }
    Ok(first.clone())
}

pub(crate) fn unify_agree<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [first, repeated] = args else {
        return Vec::new();
    };
    first
        .unify(value, bindings)
        .into_iter()
        .flat_map(|bound| repeated.unify(value, bound))
        .collect()
}
