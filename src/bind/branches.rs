//! A casting branch specializes bound source-payment tests, not Oracle text.
use super::{Scope, bind_scope, json, slot_uses};
use serde_json::Value;

pub(super) fn bind(node: &mut Value, outer: &Scope) -> Result<Value, String> {
    crate::functions::branch_bounds(node)?;
    let arguments = node["args"]
        .as_array_mut()
        .ok_or("kicker branch arguments")?;
    let [mut cost, mut spell]: [Value; 2] = std::mem::take(arguments)
        .try_into()
        .map_err(|_| "kicker branch arity")?;
    if spell["kind"] != "Spell" {
        return Err("kicker branch needs a spell".into());
    }
    bind_scope(&mut cost, &mut outer.nested())?;
    bind_scope(&mut spell, &mut outer.nested())?;
    if !spell["args"]["announcement"]["modes"].is_null() {
        return Err("modal kicker specialization needs its mode contract".into());
    }
    let mut changed = 0;
    let paid_body = specialize(&spell["args"]["body"], true, &mut changed)?;
    let unpaid_body = specialize(&spell["args"]["body"], false, &mut changed)?;
    if changed == 0 {
        return Err("no bound source kicker test".into());
    }
    let branch = |body| -> Result<Value, String> {
        let mut ability = spell.clone();
        ability["args"]["body"] = body;
        retain_targets(&mut ability)?;
        Ok(ability)
    };
    Ok(json!({"kind":"CostBranch", "args":{
        "keyword":"Kicker", "cost":cost,
        "paid":branch(paid_body)?, "unpaid":branch(unpaid_body)?
    }}))
}

fn source_kicked(condition: &Value) -> Option<bool> {
    if condition
        == &json!({"kind":"ObjectMatches", "args":{
            "object":{"kind":"Source"}, "filter":{"kind":"State","args":"Kicked"}
        }})
    {
        Some(true)
    } else if condition["kind"] == "Not" {
        source_kicked(&condition["args"]).map(|value| !value)
    } else {
        None
    }
}

fn specialize(program: &Value, paid: bool, changed: &mut usize) -> Result<Value, String> {
    let instructions = program["instructions"]
        .as_array()
        .ok_or("branch program instructions")?;
    let mut out = Vec::new();
    for instruction in instructions {
        if instruction["kind"] == "If" {
            let args = &instruction["args"];
            if let Some(wanted) = source_kicked(&args["condition"]) {
                *changed += 1;
                let key = if wanted == paid { "then" } else { "otherwise" };
                let mut inner = specialize(&args[key], paid, changed)?;
                out.append(inner["instructions"].as_array_mut().unwrap());
                continue;
            }
            let mut copy = instruction.clone();
            for key in ["then", "otherwise"] {
                copy["args"][key] = specialize(&args[key], paid, changed)?;
            }
            out.push(copy);
        } else if instruction["kind"] == "Group" {
            let mut inner = specialize(&instruction["args"], paid, changed)?;
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
