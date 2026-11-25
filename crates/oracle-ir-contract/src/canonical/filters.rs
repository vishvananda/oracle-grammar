//! Object filters with one meaning and two spellings.
use super::trace;
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

/// `AnyOf([Property{property: p, test: t1}, Property{property: p, test: t2}, ...])`
/// is `Property{property: p, test: NumberTest::AnyOf([t1, t2, ...])}`: both
/// evaluate `p` once on the same candidate object and accept if any test passes.
pub(super) fn merge_same_property_any_of(value: &mut Value, tracing: bool) {
    if !value.as_object().is_some_and(|fields| fields.len() == 2) {
        return;
    }
    let Some(items) = value.get("args").and_then(Value::as_array) else {
        return;
    };
    if items.len() < 2 {
        return;
    }
    let mut shared_key: Option<String> = None;
    for item in items {
        let Some(obj) = item.as_object() else {
            return;
        };
        if obj.len() != 2 || obj.get("kind").and_then(Value::as_str) != Some("Property") {
            return;
        }
        let Some(args) = obj.get("args").and_then(Value::as_object) else {
            return;
        };
        if args.len() != 2 || !args.contains_key("property") || !args.contains_key("test") {
            return;
        }
        let key = trace::key(&args["property"], tracing);
        if let Some(prev) = &shared_key {
            if prev != &key {
                return;
            }
        } else {
            shared_key = Some(key);
        }
    }
    let Some(items) = value.get_mut("args").and_then(Value::as_array_mut) else {
        return;
    };
    let mut property = Value::Null;
    let mut tests = Vec::new();
    for (idx, item) in items.iter_mut().enumerate() {
        let args = item["args"]
            .as_object_mut()
            .expect("verified Property args");
        if idx == 0 {
            property = args.remove("property").expect("verified property");
        }
        let test = args.remove("test").expect("verified test");
        if test.as_object().is_some_and(|o| o.len() == 2)
            && test.get("kind").and_then(Value::as_str) == Some("AnyOf")
            && let Some(Value::Array(nested)) = test.get("args").cloned()
        {
            tests.extend(nested);
        } else {
            tests.push(test);
        }
    }
    tests.sort_by_cached_key(|v| trace::key(v, tracing));
    *value = json!({
        "kind": "Property",
        "args": {
            "property": property,
            "test": {"kind": "AnyOf", "args": tests},
        },
    });
}
