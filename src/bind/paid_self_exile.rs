//! One explicit optional self-exile payment owns its implicit delayed return.
//! The lexical proof is retained separately while normal reference binding runs.
use super::symbol_name;
use crate::model::ir::{
    CostObjectSelection, CostPart, ExileVisibility, MovementDestination, ObjectRef, ObjectSet,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

mod result_uses;

#[derive(Clone, Copy)]
pub(super) struct Site {
    index: usize,
    action: bool,
}
fn controller() -> Value {
    json!({"kind":"AbilityController"})
}
#[cfg(test)]
fn normal_entry() -> Value {
    json!({"controller":null,"tapped":false,"counters":[],"states":[]})
}
fn normal_exile() -> Value {
    json!({"Exile":{"visibility":"FaceUp","arrival":{"counters":[],"states":[]}}})
}
fn operation(instruction: &Value) -> Option<&Value> {
    (instruction["kind"] == "Effect").then_some(&instruction["args"]["operation"])
}
fn body(value: &Value) -> Option<&[Value]> {
    value["args"]["body"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
}
fn return_operation(branch: &Value) -> Option<&Value> {
    if branch["kind"] != "If"
        || !branch["args"]["otherwise"]["instructions"]
            .as_array()?
            .is_empty()
    {
        return None;
    }
    let [delay] = branch["args"]["then"]["instructions"]
        .as_array()?
        .as_slice()
    else {
        return None;
    };
    let delayed = operation(delay)?;
    let d = &delayed["args"];
    if delayed["kind"] != "CreateDelayedTrigger"
        || d["event"] != json!({"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}})
        || d["start"] != "Immediately"
        || !d["expires"].is_null()
        || !d["intervening_if"].is_null()
    {
        return None;
    }
    let [returned] = d["body"]["instructions"].as_array()?.as_slice() else {
        return None;
    };
    let moved = operation(returned)?;
    let a = &moved["args"];
    (moved["kind"] == "Move"
        && (a["destination"]["Battlefield"]["controller"] == "Owner"
            || a["destination"]["Battlefield"]["controller"] == json!({"Player":controller()}))
        && a["qualifiers"] == json!([]))
    .then_some(moved)
}
fn implicit(mut value: &Value) -> bool {
    let mut annotated = false;
    for _ in 0..4 {
        if matches!(symbol_name(value), Some("zone_members" | "exile_followup")) {
            annotated |= symbol_name(value) == Some("exile_followup");
            let Some(args) = value["args"].as_array() else {
                return false;
            };
            let [inner] = args.as_slice() else {
                return false;
            };
            value = inner;
        } else {
            break;
        }
    }
    if !annotated || value["kind"] != "One" {
        return false;
    }
    value = &value["args"];
    if symbol_name(value) == Some("member_count_it") {
        let Some(args) = value["args"].as_array() else {
            return false;
        };
        let [inner] = args.as_slice() else {
            return false;
        };
        value = inner;
    }
    matches!(symbol_name(value), Some("it" | "current_it")) && value["args"] == json!([])
}
fn source(value: &Value, raw: bool) -> bool {
    value == &json!({"kind":"Source"})
        || raw
            && matches!(symbol_name(value), Some("self" | "named_source"))
            && value["args"] == json!([])
}
fn source_move(value: &Value, raw: bool) -> bool {
    value["kind"] == "Move"
        && value["args"]["objects"]["kind"] == "One"
        && source(&value["args"]["objects"]["args"], raw)
        && value["args"]["destination"] == normal_exile()
        && value["args"]["qualifiers"] == json!([])
}
fn payment(value: &Value, raw: bool) -> Option<bool> {
    if value["kind"] != "PayCost"
        || value["args"]["player"] != controller()
        || value["args"]["optional"] != true
    {
        return None;
    }
    let [part] = value["args"]["cost"]["parts"].as_array()?.as_slice() else {
        return None;
    };
    if let Some(m) = part.get("Move") {
        let [selection] = m["selections"].as_array()?.as_slice() else {
            return None;
        };
        return (m["destination"] == normal_exile()
            && selection["objects"]["kind"] == "One"
            && source(&selection["objects"]["args"], raw)
            && selection["choose"].is_null())
        .then_some(false);
    }
    let action = part.get("PlayerAction")?;
    let [only] = action["body"]["instructions"].as_array()?.as_slice() else {
        return None;
    };
    (action["player"] == controller() && source_move(operation(only)?, raw)).then_some(true)
}
fn prefix(items: &[Value]) -> bool {
    items.iter().all(|i| {
        operation(i).is_some_and(|o| {
            matches!(
                o["kind"].as_str(),
                Some("Draw" | "GainLife" | "LoseLife" | "BindValue")
            )
        })
    })
}
fn bounded(value: &Value) -> Result<(), String> {
    crate::functions::branch_bounds(value)?;
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > 262_144 {
        return Err("paid self-exile lexical proof exceeds 256 KiB".into());
    }
    Ok(())
}
/// Inspect only a terminal pair in this ability, never a nested holder or branch.
/// Existing marker provenance distinguishes implicit it from explicit references.
pub(super) fn prepare(root: &Value) -> Result<Option<Site>, String> {
    if !matches!(
        root["kind"].as_str(),
        Some("Triggered" | "Activated" | "Spell")
    ) {
        return Ok(None);
    }
    let Some(items) = body(root) else {
        return Ok(None);
    };
    if !(2..=64).contains(&items.len()) {
        return Ok(None);
    }
    let index = items.len() - 2;
    let Some(returned) = return_operation(&items[index + 1]) else {
        return Ok(None);
    };
    if !implicit(&returned["args"]["objects"]) || !prefix(&items[..index]) {
        return Ok(None);
    }
    let Some(action) = operation(&items[index]).and_then(|op| payment(op, true)) else {
        return Ok(None);
    };
    bounded(root)?;
    Ok(Some(Site { index, action }))
}
fn definitions(value: &Value, ids: &mut BTreeSet<u64>) -> bool {
    if value["kind"] == "Effect" {
        let Some(id) = value["args"]["result"].as_u64() else {
            return false;
        };
        if id > u64::from(u32::MAX) || !ids.insert(id) || ids.len() > 4096 {
            return false;
        }
    }
    match value {
        Value::Object(fields) => fields.values().all(|v| definitions(v, ids)),
        Value::Array(items) => items.iter().all(|v| definitions(v, ids)),
        _ => true,
    }
}
/// After ordinary binding, use the actual parent ID and recheck its exact owner.
/// Replacing this one PlayerAction is a source lowering, not a general IR alias.
pub(super) fn lower(root: &mut Value, site: Option<Site>) -> Result<(), String> {
    let Some(site) = site else { return Ok(()) };
    bounded(root)?;
    let Some(items) = body(root) else {
        return Ok(());
    };
    if items.len() != site.index + 2 || !prefix(&items[..site.index]) {
        return Ok(());
    }
    let pay = &items[site.index];
    let Some(op) = operation(pay) else {
        return Ok(());
    };
    if payment(op, false) != Some(site.action) || return_operation(&items[site.index + 1]).is_none()
    {
        return Ok(());
    }
    let Some(result) = pay["args"]["result"]
        .as_u64()
        .filter(|n| *n <= u64::from(u32::MAX))
    else {
        return Ok(());
    };
    if items[site.index + 1]["args"]["condition"] != json!({"kind":"PaymentChosen","args":result})
        || !definitions(root, &mut BTreeSet::new())
    {
        return Ok(());
    }
    let strict = serde_json::to_value(CostPart::Move {
        selections: vec![CostObjectSelection {
            objects: ObjectSet::One(ObjectRef::Source),
            choose: None,
        }],
        destination: MovementDestination::Exile {
            visibility: ExileVisibility::FaceUp,
            arrival: Default::default(),
        },
    })
    .map_err(|e| e.to_string())?;
    let receipt = json!({"kind":"InZone","args":{
        "objects":{"kind":"CostObjects","args":{"payment":{"Resolution":result},"component":[0],"view":"Performed","phase":"After"}},"zone":"Exile"
    }});
    if site.action {
        let removed = op["args"]["cost"]["parts"][0]["PlayerAction"]["body"]["instructions"][0]["args"]["result"].as_u64().ok_or("paid self-exile inner result")?;
        let mut proposed = root.clone();
        replace(&mut proposed, site, strict.clone(), receipt.clone())?;
        if !result_uses::absent(proposed, removed)? {
            return Ok(());
        }
    }
    replace(root, site, strict, receipt)?;
    Ok(())
}

fn replace(root: &mut Value, site: Site, strict: Value, receipt: Value) -> Result<(), String> {
    let items = root["args"]["body"]["instructions"]
        .as_array_mut()
        .ok_or("paid self-exile body")?;
    if site.action {
        items[site.index]["args"]["operation"]["args"]["cost"]["parts"][0] = strict;
    }
    items[site.index + 1]["args"]["then"]["instructions"][0]["args"]["operation"]["args"]["body"]
        ["instructions"][0]["args"]["operation"]["args"]["objects"] = receipt;
    Ok(())
}

#[cfg(test)]
#[path = "paid_self_exile_tests.rs"]
mod tests;
