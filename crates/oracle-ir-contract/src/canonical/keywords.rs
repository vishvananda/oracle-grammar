//! A single-object Suspect and Suspect over a singleton set are one action.
use crate::ir::Operand;
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

/// Each villainous option is one program, including the legacy single-item list.
pub(super) fn villainous_options(value: &mut Value, tracing: bool) {
    let args = &mut value["args"];
    if args["villainous"] != true {
        return;
    }
    let Some(options) = args["options"]
        .as_array_mut()
        .filter(|options| options.len() == 2)
    else {
        return;
    };
    for option in options {
        if option["kind"] != "List" {
            continue;
        }
        let Some(Operand::List(children)) = super::walk::lossless(option, tracing) else {
            continue;
        };
        if matches!(children.as_slice(), [Operand::Program(_)]) {
            *option = option["args"][0].take();
        }
    }
}

/// A lone conditional static granting Flash to its own source spells exactly
/// that source's conditional Flash. Other affected objects and changes retain
/// the ordinary conditional static, including explicitly named creators.
pub(super) fn conditional_flash(value: &mut Value) {
    if value["kind"] != "Conditional"
        || !value.as_object().is_some_and(|fields| fields.len() == 2)
        || !value["args"]
            .as_object()
            .is_some_and(|args| args.len() == 3)
        || value["args"]["otherwise"] != json!([])
        || value["args"]["condition"].is_null()
        || value["args"]["then"]
            != json!([{ "kind":"Static", "args":{
                "kind":"Ordinary", "affected":{"Objects":{"kind":"One","args":{"kind":"Source"}}},
                "changes":[{"kind":"GrantKeyword","args":{"kind":"Flash"}}]
            }}])
    {
        return;
    }
    let condition = value["args"]["condition"].take();
    *value = json!({"kind":"FlashIf","args":condition});
}

/// A coordinated list of unqualified card types names the same source
/// restrictions as each of its alternatives. Keep the list representation
/// so other lists, such as "hexproof from each color" (CR 702.11g), retain
/// their independently declared qualities.
pub(super) fn hexproof_object_qualities(value: &mut Value, tracing: bool) {
    let card_type = |filter: &Value| {
        filter["kind"] == "CardType"
            && filter["args"].is_string()
            && filter.as_object().is_some_and(|fields| fields.len() == 2)
    };
    let unqualified = |objects: &Value| {
        objects["kind"] == "Any" && objects.as_object().is_some_and(|fields| fields.len() == 2)
    };
    let mut filters = if let Some(qualities) = value["args"]["Each"].as_array() {
        if !(2..=64).contains(&qualities.len())
            || !qualities.iter().all(|quality| {
                quality.as_object().is_some_and(|fields| fields.len() == 1)
                    && unqualified(&quality["Objects"])
                    && card_type(&quality["Objects"]["filter"])
            })
        {
            return;
        }
        qualities
            .iter()
            .map(|quality| quality["Objects"]["filter"].clone())
            .collect::<Vec<_>>()
    } else {
        let source = &value["args"]["Objects"];
        let filter = &source["filter"];
        let Some(filters) = filter["args"]
            .as_array()
            .filter(|filters| (2..=64).contains(&filters.len()))
        else {
            return;
        };
        if !unqualified(source) || filter["kind"] != "AnyOf" || !filters.iter().all(card_type) {
            return;
        }
        filters.clone()
    };
    filters.sort_by_cached_key(|part| super::trace::key(part, tracing));
    if filters
        .windows(2)
        .any(|pair| super::trace::same(&pair[0], &pair[1], tracing))
    {
        return;
    }
    value["args"] = json!({"Each": filters.into_iter().map(|part| {
        json!({"Objects":{"kind":"Any","filter":part}})
    }).collect::<Vec<_>>()});
}

/// A singleton activated-ability selector is one kind. The top-card population
/// stays a live query owned by the same static condition.
pub(super) fn top_ability_kinds(value: &mut Value) {
    if !super::observed_top::one_library_top(&value["args"]["objects"]) {
        return;
    }
    let kinds = &mut value["args"]["kinds"];
    if kinds["kind"] != "List" {
        return;
    }
    let Some([only]) = kinds["args"].as_array_mut().map(Vec::as_mut_slice) else {
        return;
    };
    if only
        == &json!({"kind":"Term","args":{
            "family":"CheckHasable","form":"ActivatedAbility","operands":[]
        }})
    {
        *kinds = only.take();
    }
}
