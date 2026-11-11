//! Cost guards preserve values and distinguish native fixed/variable mana forms.
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

pub(crate) fn guard_cost(name: &str, cost: &Value) -> Result<Value, String> {
    let parts = cost["parts"]
        .as_array()
        .ok_or("a cost guard needs Cost.parts")?;
    let mut variable = false;
    for part in parts {
        if let Some(mana) = part.get("Mana") {
            let symbols = mana.as_array().ok_or("Mana must contain symbols")?;
            variable |= symbols.iter().any(|symbol| symbol == "X");
        }
    }
    if variable != (name == "variable_mana") {
        return Err("mana cost variability disagrees with its native keyword".into());
    }
    Ok(cost.clone())
}

pub(crate) fn unify_cost_guard<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [cost] = args else {
        return Vec::new();
    };
    if guard_cost(name, value).is_err() {
        return Vec::new();
    }
    cost.unify(value, bindings)
}

/// The pinned producer supplies direct source quantities in the cost itself.
/// Other explicit counter prices use a separate definition. This partitions
/// representations without changing either expression or choosing a reading.
pub(crate) fn guard_counter_price(name: &str, value: &Value) -> Result<Value, String> {
    let args = &value["args"];
    // The grammar's holder word is still symbolic before reference binding.
    // Card-name, target and event references are distinct from this marker.
    let holder = |object: &Value| object["kind"] == "Source" || object["$sym"] == "self";
    let source = match value["kind"].as_str() {
        Some("ObjectProperty") => holder(&args["object"]),
        Some("CountCounters") => {
            let recipients = &args["recipients"];
            // Semantic prototypes use the schema's tagged enum; the native
            // CounterRecipients serializer uses its external variant name.
            let objects = if recipients["kind"] == "Objects" {
                &recipients["args"]
            } else {
                &recipients["Objects"]
            };
            objects["kind"] == "One" && holder(&objects["args"])
        }
        _ => false,
    } && args["information"] == "CurrentOrLastKnown";
    if source != (name == "inline_counter_price") {
        return Err("counter price uses a different definition form".into());
    }
    Ok(value.clone())
}

pub(crate) fn unify_counter_price<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [expression] = args else {
        return Vec::new();
    };
    if guard_counter_price(name, value).is_err() {
        return Vec::new();
    }
    expression.unify(value, bindings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn direct_source_quantities_and_other_definitions_are_complementary() {
        let property = json!({"kind":"ObjectProperty","args":{"object":{"kind":"Source"},"property":"Power","information":"CurrentOrLastKnown"}});
        let counters = json!({"kind":"CountCounters","args":{"recipients":{"Objects":{"kind":"One","args":{"kind":"Source"}}},"kind":{"Named":"ChargeCounter"},"information":"CurrentOrLastKnown"}});
        for value in [property, counters] {
            assert_eq!(
                guard_counter_price("inline_counter_price", &value),
                Ok(value.clone())
            );
            assert!(guard_counter_price("captured_counter_price", &value).is_err());
        }
        let symbolic = json!({"kind":"ObjectProperty","args":{"object":{"$sym":"self","args":[],"at":12},"property":"Power","information":"CurrentOrLastKnown"}});
        assert_eq!(
            guard_counter_price("inline_counter_price", &symbolic),
            Ok(symbolic.clone())
        );
        assert!(guard_counter_price("captured_counter_price", &symbolic).is_err());
        let prototype = json!({"kind":"CountCounters","args":{"recipients":{"kind":"Objects","args":{"kind":"One","args":{"$sym":"self","args":[],"at":12}}},"kind":{"Named":"ChargeCounter"},"information":"CurrentOrLastKnown"}});
        assert_eq!(
            guard_counter_price("inline_counter_price", &prototype),
            Ok(prototype.clone())
        );
        assert!(guard_counter_price("captured_counter_price", &prototype).is_err());
    }

    #[test]
    fn target_event_and_predicate_objects_never_become_source_prices() {
        for object in [
            json!({"kind":"Target","args":{"slot":0,"index":0}}),
            json!({"kind":"EventSubject"}),
            json!({"kind":"Candidate"}),
            json!({"$sym":"it","args":[],"at":12}),
        ] {
            let value = json!({"kind":"ObjectProperty","args":{"object":object,"property":"ManaValue","information":"CurrentOrLastKnown"}});
            assert!(guard_counter_price("inline_counter_price", &value).is_err());
            assert_eq!(
                guard_counter_price("captured_counter_price", &value),
                Ok(value.clone())
            );
        }
    }
}
