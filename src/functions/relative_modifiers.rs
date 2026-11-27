//! Keep an adjacent controller qualifier in the comparative's counted phrase.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

fn count_population(value: &Value) -> Option<&Value> {
    match value["kind"].as_str()? {
        "CountObjects" => value.get("args"),
        "CountCounters" if value["args"]["recipients"]["kind"] == "Objects" => {
            value["args"]["recipients"].get("args")
        }
        _ => None,
    }
}

fn misplaced_controller(first: &Value, rest: &Value) -> bool {
    let comparison = &first["args"]["test"];
    if first["kind"] != "Property"
        || !matches!(
            first["args"]["property"].as_str(),
            Some("Power" | "ManaValue")
        )
        || comparison["kind"] != "Compare"
        || comparison["args"]["comparison"] != "LessOrEqual"
    {
        return false;
    }
    let Some(population) = count_population(&comparison["args"]["value"]) else {
        return false;
    };
    if population["kind"] != "Zone"
        || population["args"]["scope"]["Shared"] != "Battlefield"
        || population["args"]["kind"] != "Permanent"
    {
        return false;
    }
    let controller =
        json!({"kind":"ControlledBy","args":{"kind":"One","args":{"kind":"AbilityController"}}});
    let next = if rest["kind"] == "AllOf" {
        &rest["args"][0]
    } else {
        rest
    };
    if *next != controller {
        return false;
    }
    let filter = &population["args"]["filter"];
    match filter["kind"].as_str() {
        Some("Any" | "CardType" | "Subtype") => true,
        Some("AllOf") => filter["args"].as_array().is_some_and(|parts| {
            parts.len() <= 64
                && parts.iter().all(|part| {
                    matches!(
                        part["kind"].as_str(),
                        Some("Any" | "CardType" | "Subtype" | "Color" | "Supertype")
                    )
                })
        }),
        _ => false,
    }
}

pub(crate) fn join(first: &Value, rest: &Value) -> Result<Value, String> {
    if misplaced_controller(first, rest) {
        return Err("the controller phrase belongs to the counted population".into());
    }
    Ok(super::conjunction(vec![first.clone(), rest.clone()]))
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [first, rest] = args else {
        return Vec::new();
    };
    super::unify_and(args, value, bindings)
        .into_iter()
        .filter(|bound| {
            first
                .fill(bound, 0)
                .ok()
                .zip(rest.fill(bound, 0).ok())
                .is_some_and(|(first, rest)| !misplaced_controller(&first, &rest))
        })
        .collect()
}
