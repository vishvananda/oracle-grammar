//! Named removed counters measure one proved component of this activation's payment.
use super::{Step, at_path, path_of};
use crate::model::ir::{Cost, CostPart, Expr};
use serde_json::{Value, json};

pub(super) const NAME: &str = "activation_removed_counters";

pub(super) fn marker(root: &mut Value, path: &[Step], name: &str) -> Result<bool, String> {
    if name != NAME {
        return Ok(false);
    }
    crate::functions::branch_bounds(root)?;
    let kind = at_path(root, path).ok_or("counter payment path")?["args"][0].clone();
    let cost = cost(root)?;
    let reference = reference(root, path, &cost, &kind)?;
    *at_path(root, path).ok_or("counter payment path")? = reference;
    Ok(true)
}

fn cost(root: &Value) -> Result<Cost, String> {
    if root["kind"] != "Activated" {
        return Err("removed counters need this activation's payment".into());
    }
    let mut raw = root["args"]["cost"].clone();
    super::filters::strip(&mut raw);
    serde_json::from_value(raw).map_err(|e| format!("counter payment cost: {e}"))
}

/// The receipt measures the whole component. Every removal in it must have the
/// named kind: a mixed/unspecified-kind component cannot answer this question.
fn homogeneous(part: &CostPart) -> Result<Value, String> {
    let CostPart::RemoveCounters { removals } = part else {
        return Err("counter receipt needs a removal component".into());
    };
    let mut kinds = removals.iter().map(|r| {
        r.kind
            .as_ref()
            .map(|k| serde_json::to_value(k).expect("typed counter kind"))
            .ok_or("counter receipt cannot isolate an unspecified kind")
    });
    let kind = kinds.next().ok_or("empty counter removal component")??;
    for other in kinds {
        if other? != kind {
            return Err("counter receipt cannot isolate a mixed-kind component".into());
        }
    }
    Ok(kind)
}

fn component(cost: &Cost, kind: &Value) -> Result<u32, String> {
    let mut found = None;
    for (index, part) in cost.parts.iter().enumerate() {
        match part {
            CostPart::Mana(_) | CostPart::TapSymbol | CostPart::Life(Expr::Constant(_)) => {}
            CostPart::RemoveCounters { .. } => {
                if homogeneous(part)? == *kind {
                    if found.is_some() {
                        return Err(
                            "removed counters have multiple matching payment components".into()
                        );
                    }
                    found = Some(u32::try_from(index).map_err(|_| "counter component overflow")?);
                }
            }
            _ => return Err("counter receipt has an unsupported payment composition".into()),
        }
    }
    found.ok_or_else(|| "no matching counter removal in this activation's payment".into())
}

fn amount_path(index: usize) -> Vec<Step> {
    let mut path = path_of(&["args", "body", "instructions"]);
    path.push(Step::Index(index));
    path.extend(path_of(&[
        "args",
        "operation",
        "args",
        "mana",
        "Repeat",
        "amount",
    ]));
    path
}

/// Limit this lexical proof to immediate mana instructions. An intervening
/// effect/branch or a future ability may supply its own removal receipt.
fn immediate(root: &Value, path: &[Step]) -> bool {
    let Some(items) = root["args"]["body"]["instructions"].as_array() else {
        return false;
    };
    for (index, item) in items.iter().enumerate() {
        let operation = &item["args"]["operation"];
        if item["kind"] != "Effect" || operation["kind"] != "AddMana" {
            return false;
        }
        if path == amount_path(index) {
            return operation["args"]["mana"].get("Repeat").is_some();
        }
    }
    false
}

fn reference(root: &Value, path: &[Step], cost: &Cost, kind: &Value) -> Result<Value, String> {
    if !immediate(root, path) {
        return Err("counter payment receipt needs an immediate mana amount".into());
    }
    let component = component(cost, kind)?;
    Ok(json!({"kind":"PaidCountersRemoved","args":{
        "payment":"Activation","component":[component]}}))
}

/// Reconstruct the noun from the current typed cost and prove that the lexical
/// binding returns this exact receipt. No retained text supplies a counter kind.
pub(super) fn surface(root: &Value) -> Result<Option<Value>, String> {
    if root["kind"] != "Activated" {
        return Ok(None);
    }
    crate::functions::branch_bounds(root)?;
    let cost = cost(root)?;
    let Some(items) = root["args"]["body"]["instructions"].as_array() else {
        return Ok(None);
    };
    let mut replacements = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let operation = &item["args"]["operation"];
        if item["kind"] != "Effect" || operation["kind"] != "AddMana" {
            break;
        }
        let mana = &operation["args"]["mana"];
        let amount = &mana["Repeat"]["amount"];
        if mana.get("Repeat").is_none()
            || amount["kind"] != "PaidCountersRemoved"
            || amount["args"]["payment"] != "Activation"
        {
            continue;
        }
        let Some([address]) = amount["args"]["component"].as_array().map(Vec::as_slice) else {
            continue;
        };
        let Some(part) = address
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| cost.parts.get(i))
        else {
            continue;
        };
        let Ok(kind) = homogeneous(part) else {
            continue;
        };
        let path = amount_path(index);
        if reference(root, &path, &cost, &kind).is_ok_and(|r| r == *amount) {
            replacements.push((path, json!({"$sym":NAME,"args":[kind]})));
        }
    }
    if replacements.is_empty() {
        return Ok(None);
    }
    let mut view = root.clone();
    for (path, value) in replacements {
        *at_path(&mut view, &path).ok_or("counter receipt surface path")? = value;
    }
    Ok(Some(view))
}

#[cfg(test)]
#[path = "../tests/counter_payment_bind.rs"]
mod tests;
