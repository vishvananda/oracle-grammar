//! Remove only proven redundant predicates from object-set matching.
use serde_json::{Value, json};

pub(super) fn matching(value: &mut Value) {
    if !value.as_object().is_some_and(|fields| fields.len() == 2) {
        return;
    }
    let args = &value["args"];
    if !args.as_object().is_some_and(|fields| fields.len() == 2) {
        return;
    }
    let objects = &args["objects"];
    if args["filter"] == json!({"kind":"Any"})
        || (objects["kind"] == "Zone"
            && objects["args"]["scope"] == json!({"Shared":"Battlefield"})
            && args["filter"] == json!({"kind":"InZone","args":{"Shared":"Battlefield"}}))
    {
        // Move the intact subtree, including ID-origin annotations. Unknown
        // inner fields remain available for native validation to reject.
        *value = objects.clone();
    }
}
