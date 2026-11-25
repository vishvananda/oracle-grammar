//! Remove proven redundant predicates and combine nested matching intersections.
use serde_json::{Value, json};

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
