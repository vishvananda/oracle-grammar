//! Count-valued noun agreement keeps the value unchanged in either direction.
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

pub(crate) fn guard(name: &str, count: &Value) -> Result<Value, String> {
    if name == "plural_amount" {
        return (count["kind"] == "Constant"
            && count["args"].as_i64().is_some_and(|amount| amount >= 2))
        .then(|| count.clone())
        .ok_or_else(|| "source callback needs a fixed plural token count".into());
    }
    let amount = count.get("Exactly").or_else(|| count.get("UpTo"));
    let singular = amount
        .is_some_and(|amount| amount["kind"] == "Constant" && amount["args"].as_i64() == Some(1));
    if singular != (name == "singular_count") {
        return Err("count disagrees with its noun".into());
    }
    Ok(count.clone())
}

pub(crate) fn unify<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [count] = args else {
        return Vec::new();
    };
    if guard(name, value).is_err() {
        return Vec::new();
    }
    count.unify(value, bindings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plural_amount_requires_a_proved_multiple_not_an_unknown_quantity() {
        for amount in [2, 3, 19, 1_000] {
            let value = json!({"kind":"Constant","args":amount});
            assert_eq!(guard("plural_amount", &value).unwrap(), value);
        }
        for value in [
            json!({"kind":"Constant","args":1}),
            json!({"kind":"Constant","args":0}),
            json!({"kind":"Constant","args":-1}),
            json!({"kind":"AnnouncedX"}),
            json!({"Exactly":{"kind":"Constant","args":3}}),
        ] {
            assert!(guard("plural_amount", &value).is_err(), "{value}");
        }
    }
}
