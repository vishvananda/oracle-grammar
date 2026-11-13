//! Alternatives of the same casting predicate form one disjunction of filters.
use super::{flatten, single, trace};
use serde_json::{Value, json};

pub(super) fn casting_alternatives(value: &mut Value, keep_order: bool, tracing: bool) {
    let Some(fields) = value.as_object().filter(|f| f.len() == 1) else {
        return;
    };
    let Some(arms) = fields.get("Either").and_then(Value::as_array) else {
        return;
    };
    if arms.is_empty()
        || arms.iter().any(|arm| {
            !arm.as_object()
                .is_some_and(|f| f.len() == 1 && f.contains_key("CastSpell"))
        })
    {
        return;
    }
    // Children have already been visited. Move their complete filters, including
    // typed ID origins; do not visit IDs twice or merge other payment predicates.
    let mut filters: Vec<Value> = arms.iter().map(|arm| arm["CastSpell"].clone()).collect();
    flatten(&mut filters, "AnyOf");
    if !keep_order {
        filters.sort_by_cached_key(|v| trace::key(v, tracing));
    }
    let filters = Value::Array(filters);
    let filter = single(&filters).unwrap_or_else(|| json!({"kind":"AnyOf","args":filters}));
    *value = json!({"CastSpell":filter});
}
