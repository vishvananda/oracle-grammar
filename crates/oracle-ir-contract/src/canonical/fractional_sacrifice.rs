//! A resolving half-sacrifice uses the payer's own permanents and clamps a
//! short choice. Cost components have a separate completion requirement.
use super::trace;
use serde_json::{Value, json};

fn without_actor(objects: &Value, actor: &Value, tracing: bool) -> Option<Value> {
    let filter = super::battlefield_filter(objects)?;
    let controlled = json!({"kind":"ControlledBy","args":actor});
    let mut filters = if filter["kind"] == "AllOf" {
        let parts = filter["args"].as_array()?;
        if parts.len() > 64 {
            return None;
        }
        parts.clone()
    } else {
        vec![filter.clone()]
    };
    let at = filters
        .iter()
        .position(|p| trace::same(p, &controlled, tracing))?;
    filters.remove(at);
    let remaining = match filters.len() {
        0 => json!({"kind":"Any"}),
        1 => filters.remove(0),
        _ => json!({"kind":"AllOf","args":filters}),
    };
    let mut population = objects.clone();
    population["args"]["filter"] = remaining;
    Some(population)
}

pub(super) fn normalize(operation: &mut Value, tracing: bool) -> bool {
    if operation["kind"] != "Sacrifice" {
        return false;
    }
    let args = &mut operation["args"];
    let actor = &args["players"];
    if actor["kind"] != "One" {
        return false;
    }
    let requested = &args["choose"];
    let Some((count, exact)) = requested
        .get("Exactly")
        .map(|q| (q, true))
        .or_else(|| requested.get("AsManyAsCan").map(|q| (q, false)))
    else {
        return false;
    };
    let operands = &count["args"];
    if count["kind"] != "Divide"
        || operands["numerator"]["kind"] != "CountObjects"
        || operands["denominator"] != json!({"kind":"Constant","args":2})
        || !matches!(operands["rounding"].as_str(), Some("Up" | "Down"))
    {
        return false;
    }
    let counted = &operands["numerator"]["args"];
    let Some(mut population) = without_actor(counted, actor, tracing) else {
        return false;
    };
    let objects = &args["objects"];
    if !trace::same(objects, counted, tracing) && !trace::same(objects, &population, tracing) {
        return false;
    }
    let reordered = !trace::same(objects, &population, tracing);
    if tracing && !reordered {
        trace::merge(&mut population, objects);
    }
    args["objects"] = population;
    if exact {
        let count = args["choose"]["Exactly"].take();
        args["choose"] = json!({"AsManyAsCan":count});
    }
    exact || reordered
}
