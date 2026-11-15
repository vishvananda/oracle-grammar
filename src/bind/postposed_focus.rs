//! A postposed condition's possessive names the object its consequent acts on.
use super::symbol_name;
use serde_json::Value;

/// "If this spell was kicked, counter that spell if its mana value is 4 or less instead":
/// "its" follows "that spell", so it names that spell, not the spell tested before it.
/// Called on a `resolution` marker before placement markers are removed; the
/// condition is bound before its consequent, so it shares the consequent's reference.
pub(super) fn mark(marker: &mut Value) {
    let Some(inner) = marker["args"].get_mut(0) else {
        return;
    };
    if inner["kind"] != "If" {
        return;
    }
    let Some(reference) = consequent_reference(&inner["args"]["then"]).cloned() else {
        return;
    };
    let condition = &mut inner["args"]["condition"];
    if condition["kind"] == "ObjectMatches"
        && symbol_name(&condition["args"]["object"]) == Some("it")
        && condition["args"]["object"]["args"]
            .as_array()
            .is_some_and(Vec::is_empty)
    {
        condition["args"]["object"] = reference;
    }
}

/// The one named object a single-effect consequent acts on: `counter that spell`.
fn consequent_reference(then: &Value) -> Option<&Value> {
    let [effect] = then["instructions"].as_array()?.as_slice() else {
        return None;
    };
    if effect["kind"] != "Effect" {
        return None;
    }
    let objects = &effect["args"]["operation"]["args"]["objects"];
    let reference = &objects["args"];
    (objects["kind"] == "One" && symbol_name(reference) == Some("qualified_object"))
        .then_some(reference)
}
