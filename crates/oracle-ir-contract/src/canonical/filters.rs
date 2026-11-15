//! Object filters with one meaning and two spellings.
use serde_json::{Value, json};

/// `Not(Condition(c))` is `Condition(Not(c))`: both evaluate `c` once, with the
/// candidate bound to the same object at the same point, and negate the answer.
/// The negation moves inside, the form most records already use. Only comparison
/// merges them: a printer reads each spelling back from its own form.
pub(super) fn negated_condition(value: &mut Value) {
    if !value.as_object().is_some_and(|fields| fields.len() == 2) {
        return;
    }
    let inner = &value["args"];
    if inner["kind"] != "Condition" || !inner.as_object().is_some_and(|fields| fields.len() == 2) {
        return;
    }
    // Move the intact condition, including ID-origin annotations.
    let condition = value["args"]["args"].take();
    *value = json!({"kind": "Condition", "args": {"kind": "Not", "args": condition}});
}
