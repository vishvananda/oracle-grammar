//! Event alternatives have no semantic order; their written order is grammar data.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

pub(crate) fn either_event(left: &Value, right: &Value) -> Value {
    json!({"AnyOf":[left, right]})
}

/// Two alternatives admit at most two assignments, both retained. This is not
/// a permutation search over arbitrary lists or a first-match event selection.
pub(crate) fn unify_either_event<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [left, right] = args else {
        return Vec::new();
    };
    let Some([first, second]) = value
        .get("AnyOf")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
    else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for bound in left.unify(first, bindings.clone()) {
        found.extend(right.unify(second, bound));
    }
    if first != second {
        for bound in left.unify(second, bindings) {
            found.extend(right.unify(first, bound));
        }
    }
    found
}
