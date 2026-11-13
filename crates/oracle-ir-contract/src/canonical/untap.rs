//! A live population controlled by the ability's controller implies that role.
use serde_json::{Value, json};

fn fields(value: &Value, names: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|f| f.len() == names.len() && names.iter().all(|n| f.contains_key(*n)))
}

pub(super) fn controlled_population(value: &mut Value) {
    let args = &value["args"];
    if !fields(value, &["kind", "args"])
        || !fields(
            args,
            &["source", "affected", "changes", "duration", "evaluation"],
        )
        || args["evaluation"] != "LiveObjects"
        || args["duration"] != json!({"DuringNextUntapStep":{"kind":"AbilityController"}})
        || !fields(&args["affected"], &["Objects"])
    {
        return;
    }
    let objects = &args["affected"]["Objects"];
    if !fields(objects, &["kind", "args"]) || objects["kind"] != "Zone" {
        return;
    }
    let zone = &objects["args"];
    if !fields(zone, &["scope", "kind", "filter"])
        || zone["scope"] != json!({"Shared":"Battlefield"})
        || zone["kind"] != "Permanent"
    {
        return;
    }
    let controlled =
        json!({"kind":"ControlledBy","args":{"kind":"One","args":{"kind":"AbilityController"}}});
    let filter = &zone["filter"];
    let implied = filter == &controlled
        || fields(filter, &["kind", "args"])
            && filter["kind"] == "AllOf"
            && filter["args"]
                .as_array()
                .is_some_and(|parts| parts.len() <= 64 && parts.contains(&controlled));
    if !implied {
        return;
    }
    let expected = json!([{"kind":"Rule","args":{"UntapStep":{
        "players":{"kind":"One","args":{"kind":"ControllerOf","args":{"kind":"Affected"}}},"behavior":"Skip"}}}]);
    if args["changes"] != expected {
        return;
    }
    // Only this literal stable player role qualifies. Snapshot/fixed populations,
    // other players/windows and unfiltered objects do not prove the implication.
    // No typed ID is removed or synthesized by replacing these two ID-free roles.
    value["args"]["changes"][0]["args"]["UntapStep"]["players"] =
        json!({"kind":"One","args":{"kind":"AbilityController"}});
}
