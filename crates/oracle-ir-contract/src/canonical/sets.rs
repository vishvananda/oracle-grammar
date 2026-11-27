//! Remove proven redundant predicates and combine nested matching intersections.
use serde_json::{Value, json};
use std::collections::HashSet;

const MAX_CONJUNCTS: usize = 64;

fn matching_shape(value: &Value) -> bool {
    value["kind"] == "Matching"
        && value.as_object().is_some_and(|fields| fields.len() == 2)
        && value["args"].as_object().is_some_and(|fields| {
            fields.len() == 2 && fields.contains_key("objects") && fields.contains_key("filter")
        })
}

pub(super) fn matching(value: &mut Value, keep_order: bool, tracing: bool) {
    if !matching_shape(value) {
        return;
    }
    let args = &value["args"];
    let objects = &args["objects"];
    if args["filter"] == json!({"kind":"Any"})
        || (objects["kind"] == "Zone"
            && objects["args"]["scope"] == json!({"Shared":"Battlefield"})
            && args["filter"] == json!({"kind":"InZone","args":{"Shared":"Battlefield"}}))
    {
        *value = value["args"]["objects"].take();
        return;
    }
    // Both predicates see the same candidate and object view. Never replace the
    // underlying receipt, its phase, or a historical arrival-zone predicate.
    // Printing retains the authored nesting; comparison has one conjunction.
    if !keep_order && matching_shape(objects) {
        let Some(filter) = conjunction(&objects["args"]["filter"], &args["filter"], tracing) else {
            return;
        };
        let objects = value["args"]["objects"]["args"]["objects"].take();
        *value = json!({"kind":"Matching","args":{"objects":objects,"filter":filter}});
    }
}

fn conjunction(inner: &Value, outer: &Value, tracing: bool) -> Option<Value> {
    let mut filters = Vec::new();
    for filter in [inner, outer] {
        if filter["kind"] == "AllOf"
            && filter.as_object().is_some_and(|fields| fields.len() == 2)
            && let Some(items) = filter["args"].as_array()
        {
            if filters.len() + items.len() > MAX_CONJUNCTS {
                return None;
            }
            filters.extend(items.iter().cloned());
        } else {
            if filters.len() == MAX_CONJUNCTS {
                return None;
            }
            filters.push(filter.clone());
        }
    }
    super::drop_identity(&mut filters);
    filters.sort_by_cached_key(|filter| super::trace::key(filter, tracing));
    Some(if filters.len() == 1 {
        filters.pop().expect("one conjunct")
    } else {
        json!({"kind":"AllOf","args":filters})
    })
}

/// Exact and optional single permanent targets name the same battlefield
/// object when expressed as a singleton or as a SameObject zone filter.
pub(super) fn single_battlefield_target(spec: &Value) -> bool {
    spec["kind"] == json!({"Object":{"kind":"Permanent","zones":["Battlefield"]}})
        && ["Exactly", "UpTo"]
            .iter()
            .any(|name| spec["count"][*name] == json!({"kind":"Constant","args":1}))
}

fn member_filter(objects: &Value, slots: Option<&HashSet<u64>>) -> Option<Value> {
    if let Some(filter) = super::battlefield_filter(objects) {
        return Some(filter.clone());
    }
    let slots = slots?;
    let slot = match objects["kind"].as_str()? {
        "One" if objects["args"]["kind"] == "Target" && objects["args"]["args"]["index"] == 0 => {
            &objects["args"]["args"]["slot"]
        }
        "Targets" => &objects["args"],
        _ => return None,
    };
    let id = super::trace::number(slot)?;
    slots.contains(&id).then(|| {
        json!({"kind":"SameObject","args":{
            "kind":"Target","args":{"slot":slot,"index":0}
        }})
    })
}

pub(super) fn battlefield_union<'a>(
    objects: impl ExactSizeIterator<Item = Option<&'a Value>>,
    slots: Option<&HashSet<u64>>,
    keep_order: bool,
    tracing: bool,
) -> Option<Value> {
    if objects.len() == 0 || objects.len() > MAX_CONJUNCTS {
        return None;
    }
    let mut filters = objects
        .map(|objects| member_filter(objects?, slots))
        .collect::<Option<Vec<_>>>()?;
    super::flatten(&mut filters, "AnyOf");
    if !keep_order {
        filters.sort_by_cached_key(|filter| super::trace::key(filter, tracing));
    }
    Some(
        json!({"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent",
        "filter":{"kind":"AnyOf","args":filters}}}),
    )
}

/// Only a simple simultaneous move to face-up exile can use this unordered
/// population proof. Ordered zones, qualifiers and other destinations keep
/// the action's original selection shape.
pub(super) fn plain_exile(args: &Value) -> bool {
    args["qualifiers"] == json!([])
        && args["destination"]
            == json!({"Exile":{"visibility":"FaceUp","arrival":{"counters":[],"states":[]}}})
}

pub(super) fn exile_union(operation: &mut Value, slots: &HashSet<u64>, tracing: bool) {
    let args = &mut operation["args"];
    if args["objects"]["kind"] != "Union" || !plain_exile(args) {
        return;
    }
    let Some(items) = args["objects"]["args"].as_array() else {
        return;
    };
    if let Some(zone) = battlefield_union(items.iter().map(Some), Some(slots), false, tracing) {
        args["objects"] = zone;
    }
}
