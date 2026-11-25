//! One activation's explicit input query can name its proven holder source.
use serde_json::{Value, json};

const MAX_PARTS: usize = 64;

pub(super) fn components(ability: &Value) -> Vec<usize> {
    if ability["kind"] != "Activated" {
        return Vec::new();
    }
    let Some(parts) = ability["args"]["cost"]["parts"].as_array() else {
        return Vec::new();
    };
    if parts.len() > MAX_PARTS {
        return Vec::new();
    }
    parts
        .iter()
        .enumerate()
        .filter_map(|(index, part)| source_component(part).then_some(index))
        .collect()
}

fn source_selection(value: &Value) -> bool {
    let Some(fields) = value.as_object().filter(|fields| fields.len() == 2) else {
        return false;
    };
    let objects = &value["objects"];
    fields.get("choose").is_some_and(Value::is_null)
        && objects.as_object().is_some_and(|fields| fields.len() == 2)
        && objects["kind"] == "One"
        && objects["args"]
            .as_object()
            .is_some_and(|fields| fields.len() == 1)
        && objects["args"]["kind"] == "Source"
}

fn source_component(part: &Value) -> bool {
    let Some(fields) = part.as_object().filter(|fields| fields.len() == 1) else {
        return false;
    };
    let (name, args) = fields.iter().next().expect("one cost field");
    match name.as_str() {
        "Sacrifice" => source_selection(args),
        "Move" => {
            args.as_object().is_some_and(|fields| fields.len() == 2)
                && args["selections"]
                    .as_array()
                    .is_some_and(|items| items.len() == 1 && source_selection(&items[0]))
                && args["destination"]
                    .as_object()
                    .is_some_and(|fields| fields.len() == 1 && fields.contains_key("Exile"))
        }
        _ => false,
    }
}

pub(super) fn reference(value: &mut Value, components: &[usize]) -> bool {
    if components.is_empty()
        || value["kind"] != "Query"
        || !value.as_object().is_some_and(|fields| fields.len() == 2)
        || !value["args"]
            .as_object()
            .is_some_and(|fields| fields.len() == 2)
        || value["args"]["index"] != 0
    {
        return false;
    }
    let objects = &value["args"]["objects"];
    let args = &objects["args"];
    if objects["kind"] != "CostObjects"
        || !objects.as_object().is_some_and(|fields| fields.len() == 2)
        || !args.as_object().is_some_and(|fields| fields.len() == 4)
        || args["payment"] != "Activation"
        || args["view"] != "Inputs"
        || args["phase"] != "Before"
    {
        return false;
    }
    let Some(address) = args["component"]
        .as_array()
        .filter(|items| items.len() == 1)
    else {
        return false;
    };
    let Some(component) = address[0].as_u64().and_then(|n| usize::try_from(n).ok()) else {
        return false;
    };
    if !components.contains(&component) {
        return false;
    }
    // No typed identifier leaves are erased: Activation and the source are
    // roles; a top-level component address and index are ordinary integers.
    // Performed may be empty or redirected; After names a new incarnation.
    *value = json!({"kind":"Source"});
    true
}
