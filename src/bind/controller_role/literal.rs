//! A literal that-player may refine only the default of an own-object ETB.
use super::super::{Scope, Step, per_recipient, references, symbol_name};
use serde_json::{Value, json};

pub(super) fn seed(root: &Value, scope: &Scope) -> bool {
    root["kind"] == "Triggered"
        && scope.holders == 0
        && root["args"]["event"]
            == json!({"ZoneChange":{"objects":{
            "kind":"SameObject","args":{"kind":"Source"}},"from":null,"to":"Battlefield"}})
        && root["args"]["intervening_if"].is_null()
        && root["args"]["limits"] == json!({"per_turn":null,"total":null})
        && root["args"]["announcement"]["modes"].is_null()
        && scope.players == vec![json!({"kind":"ControllerOf","args":{"kind":"EventSubject"}})]
}

pub(super) fn scalar_spec(spec: &Value) -> bool {
    let Some(parts) = spec["filter"]["args"].as_array() else {
        return false;
    };
    spec["count"]["Exactly"] == json!({"kind":"Constant","args":1})
        && spec["filter"]["kind"] == "AllOf"
        && parts.len() == 2
        && parts
            .iter()
            .any(|p| p["kind"] == "CardType" && p["args"].is_string())
        && parts.iter().any(|p| {
            symbol_name(p) == Some("indefinite_opponent_controller")
                && p["args"]
                    == json!([{"kind":"ControlledBy","args":{"kind":"OpponentsOf",
                "args":{"kind":"AbilityController"}}}])
        })
}

pub(super) fn reference(
    root: &Value,
    path: &[Step],
    introduced: &[references::TargetIntroduction],
    scope: &Scope,
    contexts: &references::Contexts,
) -> Result<Value, String> {
    let prior = contexts.before(path, scope);
    if let Some(role) = &prior.controller_role
        && role.player_generation == prior.players.len()
        && per_recipient::player(root, path, introduced, scope.target_base)?.is_none()
        && co_object(
            root,
            path,
            &json!(role.object),
            introduced,
            scope.target_base,
        )?
    {
        return Ok(json!({"kind":"ControllerOf","args":role.object}));
    }
    let fallback = per_recipient::reference(root, path, introduced, scope, contexts)?;
    let Some(role) = &prior.controller_role else {
        return Ok(fallback);
    };
    if prior.default_object_player != Some(prior.players.len())
        || role.player_generation != prior.players.len()
        || !role.literal
        || fallback != json!({"kind":"ControllerOf","args":{"kind":"EventSubject"}})
        || !root["args"]["announcement"]["modes"].is_null()
    {
        return Ok(fallback);
    }
    crate::functions::branch_bounds(root)?;
    let Some(actor) = action(root, path, introduced, scope.target_base)? else {
        return Ok(fallback);
    };
    if actor != json!(role.object) {
        return Ok(fallback);
    }
    Ok(json!({"kind":"ControllerOf","args":actor}))
}

fn keys(path: &[Step], expected: &[&str]) -> bool {
    path.len() == expected.len()
        && path
            .iter()
            .zip(expected)
            .all(|(step, key)| matches!(step, Step::Key(actual) if actual == key))
}

/// Direct terminal actions only: no generated scope, optional, loop or callback.
fn action(
    root: &Value,
    path: &[Step],
    introduced: &[references::TargetIntroduction],
    base: usize,
) -> Result<Option<Value>, String> {
    if path.len() < 7
        || !keys(&path[..3], &["args", "body", "instructions"])
        || !keys(&path[4..7], &["args", "operation", "args"])
    {
        return Ok(None);
    }
    let Step::Index(index) = path[3] else {
        return Ok(None);
    };
    let Some(items) = root["args"]["body"]["instructions"].as_array() else {
        return Ok(None);
    };
    if index + 1 != items.len() || items[index]["kind"] != "Effect" {
        return Ok(None);
    }
    let op = &items[index]["args"]["operation"];
    let (actor, prefix_end) = if op["kind"] == "DealDamage"
        && keys(&path[7..], &["recipients", "args", "args"])
        && op["args"]["recipients"]["kind"] == "Players"
        && op["args"]["recipients"]["args"]["kind"] == "One"
    {
        let actor = plain(&op["args"]["source"], introduced, base)?;
        let amount = plain(&op["args"]["amount"], introduced, base)?;
        if !constant(&amount)
            && !(amount["kind"] == "ObjectProperty"
                && amount["args"]["object"] == actor
                && ["Power", "Toughness"]
                    .iter()
                    .any(|p| amount["args"]["property"] == *p)
                && amount["args"]["information"] == "CurrentOrLastKnown")
        {
            return Ok(None);
        }
        (actor, index)
    } else if op["kind"] == "LoseLife"
        && keys(&path[7..], &["players", "args"])
        && op["args"]["players"]["kind"] == "One"
        && constant(&op["args"]["amount"])
        && index > 0
        && items[index - 1]["kind"] == "Effect"
        && items[index - 1]["args"]["operation"]["kind"] == "Destroy"
        && items[index - 1]["args"]["operation"]["args"]["objects"]["kind"] == "One"
    {
        (
            plain(
                &items[index - 1]["args"]["operation"]["args"]["objects"]["args"],
                introduced,
                base,
            )?,
            index - 1,
        )
    } else {
        return Ok(None);
    };
    if actor["kind"] != "Target"
        || actor["args"]["index"] != 0
        || !items[..prefix_end].iter().all(prefix)
    {
        return Ok(None);
    }
    Ok(Some(actor))
}

/// `target X an opponent controls and all Y that player controls`: the player
/// is written inside the one object set that also lists the role's target.
fn co_object(
    root: &Value,
    path: &[Step],
    object: &Value,
    introduced: &[references::TargetIntroduction],
    base: usize,
) -> Result<bool, String> {
    if path.len() > 128 {
        return Err("controller co-object depth exceeds 128".into());
    }
    let mut node = root;
    let mut set = None;
    for step in path {
        node = match step {
            Step::Key(key) => {
                let child = node.get(key);
                if key == "objects" {
                    set = child;
                }
                child
            }
            Step::Index(index) => node.get(*index),
        }
        .ok_or("controller co-object path")?;
    }
    let Some(set) = set else {
        return Ok(false);
    };
    let set = plain(set, introduced, base)?;
    let members = |value: &Value, member: Value| {
        value["args"]
            .as_array()
            .is_some_and(|items| items.contains(&member))
    };
    Ok(match set["kind"].as_str() {
        Some("Union") => members(&set, json!({"kind":"One","args":object})),
        Some("Zone") => {
            let filter = &set["args"]["filter"];
            filter["kind"] == "AnyOf" && members(filter, json!({"kind":"SameObject","args":object}))
        }
        _ => false,
    })
}

// Compare already introduced operands through their transparent lexical
// annotations. This view never mutates pending target specs or the source tree.
fn plain(
    value: &Value,
    introduced: &[references::TargetIntroduction],
    base: usize,
) -> Result<Value, String> {
    let mut value = references::bound_reference(value, introduced, base)?;
    super::super::filters::strip(&mut value);
    Ok(value)
}

fn constant(value: &Value) -> bool {
    value["kind"] == "Constant" && value["args"].as_u64().is_some()
}

fn prefix(item: &Value) -> bool {
    let op = &item["args"]["operation"];
    item["kind"] == "Effect"
        && ((op["kind"] == "Draw"
            && op["args"]["players"] == json!({"kind":"One","args":{"kind":"AbilityController"}})
            && constant(&op["args"]["amount"]))
            || (op["kind"] == "SetTapped"
                && op["args"]["objects"]["kind"] == "One"
                && op["args"]["objects"]["args"]["kind"] == "Target"))
}
