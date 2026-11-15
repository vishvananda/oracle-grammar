//! Casting choices specialize bound source predicates while retaining branch announcements.
use super::{Scope, bind_scope, json, slot_uses};
use crate::functions::Choice;
use serde_json::Value;

pub(super) fn bind(node: &mut Value, outer: &Scope) -> Result<Value, String> {
    crate::functions::branch_bounds(node)?;
    let overload = node["$sym"] == "overload_branch";
    let kind = Choice::of(node["$sym"].as_str().unwrap_or_default()).unwrap_or(Choice::Kicker);
    let gift = kind == Choice::Gift;
    let arguments = node["args"]
        .as_array_mut()
        .ok_or("casting branch arguments")?;
    let [mut choice, mut spell]: [Value; 2] = std::mem::take(arguments)
        .try_into()
        .map_err(|_| "casting branch arity")?;
    if spell["kind"] != "Spell" {
        return Err("casting branch needs a spell".into());
    }
    if gift
        && (choice["kind"] != "Gift"
            || !choice["args"]["promised"].is_null()
            || !choice["args"]["unpromised"].is_null())
    {
        return Err("gift header already owns a resolution".into());
    }
    bind_scope(&mut choice, &mut outer.nested())?;
    let beheld = crate::functions::beheld_kind(&choice).filter(|_| kind == Choice::Additional);
    resolve_beheld(&mut spell, beheld, 0)?;
    bind_scope(&mut spell, &mut outer.nested())?;
    if !spell["args"]["announcement"]["modes"].is_null() {
        return Err("modal casting specialization needs its mode contract".into());
    }
    if overload {
        let paid = crate::functions::overload::paid(&spell)?;
        return Ok(json!({"kind":"CostBranch", "args":{
            "keyword":"Overload", "cost":choice, "paid":paid, "unpaid":spell
        }}));
    }
    let mut changed = 0;
    let paid_body = specialize(&spell["args"]["body"], true, kind, &mut changed)?;
    let unpaid_body = specialize(&spell["args"]["body"], false, kind, &mut changed)?;
    if changed == 0 && !gift {
        return Err("no bound source test of the casting choice".into());
    }
    let branch = |body| -> Result<Value, String> {
        let mut ability = spell.clone();
        ability["args"]["body"] = body;
        retain_targets(&mut ability)?;
        Ok(ability)
    };
    if gift {
        choice["args"]["promised"] = branch(paid_body)?;
        choice["args"]["unpromised"] = branch(unpaid_body)?;
        Ok(choice)
    } else {
        Ok(json!({"kind":"CostBranch", "args":{
            "keyword":kind.keyword(), "cost":choice,
            "paid":branch(paid_body)?, "unpaid":branch(unpaid_body)?
        }}))
    }
}

/// "If a Dragon was beheld" asks whether this spell's optional cost to behold a Dragon
/// was paid. Any other cost leaves what was beheld unnamed.
fn resolve_beheld(value: &mut Value, cost: Option<&Value>, depth: usize) -> Result<(), String> {
    if depth > 128 {
        return Err("beheld test depth bound".into());
    }
    if value["kind"] == "If"
        && value["args"]["condition"]["$sym"] == "beheld"
        && names_it(&value["args"]["then"], 0)
    {
        return Err("\"it\" after what was beheld names the beheld object".into());
    }
    if value["$sym"] == "beheld" {
        let what = cost.ok_or("only an optional cost to behold names what was beheld")?;
        if value["args"].get(0) != Some(what) {
            return Err("what was beheld differs from the cost to behold".into());
        }
        *value = Choice::Additional.paid_test();
        return Ok(());
    }
    match value {
        Value::Object(fields) => fields
            .values_mut()
            .try_for_each(|v| resolve_beheld(v, cost, depth + 1)),
        Value::Array(items) => items
            .iter_mut()
            .try_for_each(|v| resolve_beheld(v, cost, depth + 1)),
        _ => Ok(()),
    }
}

fn names_it(value: &Value, depth: usize) -> bool {
    depth <= 128
        && (value["$sym"] == "it"
            || match value {
                Value::Object(fields) => fields.values().any(|v| names_it(v, depth + 1)),
                Value::Array(items) => items.iter().any(|v| names_it(v, depth + 1)),
                _ => false,
            })
}

fn source_tested(condition: &Value, kind: Choice) -> Option<bool> {
    if *condition == kind.paid_test() {
        Some(true)
    } else if condition["kind"] == "Not" {
        source_tested(&condition["args"], kind).map(|value| !value)
    } else {
        None
    }
}

// Only pure boolean structure is projected. Other predicates and nested ability
// frames keep their operands; each gift outcome still validates independently.
fn condition(
    value: &Value,
    selected: bool,
    kind: Choice,
    changed: &mut usize,
) -> Result<Value, String> {
    let gift = kind == Choice::Gift;
    if gift && value["kind"] == "GiftPromised" {
        *changed += 1;
        return Ok(json!(
            value["args"].as_bool().ok_or("gift promise boolean")? == selected
        ));
    }
    if !gift {
        return Ok(if let Some(wanted) = source_tested(value, kind) {
            *changed += 1;
            json!(wanted == selected)
        } else {
            value.clone()
        });
    }
    if value["kind"] == "Not" {
        let inner = condition(&value["args"], selected, kind, changed)?;
        return Ok(if let Some(truth) = inner.as_bool() {
            json!(!truth)
        } else {
            json!({"kind":"Not","args":inner})
        });
    }
    let all = value["kind"] == "AllOf";
    if all || value["kind"] == "AnyOf" {
        let mut remaining = Vec::new();
        for item in value["args"].as_array().ok_or("boolean condition list")? {
            let reduced = condition(item, selected, kind, changed)?;
            match reduced.as_bool() {
                Some(truth) if truth != all => return Ok(json!(truth)),
                Some(_) => (),
                None => remaining.push(reduced),
            }
        }
        return Ok(match remaining.len() {
            0 => json!(all),
            1 => remaining.remove(0),
            _ => json!({"kind":value["kind"],"args":remaining}),
        });
    }
    Ok(value.clone())
}

fn specialize(
    program: &Value,
    paid: bool,
    kind: Choice,
    changed: &mut usize,
) -> Result<Value, String> {
    let instructions = program["instructions"]
        .as_array()
        .ok_or("branch program instructions")?;
    let mut out = Vec::new();
    for instruction in instructions {
        if instruction["kind"] == "If" {
            let args = &instruction["args"];
            let condition = condition(&args["condition"], paid, kind, changed)?;
            if let Some(taken) = condition.as_bool() {
                let key = if taken { "then" } else { "otherwise" };
                let mut inner = specialize(&args[key], paid, kind, changed)?;
                out.append(inner["instructions"].as_array_mut().unwrap());
                continue;
            }
            let mut copy = instruction.clone();
            copy["args"]["condition"] = condition;
            for key in ["then", "otherwise"] {
                copy["args"][key] = specialize(&args[key], paid, kind, changed)?;
            }
            out.push(copy);
        } else if instruction["kind"] == "Group" {
            let mut inner = specialize(&instruction["args"], paid, kind, changed)?;
            out.append(inner["instructions"].as_array_mut().unwrap());
        } else {
            // Nested abilities and their payment histories belong to their own scope.
            out.push(instruction.clone());
        }
    }
    Ok(json!({"instructions":out}))
}

fn retain_targets(ability: &mut Value) -> Result<(), String> {
    let targets = ability["args"]["announcement"]["targets"]
        .as_array()
        .ok_or("branch targets")?;
    if targets.len() > 64 {
        return Err("branch target bound".into());
    }
    let mut keep = vec![false; targets.len()];
    for (index, target) in targets.iter().enumerate() {
        let slot = target["slot"].as_u64().ok_or("branch target slot")?;
        let mut paths = Vec::new();
        slot_uses(&ability["args"]["body"], slot, &mut Vec::new(), &mut paths);
        keep[index] = !paths.is_empty();
    }
    // Compute dependencies once; closing this graph never repeats a tree walk.
    let mut dependencies = vec![vec![false; targets.len()]; targets.len()];
    for (index, target) in targets.iter().enumerate() {
        for (other, spec) in targets.iter().enumerate() {
            let slot = spec["slot"].as_u64().ok_or("branch target slot")?;
            let mut paths = Vec::new();
            slot_uses(target, slot, &mut Vec::new(), &mut paths);
            dependencies[index][other] = !paths.is_empty();
        }
    }
    // A kept target's constraints/filter may depend on an earlier target.
    for _ in 0..targets.len() {
        let before = keep.clone();
        for (index, _) in targets.iter().enumerate().filter(|(i, _)| before[*i]) {
            for (other, dependent) in dependencies[index].iter().enumerate() {
                keep[other] |= dependent;
            }
        }
        if before == keep {
            break;
        }
    }
    ability["args"]["announcement"]["targets"] = Value::Array(
        targets
            .iter()
            .zip(keep)
            .filter(|(_, keep)| *keep)
            .map(|(v, _)| v.clone())
            .collect(),
    );
    // The pinned canonicalizer renumbers definitions and uses together afterward.
    Ok(())
}

// Bare gift definitions describe permanents. A spell must keep the promise's
// resolution alternatives together, including their distinct target lists.
pub(crate) fn check_gift_frames(abilities: &Value) -> Result<(), String> {
    let Some(items) = abilities.as_array() else {
        return Ok(());
    };
    let bare = items.iter().map(super::presentation::core).any(|a| {
        a["kind"] == "Gift" && a["args"]["promised"].is_null() && a["args"]["unpromised"].is_null()
    });
    if bare
        && items.iter().map(super::presentation::core).any(|a| {
            matches!(
                a["kind"].as_str(),
                Some("Spell" | "CostBranch" | "CostOptions")
            )
        })
    {
        return Err("a spell gift needs its resolution alternatives".into());
    }
    Ok(())
}
