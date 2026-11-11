//! The literal enchanted-creature subject owns only its Pump counter exclusion.
use super::{Step, at_path, symbol_name};
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};
fn host() -> Value {
    json!({"kind":"AttachedTo","args":{"kind":"Source"}})
}
fn creature() -> Value {
    json!({"kind":"CardType","args":"Creature"})
}
fn controlled() -> Value {
    json!({"kind":"ControlledBy","args":{"kind":"One","args":{"kind":"AbilityController"}}})
}
fn unary(value: &Value) -> Option<&Value> {
    let [inner] = value["args"].as_array()?.as_slice() else {
        return None;
    };
    Some(inner)
}
/// Preserve the existing unary reference and argument; add inert lexical proof.
pub(crate) fn literal(value: &Value, at: usize) -> Result<Value, String> {
    crate::functions::branch_bounds(value)?;
    if *value != host() {
        return Err("enchanted creature needs literal host".into());
    }
    Ok(json!({"$sym":"enchanted_operand","args":[value],"at":at,"attached_noun":"Creature"}))
}
fn owner(root: &Value, lexical: bool) -> Result<bool, String> {
    if root["kind"] != "Static" || root["args"]["kind"] != "Ordinary" {
        return Ok(false);
    }
    let one = &root["args"]["affected"]["Objects"];
    if one["kind"] != "One" {
        return Ok(false);
    }
    let r = &one["args"];
    if !lexical {
        return Ok(*r == host());
    }
    if symbol_name(r) != Some("enchanted_operand") || r["attached_noun"] != "Creature" {
        return Ok(false);
    }
    let inner = unary(r).ok_or("enchanted creature annotation arity")?;
    if *inner != host() {
        return Err("enchanted creature annotation identity".into());
    }
    Ok(true)
}
fn leaves<'a>(
    v: &'a Value,
    path: &mut Vec<Step>,
    out: &mut Vec<(Vec<Step>, &'a Value)>,
    depth: usize,
) -> bool {
    if depth > 32 || out.len() > 8 {
        return false;
    }
    if v["kind"] == "AllOf" {
        let Some(parts) = v["args"].as_array() else {
            return false;
        };
        for (i, part) in parts.iter().enumerate() {
            path.extend([Step::Key("args".into()), Step::Index(i)]);
            let ok = leaves(part, path, out, depth + 1);
            path.truncate(path.len() - 2);
            if !ok {
                return false;
            }
        }
    } else {
        out.push((path.clone(), v));
    }
    out.len() <= 8
}
fn domain(v: &Value) -> bool {
    if crate::functions::branch_bounds(v).is_err() {
        return false;
    }
    let mut parts = Vec::new();
    if !leaves(v, &mut Vec::new(), &mut parts, 0) {
        return false;
    }
    let (mut c, mut p) = (0, 0);
    for (_, part) in parts {
        if *part == creature() {
            c += 1;
        } else if *part == controlled() {
            p += 1;
        } else {
            return false;
        }
    }
    c == 1 && p <= 1
}
fn lexical_other(r: &Value) -> bool {
    match symbol_name(r) {
        Some("other_reference") => unary(r).is_some_and(domain),
        Some("pump_other_ref") => unary(r).is_some_and(|inner| {
            *inner == json!({"kind":"Source"})
                || symbol_name(inner) == Some("move_after_other")
                    && unary(inner) == Some(&json!({"kind":"Source"}))
        }),
        _ => false,
    }
}
fn count_site(v: &Value, base: &[Step], lexical: bool) -> Option<Vec<Step>> {
    if v["kind"] != "CountCounters"
        || v["args"]["information"] != "CurrentOrLastKnown"
        || v["args"]["recipients"]["kind"] != "Objects"
    {
        return None;
    }
    let mut path = base.to_vec();
    path.extend([
        Step::Key("args".into()),
        Step::Key("recipients".into()),
        Step::Key("args".into()),
    ]);
    let mut objects = &v["args"]["recipients"]["args"];
    if symbol_name(objects) == Some("distributive_pump_subject") {
        objects = unary(objects)?;
        path.extend([Step::Key("args".into()), Step::Index(0)]);
    }
    if objects["kind"] != "Zone"
        || objects["args"]["scope"] != json!({"Shared":"Battlefield"})
        || objects["args"]["kind"] != "Permanent"
    {
        return None;
    }
    path.extend([Step::Key("args".into()), Step::Key("filter".into())]);
    let mut parts = Vec::new();
    if !leaves(&objects["args"]["filter"], &mut path, &mut parts, 0) || parts.len() != 3 {
        return None;
    }
    let (mut c, mut p, mut excluded) = (0, 0, None);
    for (mut path, part) in parts {
        if *part == creature() {
            c += 1;
        } else if *part == controlled() {
            p += 1;
        } else if part["kind"] == "Not" && part["args"]["kind"] == "SameObject" {
            let r = &part["args"]["args"];
            if excluded.is_some()
                || !(if lexical {
                    lexical_other(r)
                } else {
                    *r == host()
                })
            {
                return None;
            }
            path.extend([Step::Key("args".into()), Step::Key("args".into())]);
            excluded = Some(path);
        } else {
            return None;
        }
    }
    if c == 1 && p == 1 { excluded } else { None }
}
fn expressions(
    v: &Value,
    path: &mut Vec<Step>,
    lexical: bool,
    sites: &mut Vec<Vec<Step>>,
    depth: usize,
) -> Result<(), String> {
    if depth > 32 || sites.len() > 64 {
        return Err("attached counter expression bound".into());
    }
    if v["kind"] == "CountCounters" {
        if let Some(site) = count_site(v, path, lexical) {
            sites.push(site);
        }
    } else if matches!(v["kind"].as_str(), Some("Add" | "Multiply")) {
        let Some(parts) = v["args"].as_array() else {
            return Ok(());
        };
        if parts.len() > 16 {
            return Err("attached counter arithmetic bound".into());
        }
        for (i, part) in parts.iter().enumerate() {
            path.extend([Step::Key("args".into()), Step::Index(i)]);
            expressions(part, path, lexical, sites, depth + 1)?;
            path.truncate(path.len() - 2);
        }
    }
    Ok(())
}
fn sites(root: &Value, lexical: bool) -> Result<Vec<Vec<Step>>, String> {
    if !owner(root, lexical)? {
        return Ok(Vec::new());
    }
    crate::functions::branch_bounds(root)?;
    let Some(changes) = root["args"]["changes"].as_array() else {
        return Ok(Vec::new());
    };
    if changes.len() > 64 {
        return Err("attached counter changes bound".into());
    }
    let mut sites = Vec::new();
    for (i, change) in changes.iter().enumerate() {
        if change["kind"] != "Pump" {
            continue;
        }
        for field in ["power", "toughness"] {
            let mut path = vec![
                Step::Key("args".into()),
                Step::Key("changes".into()),
                Step::Index(i),
                Step::Key("args".into()),
                Step::Key(field.into()),
            ];
            expressions(&change["args"][field], &mut path, lexical, &mut sites, 0)?;
        }
    }
    Ok(sites)
}
/// Called for every own binding scope, never crossing to another holder or verb.
pub(super) fn prepare(root: &mut Value) -> Result<(), String> {
    for path in sites(root, true)? {
        *at_path(root, &path).ok_or("attached counter path")? = host();
    }
    Ok(())
}
/// Extra private spellings are appended after ordinary forms. Actual operands
/// remain unchanged; complete semantic reparse proves the lexical ownership.
pub(super) fn surface(root: &Value) -> Result<Option<Value>, String> {
    let sites = sites(root, false)?;
    if sites.is_empty() {
        return Ok(None);
    }
    let _: crate::model::ir::Ability =
        serde_json::from_value(root.clone()).map_err(|e| e.to_string())?;
    let mut copy = root.clone();
    for path in sites {
        *at_path(&mut copy, &path).ok_or("attached counter inverse path")? = json!({"$sym":"attached_counter_other_view","args":[host(),{"kind":"AllOf","args":[creature(),controlled()]}]});
    }
    Ok(Some(copy))
}
pub(crate) fn unify_other<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [pattern] = args else { return Vec::new() };
    if symbol_name(value) != Some("attached_counter_other_view") {
        return Vec::new();
    }
    let Some([reference, filter]) = value["args"].as_array().map(Vec::as_slice) else {
        return Vec::new();
    };
    if *reference != host() || !domain(filter) {
        return Vec::new();
    }
    pattern.unify_made(filter, &bindings)
}
#[cfg(test)]
#[path = "attached_counter_other_tests.rs"]
mod tests;
