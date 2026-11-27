//! Inverse spelling of populations with separate announced targets.
use super::{
    battlefield_filter, sets,
    walk::{Visit, Walker},
};
use crate::ir_schema::Ty;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const SITE_LIMIT: usize = 4;

#[derive(Clone, Copy)]
enum Population {
    Exiled,
    Damaged,
}

impl Population {
    fn field(self) -> &'static str {
        match self {
            Self::Exiled => "objects",
            Self::Damaged => "recipients",
        }
    }

    fn member(self, set: Value) -> Value {
        match self {
            Self::Exiled => set,
            Self::Damaged => json!({"kind":"Objects","args":set}),
        }
    }
}

fn population(operation: &Value) -> Option<(Population, &[Value])> {
    let args = &operation["args"];
    let (role, objects) = match operation["kind"].as_str()? {
        "Move" if sets::plain_exile(args) => (Population::Exiled, &args["objects"]),
        "DealDamage" if args["recipients"]["kind"] == "Objects" => {
            (Population::Damaged, &args["recipients"]["args"])
        }
        _ => return None,
    };
    let predicate = battlefield_filter(objects)?;
    if predicate["kind"] != "AnyOf" {
        return None;
    }
    let parts = predicate["args"].as_array()?;
    (2..=64).contains(&parts.len()).then_some((role, parts))
}

fn target(filter: &Value) -> Option<&Value> {
    let reference = &filter["args"];
    (filter["kind"] == "SameObject"
        && reference["kind"] == "Target"
        && reference["args"]["index"] == 0)
        .then_some(reference)
}

fn mentioned(value: &Value, depth: usize, work: &mut usize) -> bool {
    if depth > 128 || *work == 0 {
        return false;
    }
    *work -= 1;
    if population(value)
        .is_some_and(|(_, filters)| filters.iter().any(|filter| target(filter).is_some()))
    {
        return true;
    }
    match value {
        Value::Object(fields) => fields.values().any(|item| mentioned(item, depth + 1, work)),
        Value::Array(items) => items.iter().any(|item| mentioned(item, depth + 1, work)),
        _ => false,
    }
}

fn zone(filter: &Value) -> Value {
    json!({"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":filter}})
}

fn split(operation: &Value, slots: &BTreeMap<u64, bool>) -> Option<(Population, Value)> {
    let (role, filters) = population(operation)?;
    let mut targets = BTreeMap::new();
    let mut populations = Vec::new();
    for filter in filters {
        if let Some(reference) = target(filter) {
            let slot = reference["args"]["slot"].as_u64()?;
            let optional = slots.get(&slot)?;
            let objects = if *optional {
                json!({"kind":"Targets","args":slot})
            } else {
                json!({"kind":"One","args":reference})
            };
            if targets.insert(slot, objects).is_some() {
                return None;
            }
        } else {
            populations.push(zone(filter));
        }
    }
    if targets.is_empty() {
        return None;
    }
    let groups = targets
        .into_values()
        .chain(populations)
        .map(|set| role.member(set))
        .collect::<Vec<_>>();
    Some((role, json!({"kind":"Union","args":groups})))
}

#[derive(Default)]
struct Sites {
    slots: Vec<BTreeMap<u64, bool>>,
    variants: Vec<(String, Value)>,
}

fn owns_targets(ty: &Ty) -> bool {
    matches!(ty,Ty::Named(name) if name=="Ability" || name=="Mode")
}

impl Visit for Sites {
    fn enter(&mut self, value: &Value, ty: &Ty) {
        if !owns_targets(ty) {
            return;
        }
        let mut slots = if matches!(ty,Ty::Named(name) if name=="Mode") {
            self.slots.last().cloned().unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        let announcement = if value.get("announcement").is_some() {
            &value["announcement"]
        } else {
            &value["args"]["announcement"]
        };
        if let Some(targets) = announcement["targets"].as_array() {
            for spec in targets {
                if let Some(slot) = spec["slot"].as_u64() {
                    slots.remove(&slot);
                    if sets::single_battlefield_target(spec) {
                        slots.insert(slot, spec["count"].get("UpTo").is_some());
                    }
                }
            }
        }
        self.slots.push(slots);
    }

    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        _: bool,
        path: &str,
        _: &mut Walker<'_>,
    ) -> Result<(), String> {
        if owns_targets(ty) {
            self.slots.pop();
        } else if self.variants.len() < SITE_LIMIT
            && matches!(ty,Ty::Named(name) if name=="Operation")
            && let Some(slots) = self.slots.last()
            && let Some((role, objects)) = split(value, slots)
        {
            self.variants
                .push((format!("{path}/args/{}", role.field()), objects));
        }
        Ok(())
    }
}

/// Restore separated branches from current operands. The proof of equivalence
/// is the same scoped population normalization used during parsing.
pub fn print_views(ability: &Value) -> Result<Vec<Value>, String> {
    if !mentioned(ability, 0, &mut 131_072) {
        return Ok(Vec::new());
    }
    let mut sites = Sites::default();
    let mut original = ability.clone();
    Walker::new(super::schema()?).walk(
        &mut original,
        &Ty::Named("Ability".into()),
        false,
        "",
        0,
        &mut sites,
    )?;
    if sites.variants.is_empty() {
        return Ok(Vec::new());
    }
    let compare = |value: Value| -> Result<Value, String> {
        let mut group = Value::Array(vec![value]);
        super::semantic_abilities(&mut group)?;
        Ok(group)
    };
    let expected = compare(original.clone())?;
    let all = (1 << sites.variants.len()) - 1;
    let mut views = Vec::new();
    for mask in std::iter::once(all).chain(1..all) {
        let mut view = original.clone();
        for (at, (path, objects)) in sites.variants.iter().enumerate() {
            if mask & (1 << at) != 0 {
                *view
                    .pointer_mut(path)
                    .ok_or("population print view lost its operand")? = objects.clone();
            }
        }
        if compare(view.clone())? == expected {
            views.push(view);
        }
    }
    Ok(views)
}
