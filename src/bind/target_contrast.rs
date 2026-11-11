//! Lexical target contrasts preserve all existing fallback-role processing.
use super::{Step, at_path, symbol_name};
use crate::model::ir::{Count, Expr, ObjectRef, TargetSpec};
use serde_json::{Value, json};
use std::cell::Cell;
fn spend(b: &Cell<usize>) -> bool {
    let n = b.get();
    if n == 0 {
        false
    } else {
        b.set(n - 1);
        true
    }
}
#[derive(Clone)]
struct Site {
    path: Vec<Step>,
    domain: Value,
    at: u64,
    prior: bool,
}
fn at<'a>(mut v: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for s in path {
        v = match s {
            Step::Key(k) => v.get(k)?,
            Step::Index(i) => v.get(*i)?,
        };
    }
    Some(v)
}
fn source(v: &Value) -> bool {
    *v == json!({"kind":"Source"})
}
fn fallback(v: &Value) -> bool {
    source(v)
        || symbol_name(v) == Some("pump_other_ref")
            && v["args"].as_array().is_some_and(|a| {
                a.len() == 1
                    && (source(&a[0])
                        || symbol_name(&a[0]) == Some("move_after_other")
                            && a[0]["args"]
                                .as_array()
                                .is_some_and(|b| b.len() == 1 && source(&b[0])))
            })
}
pub(super) fn prepare(root: &mut Value) -> Result<(), String> {
    crate::functions::branch_bounds(root)?;
    fn walk(
        v: &mut Value,
        p: &mut Vec<Step>,
        out: &mut Vec<Site>,
        w: &mut usize,
    ) -> Result<(), String> {
        if p.len() > 128 || *w == 0 {
            return Err("target contrast preparation bound".into());
        }
        *w -= 1;
        if symbol_name(v) == Some("target_contrast") {
            let a = v["args"].as_array().ok_or("target contrast arguments")?;
            if a.len() != 2 || !fallback(&a[0]) || out.len() >= 4096 {
                return Err("target contrast literal fallback".into());
            }
            out.push(Site {
                path: p.clone(),
                domain: a[1].clone(),
                at: v["at"].as_u64().ok_or("target contrast lexical position")?,
                prior: false,
            });
            *v = v["args"][0].take();
        }
        match v {
            Value::Object(o) => {
                for (k, x) in o {
                    p.push(Step::Key(k.clone()));
                    walk(x, p, out, w)?;
                    p.pop();
                }
            }
            Value::Array(a) => {
                for (i, x) in a.iter_mut().enumerate() {
                    p.push(Step::Index(i));
                    walk(x, p, out, w)?;
                    p.pop();
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, &mut Vec::new(), &mut out, &mut 131_072)?;
    let budget = Cell::new(1_048_576);
    for site in &mut out {
        site.prior = own(root, &site.path).is_some_and(|owner| lexical_prior(owner, site, &budget));
    }
    if budget.get() == 0 {
        return Err("target contrast lexical proof work bound".into());
    }
    for site in out.into_iter().filter(|s| s.prior) {
        let Some(op) = operation_path(root, &site.path) else {
            continue;
        };
        let Some(relative) = relative_path(root, &op, &site.path) else {
            continue;
        };
        let mut noun = Vec::new();
        if !atoms(&site.domain, &mut noun, 0, &budget) {
            return Err("target contrast noun work bound".into());
        }
        let encoded: Vec<_> = noun.iter().map(|a| json!([a["kind"], a["args"]])).collect();
        let proof = json!({"path":relative,"noun":encoded});
        let args = at_path(root, &op[..op.len() - 1]).ok_or("target contrast effect owner")?;
        let list = args
            .as_object_mut()
            .ok_or("target contrast effect arguments")?
            .entry("_target_contrast_sites")
            .or_insert_with(|| json!([]));
        list.as_array_mut()
            .ok_or("target contrast site metadata")?
            .push(proof);
    }
    if budget.get() == 0 {
        return Err("target contrast noun work bound".into());
    }
    Ok(())
}
fn relative_path(root: &Value, op: &[Step], path: &[Step]) -> Option<Vec<Value>> {
    let mut out = Vec::new();
    let mut i = op.len();
    while i < path.len() {
        let v = at(root, &path[..i])?;
        let transparent = matches!(
            symbol_name(v),
            Some(
                "in_filter"
                    | "distributive_pump_subject"
                    | "sacrificed_result"
                    | "remember_object"
                    | "remember_operand"
                    | "created_result"
                    | "selected_result"
                    | "exiled_card_result"
                    | "drafted_result"
                    | "manifest_result"
                    | "search_move_result"
                    | "seek_move_result"
            )
        );
        if transparent
            && matches!(path.get(i),Some(Step::Key(k))if k=="args")
            && matches!(path.get(i + 1), Some(Step::Index(0)))
        {
            i += 2;
            continue;
        }
        out.push(match &path[i] {
            Step::Key(k) => json!(k),
            Step::Index(n) => json!(n),
        });
        i += 1;
    }
    Some(out)
}
fn atoms(v: &Value, out: &mut Vec<Value>, depth: usize, budget: &Cell<usize>) -> bool {
    if depth > 32 || out.len() > 32 || !spend(budget) {
        return false;
    }
    match v["kind"].as_str() {
        Some("CardType" | "Subtype") => out.push(v.clone()),
        Some("AllOf") => {
            if let Some(a) = v["args"].as_array() {
                for x in a {
                    if !atoms(x, out, depth + 1, budget) {
                        return false;
                    }
                }
            }
        }
        _ => {}
    }
    true
}
fn implies(v: &Value, a: &Value, d: usize, budget: &Cell<usize>) -> bool {
    if d > 32 || !spend(budget) {
        return false;
    }
    if v == a {
        return true;
    }
    let Some(xs) = v["args"].as_array() else {
        return false;
    };
    match v["kind"].as_str() {
        Some("AllOf") => xs.iter().any(|x| implies(x, a, d + 1, budget)),
        Some("AnyOf") => !xs.is_empty() && xs.iter().all(|x| implies(x, a, d + 1, budget)),
        _ => false,
    }
}
fn own<'a>(root: &'a Value, p: &[Step]) -> Option<&'a Value> {
    for n in (0..p.len()).rev() {
        let v = at(root, &p[..n])?;
        if matches!(
            v["kind"].as_str(),
            Some("GrantAbility" | "CreateTrigger" | "CreateDelayedTrigger" | "ReflexiveTrigger")
        ) {
            return None;
        }
    }
    for n in (0..p.len()).rev() {
        let v = at(root, &p[..n])?;
        if v["announcement"].is_object() && v["body"]["instructions"].is_array() {
            return Some(v);
        }
        if v["args"]["announcement"].is_object() && v["args"]["body"]["instructions"].is_array() {
            return Some(&v["args"]);
        }
    }
    None
}

fn lexical_prior(owner: &Value, site: &Site, budget: &Cell<usize>) -> bool {
    fn scan(
        v: &Value,
        root: bool,
        out: &mut Vec<(u64, Value)>,
        work: &mut usize,
        depth: usize,
    ) -> bool {
        if depth > 128 || *work == 0 || out.len() > 4096 {
            return false;
        }
        *work -= 1;
        if !root
            && (matches!(
                v["kind"].as_str(),
                Some(
                    "GrantAbility"
                        | "Triggered"
                        | "Activated"
                        | "Spell"
                        | "CreateTrigger"
                        | "CreateDelayedTrigger"
                        | "ReflexiveTrigger"
                )
            ) || v["announcement"].is_object() && v["body"].is_object())
        {
            return true;
        }
        if symbol_name(v) == Some("target")
            && let (Some(at), Some(spec)) = (v["at"].as_u64(), v["args"].get(0))
        {
            let pair = (at, spec.clone());
            if !out.contains(&pair) {
                out.push(pair);
            }
        }
        match v {
            Value::Object(o) => {
                for (k, x) in o {
                    if k != "cost" && k != "announcement" && !scan(x, false, out, work, depth + 1) {
                        return false;
                    }
                }
            }
            Value::Array(a) => {
                for x in a {
                    if !scan(x, false, out, work, depth + 1) {
                        return false;
                    }
                }
            }
            _ => {}
        }
        true
    }
    let mut noun = Vec::new();
    if !atoms(&site.domain, &mut noun, 0, budget) || noun.is_empty() {
        return false;
    }
    let mut targets = Vec::new();
    let mut work = budget.get().min(131_072);
    let initial = work;
    let scanned = scan(owner, true, &mut targets, &mut work, 0);
    budget.set(budget.get().saturating_sub(initial - work));
    if !scanned {
        budget.set(0);
        return false;
    }
    let compatible: Vec<_> = targets
        .iter()
        .filter(|(_, s)| {
            s["count"] == json!({"Exactly":{"kind":"Constant","args":1}})
                && s["kind"]["Object"]["kind"] == "Permanent"
                && s["kind"]["Object"]["zones"] == json!(["Battlefield"])
                && noun.iter().all(|a| implies(&s["filter"], a, 0, budget))
        })
        .collect();
    matches!(compatible.as_slice(), [(at, _) ] if *at < site.at)
}

fn target(v: &Value, own: &Value, domain: &Value, budget: &Cell<usize>) -> Option<Value> {
    let ObjectRef::Target { slot, index } = serde_json::from_value(v.clone()).ok()? else {
        return None;
    };
    if index != 0 {
        return None;
    }
    let mut noun = Vec::new();
    if !atoms(domain, &mut noun, 0, budget) || noun.is_empty() {
        return None;
    }
    for raw in own["announcement"]["targets"].as_array()? {
        if !spend(budget) {
            return None;
        }
        let spec: TargetSpec = serde_json::from_value(raw.clone()).ok()?;
        if spec.slot.0 != slot.0 || !matches!(spec.count, Count::Exactly(Expr::Constant(1))) {
            continue;
        }
        if raw["kind"]["Object"]["kind"] != "Permanent"
            || raw["kind"]["Object"]["zones"] != json!(["Battlefield"])
        {
            continue;
        }
        if noun.iter().all(|a| implies(&raw["filter"], a, 0, budget)) {
            return Some(v.clone());
        }
    }
    None
}
fn one(v: &Value) -> Option<&Value> {
    (v["kind"] == "One").then_some(&v["args"])
}
fn effect(v: &Value) -> Option<&Value> {
    (v["kind"] == "Effect").then_some(&v["args"]["operation"])
}
fn operation_path(root: &Value, p: &[Step]) -> Option<Vec<Step>> {
    // Mode bodies live beneath the outer announcement. Only that mode's own
    // announcement and costs are barriers to its resolving effect operands.
    let start = (0..p.len())
        .rev()
        .find_map(|n| {
            (matches!(&p[n], Step::Key(k) if k == "body")
                && at(root, &p[..n]).is_some_and(|v| {
                    v["announcement"].is_object() && v["body"]["instructions"].is_array()
                }))
            .then_some(n + 1)
        })
        .unwrap_or(0);
    if p[start..]
        .iter()
        .any(|s| matches!(s,Step::Key(k)if k=="cost"||k=="announcement"))
    {
        return None;
    }
    for n in (0..p.len()).rev() {
        if at(root, &p[..n])?["kind"] == "Effect" {
            let mut q = p[..n].to_vec();
            q.extend([Step::Key("args".into()), Step::Key("operation".into())]);
            return Some(q);
        }
    }
    None
}
fn field(p: &[Step], base: usize, name: &str) -> bool {
    matches!(p.get(base),Some(Step::Key(k))if k=="args")
        && matches!(p.get(base+1),Some(Step::Key(k))if k==name)
}
fn previous<'a>(root: &'a Value, p: &[Step]) -> Option<(&'a [Value], usize)> {
    for n in (1..p.len()).rev() {
        if let Step::Index(i) = p[n]
            && matches!(&p[n-1],Step::Key(k)if k=="instructions")
        {
            return Some((at(root, &p[..n])?.as_array()?, i));
        }
    }
    None
}
fn previous_target(root: &Value, p: &[Step], kind: &str) -> Option<Value> {
    let (a, i) = previous(root, p)?;
    if i == 0 {
        return None;
    }
    let op = effect(a.get(i - 1)?)?;
    match kind {
        "Sacrifice" if op["kind"] == "PutCounters" => {
            let [q] = op["args"]["placements"].as_array()?.as_slice() else {
                return None;
            };
            one(&q["recipients"]["args"]).cloned()
        }
        "CreateContinuousEffect" if op["kind"] == "CreateContinuousEffect" => {
            let [c] = op["args"]["changes"].as_array()?.as_slice() else {
                return None;
            };
            (c["kind"] == "Pump")
                .then(|| one(&op["args"]["affected"]["Objects"]).cloned())
                .flatten()
        }
        _ => None,
    }
}
fn regenerate_target(root: &Value, p: &[Step]) -> Option<Value> {
    for n in (0..p.len()).rev() {
        let v = at(root, &p[..n])?;
        if v["kind"] == "If"
            && v["args"]["otherwise"]["instructions"] == json!([])
            && field(p, n, "then")
        {
            let (a, i) = previous(root, &p[..n])?;
            if i < 2 || effect(&a[i - 1])?["kind"] != "Draw" {
                return None;
            }
            let op = effect(&a[i - 2])?;
            return (op["kind"] == "CreateRegenerationShields")
                .then(|| one(&op["args"]["objects"]).cloned())
                .flatten();
        }
    }
    None
}
fn owning_spell(root: &Value, path: &[Step]) -> bool {
    for n in (0..=path.len()).rev() {
        let Some(v) = at(root, &path[..n]) else {
            return false;
        };
        match v["kind"].as_str() {
            Some("Spell") => return true,
            Some(
                "Activated" | "Triggered" | "CreateTrigger" | "ReflexiveTrigger" | "GrantAbility",
            ) => return false,
            _ => {}
        }
    }
    false
}
fn anchor(root: &Value, path: &[Step], domain: &Value, budget: &Cell<usize>) -> Option<Value> {
    if !spend(budget) {
        return None;
    }
    let owner = own(root, path)?;
    let p = operation_path(root, path)?;
    let op = at(root, &p)?;
    let base = p.len();
    let candidate = match op["kind"].as_str()? {
        "DealDamage" if field(path, base, "recipients") => {
            if let Some(t) = target(&op["args"]["source"], owner, domain, budget) {
                return Some(t);
            }
            if owning_spell(root, path) || op["args"]["recipients"]["kind"] != "Union" {
                return None;
            }
            let a = op["args"]["recipients"]["args"].as_array()?;
            let ts: Vec<_> = a
                .iter()
                .filter(|r| r["kind"] == "Objects")
                .filter_map(|r| one(&r["args"]))
                .filter_map(|r| target(r, owner, domain, budget))
                .collect();
            let [t] = ts.as_slice() else {
                return None;
            };
            return Some(t.clone());
        }
        "Sacrifice" if field(path, base, "objects") => previous_target(root, &p, "Sacrifice")?,
        "CreateRegenerationShields" if field(path, base, "objects") => regenerate_target(root, &p)?,
        "CreateContinuousEffect" => {
            if field(path, base, "changes") {
                one(&op["args"]["affected"]["Objects"])?.clone()
            } else if field(path, base, "affected") {
                let a = op["args"]["changes"].as_array()?;
                let cs: Vec<_> = a.iter().filter(|c| c["kind"] == "CopyOf").collect();
                if let [c] = cs.as_slice() {
                    c["args"]["object"].clone()
                } else {
                    previous_target(root, &p, "CreateContinuousEffect")?
                }
            } else {
                return None;
            }
        }
        _ => return None,
    };
    target(&candidate, owner, domain, budget)
}
pub(super) fn lower(root: &mut Value) -> Result<(), String> {
    crate::functions::branch_bounds(root)?;
    fn collect(
        v: &mut Value,
        path: &mut Vec<Step>,
        out: &mut Vec<(Vec<Step>, Value)>,
        work: &mut usize,
    ) -> Result<(), String> {
        if path.len() > 128 || *work == 0 {
            return Err("target contrast bound-site work bound".into());
        }
        *work -= 1;
        if v["kind"] == "Effect" {
            let metadata = v["args"]
                .as_object_mut()
                .and_then(|a| a.remove("_target_contrast_sites"));
            if let Some(entries) = metadata {
                for entry in entries.as_array().ok_or("target contrast bound metadata")? {
                    if out.len() >= 4096 {
                        return Err("target contrast bound-site count".into());
                    }
                    let mut p = path.clone();
                    p.extend([Step::Key("args".into()), Step::Key("operation".into())]);
                    for step in entry["path"]
                        .as_array()
                        .ok_or("target contrast relative path")?
                    {
                        p.push(if let Some(k) = step.as_str() {
                            Step::Key(k.to_owned())
                        } else {
                            Step::Index(
                                usize::try_from(
                                    step.as_u64().ok_or("target contrast array address")?,
                                )
                                .map_err(|_| "target contrast array address")?,
                            )
                        });
                    }
                    let mut noun = Vec::new();
                    for atom in entry["noun"]
                        .as_array()
                        .ok_or("target contrast noun metadata")?
                    {
                        let a = atom.as_array().ok_or("target contrast noun atom")?;
                        if a.len() != 2
                            || !matches!(a[0].as_str(), Some("CardType" | "Subtype"))
                            || !a[1].is_string()
                            || noun.len() >= 33
                        {
                            return Err("target contrast noun atom".into());
                        }
                        noun.push(json!({"kind":a[0],"args":a[1]}));
                    }
                    out.push((p, json!({"kind":"AllOf","args":noun})));
                }
            }
        }
        match v {
            Value::Object(o) => {
                for (k, x) in o {
                    path.push(Step::Key(k.clone()));
                    collect(x, path, out, work)?;
                    path.pop();
                }
            }
            Value::Array(a) => {
                for (i, x) in a.iter_mut().enumerate() {
                    path.push(Step::Index(i));
                    collect(x, path, out, work)?;
                    path.pop();
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut sites = Vec::new();
    collect(root, &mut Vec::new(), &mut sites, &mut 131_072)?;
    let budget = Cell::new(1_048_576);
    for (path, domain) in sites {
        if at(root, &path).is_some_and(source)
            && let Some(t) = anchor(root, &path, &domain, &budget)
            && let Some(v) = at_path(root, &path)
        {
            *v = t;
        }
    }
    if budget.get() == 0 {
        return Err("target contrast lowering work bound".into());
    }
    Ok(())
}
/// The inverse view uses the same full parent proof, never a standalone reference alias.
pub(crate) fn projection(root: &Value) -> Result<Option<Value>, String> {
    crate::functions::branch_bounds(root)?;
    fn walk(
        root: &Value,
        v: &Value,
        p: &mut Vec<Step>,
        out: &mut Vec<Vec<Step>>,
        w: &mut usize,
        budget: &Cell<usize>,
    ) -> Result<(), String> {
        if p.len() > 128 || *w == 0 {
            return Err("target contrast inverse bound".into());
        }
        *w -= 1;
        if v["kind"] == "Not"
            && v["args"]["kind"] == "SameObject"
            && v["args"]["args"]["kind"] == "Target"
        {
            let mut domain = None;
            for n in (0..p.len()).rev() {
                let a = at(root, &p[..n]).ok_or("target contrast inverse path")?;
                if a["kind"] == "Zone" {
                    domain = Some(a["args"]["filter"].clone());
                    break;
                }
            }
            p.extend([Step::Key("args".into()), Step::Key("args".into())]);
            if let Some(d) = domain
                && anchor(root, p, &d, budget).as_ref() == Some(&v["args"]["args"])
            {
                if out.len() >= 4096 {
                    return Err("target contrast inverse site bound".into());
                }
                out.push(p.clone());
            }
            p.truncate(p.len() - 2);
        }
        match v {
            Value::Object(o) => {
                for (k, x) in o {
                    p.push(Step::Key(k.clone()));
                    walk(root, x, p, out, w, budget)?;
                    p.pop();
                }
            }
            Value::Array(a) => {
                for (i, x) in a.iter().enumerate() {
                    p.push(Step::Index(i));
                    walk(root, x, p, out, w, budget)?;
                    p.pop();
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut paths = Vec::new();
    let budget = Cell::new(1_048_576);
    walk(
        root,
        root,
        &mut Vec::new(),
        &mut paths,
        &mut 131_072,
        &budget,
    )?;
    if budget.get() == 0 {
        return Err("target contrast inverse proof work bound".into());
    }
    if paths.is_empty() {
        return Ok(None);
    }
    let mut view = root.clone();
    for p in paths {
        if let Some(v) = at_path(&mut view, &p) {
            *v = json!({"kind":"Source"});
        }
    }
    Ok(Some(view))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source_site(v: &Value, p: &mut Vec<Step>) -> Option<Vec<Step>> {
        if v["kind"] == "Not" && v["args"]["kind"] == "SameObject" && source(&v["args"]["args"]) {
            let mut q = p.clone();
            q.extend([Step::Key("args".into()), Step::Key("args".into())]);
            return Some(q);
        }
        match v {
            Value::Object(o) => {
                for (k, x) in o {
                    p.push(Step::Key(k.clone()));
                    let found = source_site(x, p);
                    p.pop();
                    if found.is_some() {
                        return found;
                    }
                }
            }
            Value::Array(a) => {
                for (i, x) in a.iter().enumerate() {
                    p.push(Step::Index(i));
                    let found = source_site(x, p);
                    p.pop();
                    if found.is_some() {
                        return found;
                    }
                }
            }
            _ => {}
        }
        None
    }
    fn tagged_spell() -> (Value, Value) {
        let data: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/target-contrast-reviewed.json"
        ))
        .unwrap();
        for c in data["protected"].as_array().unwrap() {
            if c["name"] != "Brightflame" {
                continue;
            }
            for frame in c["expected_all"].as_array().unwrap() {
                let spell = frame[0].clone();
                let Some(site) = source_site(&spell, &mut Vec::new()) else {
                    continue;
                };
                assert!(serde_json::from_value::<crate::model::ir::Ability>(spell.clone()).is_ok());
                let op = operation_path(&spell, &site).unwrap();
                let path = relative_path(&spell, &op, &site).unwrap();
                let mut marked = spell.clone();
                let args = at_path(&mut marked, &op[..op.len() - 1]).unwrap();
                args["_target_contrast_sites"] =
                    json!([{"path":path,"noun":[["CardType","Creature"]]}]);
                return (spell, marked);
            }
        }
        panic!("protected source-spell Union frame");
    }
    #[test]
    fn target_contrast_scoped_spell_union_keeps_its_proper_source_exclusion() {
        let (before, marked) = tagged_spell();
        let expected =
            json!({"kind":"Scoped","args":{"zones":["Stack"],"condition":null,"ability":before}});
        let mut actual =
            json!({"kind":"Scoped","args":{"zones":["Stack"],"condition":null,"ability":marked}});
        lower(&mut actual).unwrap();
        assert_eq!(actual, expected);
    }
    #[test]
    fn target_contrast_cost_branch_spell_union_keeps_both_proper_resolutions() {
        let (before, marked) = tagged_spell();
        let cost = json!({"parts":[{"Mana":[{"Generic":1}]}]});
        let expected = json!({"kind":"CostBranch","args":{"keyword":"Kicker","cost":cost,"paid":before,"unpaid":before}});
        let mut actual = json!({"kind":"CostBranch","args":{"keyword":"Kicker","cost":cost,"paid":marked,"unpaid":marked}});
        lower(&mut actual).unwrap();
        assert_eq!(actual, expected);
    }
    #[test]
    fn target_contrast_rejects_auxiliary_depth_before_cloning_or_erasing_it() {
        let mut domain = json!({"kind":"CardType","args":"Creature"});
        for _ in 0..160 {
            domain = json!({"kind":"AllOf","args":[domain]});
        }
        let mut raw = json!({"$sym":"target_contrast","at":1,"args":[{"kind":"Source"},domain]});
        assert!(prepare(&mut raw).is_err());
    }
}
