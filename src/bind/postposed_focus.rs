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

// A distributive creature subject tests the name condition for each of its
// members. The same clause cannot become an overall test of the source.
pub(super) fn missing_member_scope(marker: &Value) -> bool {
    let branch = if symbol_name(marker) == Some("resolution") {
        &marker["args"][0]
    } else {
        marker
    };
    if branch["kind"] != "If" {
        return false;
    }
    let args = &branch["args"];
    let condition = &args["condition"];
    let filter = &condition["args"]["filter"];
    let Some([effect]) = args["then"]["instructions"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let operation = &effect["args"]["operation"];
    let output = &operation["args"];
    let subjects = &output["affected"]["Objects"];
    let object = &condition["args"]["object"];
    args["otherwise"]["instructions"]
        .as_array()
        .is_some_and(Vec::is_empty)
        && condition["kind"] == "ObjectMatches"
        && (symbol_name(object) == Some("it")
            || symbol_name(object) == Some("member_count_it")
                && symbol_name(&object["args"][0]) == Some("it"))
        && filter["kind"] == "AnyOf"
        && matches!(filter["args"].as_array().map(Vec::as_slice), Some([another, any])
            if another["kind"] == "SharesNameWithAnother" && any["kind"] == "SharesNameWithAny")
        && effect["kind"] == "Effect"
        && operation["kind"] == "CreateContinuousEffect"
        && output["duration"] == "EndOfTurn"
        && output["evaluation"] == "Snapshot"
        && output["changes"].as_array().is_some_and(
            |changes| matches!(changes.as_slice(), [change] if change["kind"] == "Pump"),
        )
        && symbol_name(subjects) == Some("distributive_pump_subject")
        && subjects["args"][0]["kind"] == "Zone"
}
