//! A singular query keeps its declared zone when used as an action operand.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

fn query_zone(objects: &Value) -> Option<&Value> {
    match objects["kind"].as_str()? {
        "InZone" => objects.pointer("/args/zone"),
        "Zone" => objects
            .pointer("/args/scope/Shared")
            .or_else(|| objects.pointer("/args/scope/Players/zone")),
        "Matching" | "Take" => query_zone(&objects["args"]["objects"]),
        _ => None,
    }
}

fn singleton_zone(objects: &Value) -> Option<&Value> {
    let reference = &objects["args"];
    let query = &reference["args"];
    (objects["kind"] == "One"
        && reference["kind"] == "Query"
        && query["index"] == 0
        && query["objects"]["kind"] == "InZone"
        && query["objects"]["args"]["objects"]["kind"] == "One")
        .then(|| &query["objects"]["args"]["zone"])
}

pub(crate) fn zone_members(objects: &Value) -> Value {
    if singleton_zone(objects).is_some() {
        return objects.clone();
    }
    if objects["kind"] == "InZone" {
        let inner = &objects["args"]["objects"];
        if singleton_zone(inner) == Some(&objects["args"]["zone"]) {
            return inner.clone();
        }
    }
    let reference = &objects["args"];
    let zone = (objects["kind"] == "One" && reference["kind"] == "Query")
        .then(|| query_zone(&reference["args"]["objects"]))
        .flatten();
    match zone {
        Some(zone) => json!({"kind":"InZone","args":{"objects":objects,"zone":zone}}),
        None => objects.clone(),
    }
}

pub(crate) fn unify_zone_members<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [argument] = args else {
        return Vec::new();
    };
    let mut solutions = Vec::new();
    // A lexical return may add this guard from its actual preceding exile.
    // This is an inverse proposal only: complete reparse must prove that the
    // owning Move and result definition supply that context again.
    if value["kind"] == "InZone"
        && value["args"]["zone"] == "Exile"
        && value["args"]["objects"]["kind"] == "One"
        && value["args"]["objects"]["args"]["kind"] == "Result"
        && value["args"]["objects"]["args"]["args"]["phase"] == "After"
    {
        solutions.extend(argument.unify(&value["args"]["objects"], bindings.clone()));
    }
    if let Some(zone) = singleton_zone(value) {
        let guarded = json!({"kind":"InZone","args":{"objects":value,"zone":zone}});
        solutions.extend(argument.unify_made(&guarded, &bindings));
    }
    if zone_members(value) == *value {
        solutions.extend(argument.unify(value, bindings.clone()));
    }
    if value["kind"] == "InZone"
        && let Some(objects) = value.pointer("/args/objects")
        && zone_members(objects) == *value
    {
        solutions.extend(argument.unify(objects, bindings));
    }
    solutions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ir_schema::Ty;

    #[test]
    fn singular_zone_actions_and_their_references_are_inverses() {
        for zone in ["Exile", "Graveyard", "Library"] {
            let scope = if zone == "Exile" {
                json!({"Shared":zone})
            } else {
                json!({"Players":{"zone":zone,"players":{"kind":"One","args":{"kind":"AbilityController"}}}})
            };
            let reference = json!({"kind":"Query","args":{"objects":{"kind":"Zone",
                "args":{"scope":scope,"kind":"Card","filter":{"kind":"ExiledWith","args":{"kind":"Source"}}}},"index":0}});
            let objects = json!({"kind":"One","args":reference});
            let args = [Pattern::Hole(
                "objects".into(),
                Ty::Named("ObjectSet".into()),
            )];
            let value = zone_members(&objects);
            let found = unify_zone_members(&args, &value, Bindings::new());
            assert!(found.iter().any(|b| b.get("objects") == Some(&objects)));
            for binding in &found {
                assert_eq!(zone_members(binding.get("objects").unwrap()), value);
            }
            let mut wrong = value;
            wrong["args"]["zone"] = json!("Battlefield");
            let found = unify_zone_members(&args, &wrong, Bindings::new());
            assert!(found.iter().all(|b| b.get("objects") != Some(&objects)));
        }
        for objects in [
            json!({"kind":"One","args":{"kind":"Source"}}),
            json!({"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"Any"}}}),
        ] {
            assert_eq!(zone_members(&objects), objects);
        }
    }

    #[test]
    fn a_current_singleton_query_does_not_need_another_zone_check() {
        for zone in ["Exile", "Graveyard", "Library"] {
            let reference = json!({"kind":"Query","args":{"objects":{"kind":"InZone",
                "args":{"objects":{"kind":"One","args":{"kind":"EventDestination"}},"zone":zone}},"index":0}});
            let objects = json!({"kind":"One","args":reference});
            assert_eq!(zone_members(&objects), objects);
            let args = [Pattern::Hole(
                "objects".into(),
                Ty::Named("ObjectSet".into()),
            )];
            let found = unify_zone_members(&args, &objects, Bindings::new());
            let guarded = json!({"kind":"InZone","args":{"objects":objects,"zone":zone}});
            assert_eq!(zone_members(&guarded), objects);
            assert_eq!(found.len(), 2);
            assert!(found.iter().any(|b| b.get("objects") == Some(&objects)));
            assert!(found.iter().any(|b| b.get("objects") == Some(&guarded)));
            for binding in found {
                assert_eq!(zone_members(binding.get("objects").unwrap()), objects);
            }
            let other = json!({"kind":"InZone","args":{"objects":objects,"zone":"Command"}});
            assert_eq!(zone_members(&other), other);
        }
    }
}
