//! Direct object statistics use their object-predicate production instead.
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

pub(crate) fn guard(condition: &Value) -> Result<&Value, String> {
    let left = &condition["args"]["left"];
    let subject = if left["$sym"] == "its_stat" {
        &left["args"][0]
    } else {
        left
    };
    if condition["kind"] != "Compare" || subject["kind"] == "ObjectProperty" {
        return Err("direct object-stat conditions require the object predicate".into());
    }
    Ok(condition)
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [condition] = args else { return Vec::new() };
    if guard(value).is_err() {
        return Vec::new();
    }
    condition.unify(value, bindings)
}
