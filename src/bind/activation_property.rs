//! A lexical creature property may name one proved activation sacrifice input.
use super::{Scope, Step, at_path};
use crate::model::ir::{Cost, CostPart, Count, EventZone, Expr, ObjectKind, ObjectSet, ZoneScope};
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

pub(super) const NAME: &str = "activation_creature_property";

pub(super) fn marker(
    root: &mut Value,
    scope: &Scope,
    path: &[Step],
    name: &str,
) -> Result<bool, String> {
    if name != NAME {
        return Ok(false);
    }
    let fallback = at_path(root, path).ok_or("activation property path")?["args"][0].clone();
    let reference = reference(root, scope, path, fallback)?;
    *at_path(root, path).ok_or("activation property path")? = reference;
    Ok(true)
}

fn reference(root: &Value, scope: &Scope, path: &[Step], fallback: Value) -> Result<Value, String> {
    // A named target, receipt, holder or explicit source keeps its own identity.
    // This annotation wraps only the existing literal `that creature's` branch.
    if root["kind"] != "Activated" || fallback != json!({"kind":"Source"}) {
        return Ok(fallback);
    }
    let Some((instruction, field)) = property_path(root, path) else {
        return Ok(fallback);
    };
    if !immediate(root, instruction, field, scope) {
        return Ok(fallback);
    }
    let mut raw = root["args"]["cost"].clone();
    super::filters::strip(&mut raw);
    let cost: Cost =
        serde_json::from_value(raw).map_err(|e| format!("activation property cost: {e}"))?;
    if !cost.parts.iter().any(|p| has_sacrifice(p, 32)) {
        return Ok(fallback);
    }
    let announcement = &root["args"]["announcement"];
    // @announcement is materialized only by finish_scope. Prove its empty
    // target set using both already introduced and not-yet-bound targets.
    let pending = super::symbol_name(announcement) == Some("announcement")
        && announcement["args"].as_array().is_some_and(Vec::is_empty)
        && announcement.get("targets").is_none()
        && announcement.get("modes").is_none();
    let declared = announcement.get("$sym").is_none()
        && announcement["targets"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && announcement["modes"].is_null();
    if !(pending || declared)
        || !scope.targets.is_empty()
        || super::references::target_footprint(root)? != (0, 0)
    {
        return Err(
            "activation sacrifice property needs an unambiguous unannounced subject".into(),
        );
    }
    let mut component = None;
    for (index, part) in cost.parts.iter().enumerate() {
        match part {
            CostPart::Mana(_) | CostPart::TapSymbol | CostPart::Life(Expr::Constant(_)) => {}
            CostPart::Sacrifice { objects, choose } => {
                // Sacrificing the holder is not an unnamed chosen creature.
                if cost
                    .parts
                    .iter()
                    .filter(|p| matches!(p, CostPart::Sacrifice { .. }))
                    .count()
                    == 1
                    && matches!(objects, ObjectSet::One(_))
                    && choose.is_none()
                {
                    return Ok(fallback);
                }
                if component.is_some() {
                    return Err("activation property has multiple sacrifice components".into());
                }
                if !matches!(choose, Some(Count::Exactly(Expr::Constant(1)))) {
                    return Err("activation property needs exactly one sacrifice input".into());
                }
                let ObjectSet::Zone {
                    scope: ZoneScope::Shared(EventZone::Battlefield),
                    kind: ObjectKind::Permanent,
                    filter,
                } = objects
                else {
                    return Err("activation property needs a battlefield creature selection".into());
                };
                let filter = serde_json::to_value(filter).expect("typed sacrifice filter");
                if !creature(&filter, 32) {
                    return Err("activation property does not prove a creature input".into());
                }
                component = Some(index);
            }
            _ => return Err("activation property has an unsupported payment composition".into()),
        }
    }
    let index = component.ok_or("activation property lacks one direct sacrifice component")?;
    Ok(
        json!({"kind":"Query","args":{"objects":{"kind":"CostObjects","args":{
        "payment":"Activation","component":[index],"view":"Inputs","phase":"Before"
    }},"index":0}}),
    )
}

fn has_sacrifice(part: &CostPart, depth: usize) -> bool {
    if depth == 0 {
        return true; // An unproved deep composition cannot become a Source fallback.
    }
    match part {
        CostPart::Sacrifice { .. } => true,
        CostPart::Choice(costs) => costs
            .iter()
            .any(|c| c.parts.iter().any(|p| has_sacrifice(p, depth - 1))),
        CostPart::Constrained { part, .. } | CostPart::Repeated { part, .. } => {
            has_sacrifice(part, depth - 1)
        }
        _ => false,
    }
}

fn creature(filter: &Value, depth: usize) -> bool {
    if depth == 0 {
        return false;
    }
    if filter == &json!({"kind":"CardType","args":"Creature"}) {
        return true;
    }
    let Some(parts) = filter["args"].as_array() else {
        return false;
    };
    match filter["kind"].as_str() {
        Some("AllOf") => parts.iter().any(|f| creature(f, depth - 1)),
        Some("AnyOf") => !parts.is_empty() && parts.iter().all(|f| creature(f, depth - 1)),
        _ => false,
    }
}

/// Only arithmetic edges in one immediate amount can lead to this property.
fn property_path<'a>(root: &Value, path: &'a [Step]) -> Option<(usize, &'a str)> {
    let [
        Step::Key(a),
        Step::Key(b),
        Step::Key(c),
        Step::Index(index),
        Step::Key(d),
        Step::Key(e),
        Step::Key(f),
        Step::Key(field),
        rest @ ..,
    ] = path
    else {
        return None;
    };
    if (
        a.as_str(),
        b.as_str(),
        c.as_str(),
        d.as_str(),
        e.as_str(),
        f.as_str(),
    ) != ("args", "body", "instructions", "args", "operation", "args")
    {
        return None;
    }
    let operation = &root["args"]["body"]["instructions"][*index]["args"]["operation"];
    if !matches!(
        (operation["kind"].as_str(), field.as_str()),
        (Some("Draw"), "amount") | (Some("BindValue"), "value")
    ) {
        return None;
    }
    let mut value = &operation["args"][field];
    let mut rest = rest;
    while let [Step::Key(args), Step::Index(index), tail @ ..] = rest {
        if args != "args" || !matches!(value["kind"].as_str(), Some("Add" | "Multiply")) {
            return None;
        }
        value = value.get("args")?.get(*index)?;
        rest = tail;
    }
    if !matches!(rest, [Step::Key(args), Step::Key(object)] if args == "args" && object == "object")
        || value["kind"] != "ObjectProperty"
        || value["args"]["information"] != "CurrentOrLastKnown"
    {
        return None;
    }
    Some((*index, field))
}

fn immediate(root: &Value, index: usize, field: &str, scope: &Scope) -> bool {
    let Some(items) = root["args"]["body"]["instructions"].as_array() else {
        return false;
    };
    match items.as_slice() {
        [one] => index == 0 && field == "amount" && one["args"]["operation"]["kind"] == "Draw",
        [capture, draw] => {
            index == 0
                && field == "value"
                && capture["args"]["operation"]["kind"] == "BindValue"
                && draw["args"]["operation"]["kind"] == "Draw"
                && draw["args"]["operation"]["args"]["amount"]
                    == json!({"kind":"BoundValue","args":capture["args"]["result"]})
                && !scope.snapshot_payment
        }
        _ => false,
    }
}

/// Ordinary references keep their inverse. An activation paid-object proposal
/// admits only one exact Inputs/Before component; verified reparse checks owner.
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else {
        return Vec::new();
    };
    let objects = &value["args"]["objects"];
    if value["kind"] == "Query"
        && objects["kind"] == "CostObjects"
        && objects["args"]["payment"] == "Activation"
    {
        let paid = &objects["args"];
        if value["args"]["index"] != 0
            || paid["view"] != "Inputs"
            || paid["phase"] != "Before"
            || !paid["component"].as_array().is_some_and(|p| {
                p.len() == 1 && p[0].as_u64().is_some_and(|n| u32::try_from(n).is_ok())
            })
        {
            return Vec::new();
        }
    }
    inner.unify(value, bindings)
}

#[cfg(test)]
#[path = "activation_property_tests.rs"]
mod tests;
