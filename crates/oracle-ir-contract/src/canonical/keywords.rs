//! A single-object Suspect and Suspect over a singleton set are one action.
use serde_json::{Value, json};
use std::collections::HashSet;

fn objects(object: &Value, optional: Option<&HashSet<u64>>) -> Value {
    let args = &object["args"];
    let target = object["kind"] == "Target"
        && object.as_object().is_some_and(|fields| fields.len() == 2)
        && args.as_object().is_some_and(|fields| fields.len() == 2)
        && args["index"] == 0
        && super::trace::number(&args["slot"])
            .is_some_and(|slot| optional.is_some_and(|slots| slots.contains(&slot)));
    if target {
        json!({"kind":"Targets","args":args["slot"]})
    } else {
        json!({"kind":"One","args":object})
    }
}

pub(super) fn suspect(value: &mut Value, optional: Option<&HashSet<u64>>) {
    let Some(operation) = value.as_object().filter(|fields| fields.len() == 2) else {
        return;
    };
    let Some(args) = operation.get("args").and_then(Value::as_object) else {
        return;
    };
    let (Some(actor), Some(action)) = (args.get("actor"), args.get("action")) else {
        return;
    };
    if value["kind"] == "KeywordEach" {
        if args.len() == 3
            && action == "Suspect"
            && let Some(set) = args.get("objects")
            && set["kind"] == "One"
            && set.as_object().is_some_and(|fields| fields.len() == 2)
        {
            value["args"]["objects"] = objects(&set["args"], optional);
        }
        return;
    }
    if args.len() != 2 || !action.as_object().is_some_and(|fields| fields.len() == 1) {
        return;
    }
    let Some(fields) = action
        .get("Suspect")
        .and_then(Value::as_object)
        .filter(|fields| fields.len() == 1)
    else {
        return;
    };
    let Some(object) = fields.get("object") else {
        return;
    };
    // Move the same actor and object subtrees, including traced ID origins.
    // Unknown fields must remain invalid rather than disappearing in a rewrite.
    *value = json!({"kind":"KeywordEach","args":{
        "actor":actor,"action":"Suspect","objects":objects(object, optional)
    }});
}
