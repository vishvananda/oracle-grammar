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

/// A candidate's power and toughness are taken from the same view.
pub(super) fn equal_or_ordered_stats(value: &mut Value, tracing: bool) {
    use crate::ir::{Comparison, Information, ObjectFilter, ObjectRef, ObjectState, Property};
    use crate::ir::{Expr, NumberTest};
    if !matches!(
        value["args"]["property"].as_str(),
        Some("Power" | "Toughness")
    ) || value["args"]["test"]["args"]["value"]["args"]["object"]["kind"] != "Candidate"
    {
        return;
    }
    let Some(ObjectFilter::Property {
        property,
        test: NumberTest::Compare {
            comparison,
            value: right,
        },
    }) = super::walk::lossless(value, tracing)
    else {
        return;
    };
    let Expr::ObjectProperty {
        object: ObjectRef::Candidate,
        property: peer,
        information: Information::CurrentOrLastKnown,
    } = right
    else {
        return;
    };
    let state = match (property, comparison, peer) {
        (Property::Power, Comparison::Equal, Property::Toughness)
        | (Property::Toughness, Comparison::Equal, Property::Power) => {
            ObjectState::PowerEqualsToughness
        }
        (Property::Power, Comparison::Less, Property::Toughness)
        | (Property::Toughness, Comparison::Greater, Property::Power) => {
            ObjectState::PowerBelowToughness
        }
        _ => return,
    };
    *value = json!(ObjectFilter::State(state));
}

/// Both clauses test the same event object with a pure stat comparison.
pub(super) fn event_subject_disjunction(value: &mut Value, tracing: bool) {
    let Some([left, right]) = value["args"].as_array().map(Vec::as_slice) else {
        return;
    };
    let subject = json!({"kind":"EventSubject"});
    let is_stat_match = |clause: &Value| {
        let filter = &clause["args"]["filter"];
        matches!(
            filter["args"]["property"].as_str(),
            Some("Power" | "Toughness")
        ) && filter["kind"] == "Property"
            && filter["args"]["test"]["kind"] == "Compare"
            && *clause == json!({"kind":"ObjectMatches","args":{"object":subject,"filter":filter}})
    };
    if !is_stat_match(left) || !is_stat_match(right) {
        return;
    }
    let mut alternatives = [
        left["args"]["filter"].clone(),
        right["args"]["filter"].clone(),
    ];
    alternatives.sort_by_cached_key(|filter| trace::key(filter, tracing));
    *value = json!({"kind":"ObjectMatches","args":{
        "object":subject,"filter":{"kind":"AnyOf","args":alternatives}
    }});
}

/// The candidate fails each explicitly excluded type, in one snapshot.
/// Keep the transform confined to type tests, which have no nested bindings.
pub(super) fn excluded_types(value: &mut Value, tracing: bool) {
    if value["kind"] != "Not" || value["args"]["kind"] != "AnyOf" {
        return;
    }
    let Some([a, b]) = value["args"]["args"].as_array().map(Vec::as_slice) else {
        return;
    };
    if !matches!(
        (a["kind"].as_str(), b["kind"].as_str()),
        (Some("CardType"), Some("Subtype")) | (Some("Subtype"), Some("CardType"))
    ) || ![a, b].iter().all(|item| {
        item.as_object().is_some_and(|terms| terms.len() == 2) && item["args"].is_string()
    }) {
        return;
    }
    let mut tests = [
        json!({"kind":"Not","args":a}),
        json!({"kind":"Not","args":b}),
    ];
    tests.sort_by_cached_key(|test| trace::key(test, tracing));
    *value = json!({"kind":"AllOf","args":tests});
}
