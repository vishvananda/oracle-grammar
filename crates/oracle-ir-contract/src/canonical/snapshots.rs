//! A sole-use scalar capture immediately before a pure Snapshot pump is inline.
mod walk;
use crate::ir::Instruction;
use crate::ir_schema::Ty;
use serde_json::{Value, json};
use std::collections::HashMap;
use walk::{Visit, Walker};

#[derive(Default)]
struct Uses {
    total: HashMap<u64, usize>,
    definitions: HashMap<u64, usize>,
    programs: Vec<String>,
    abilities: usize,
}
impl Visit for Uses {
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        defining: bool,
        path: &str,
        _: &mut Walker<'_>,
    ) -> Result<(), String> {
        if matches!(ty, Ty::Named(name) if name == "Ability") {
            self.abilities += 1;
        }
        if matches!(ty, Ty::Named(name) if name == "ResultId")
            && let Some(id) = super::trace::number(value)
        {
            *self.total.entry(id).or_default() += 1;
            if defining {
                *self.definitions.entry(id).or_default() += 1;
            }
        }
        if matches!(ty, Ty::Vec(inner) if matches!(inner.as_ref(), Ty::Named(name) if name == "Instruction"))
        {
            // Postorder paths keep inner arrays stable until their parent is edited.
            self.programs.push(path.into());
        }
        Ok(())
    }
}

struct Pure {
    safe: bool,
    filters: usize,
}
impl Visit for Pure {
    fn enter(&mut self, _: &Value, ty: &Ty) {
        if matches!(ty, Ty::Named(name) if name == "ObjectFilter") {
            self.filters += 1;
        }
    }
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        _: bool,
        _: &str,
        _: &mut Walker<'_>,
    ) -> Result<(), String> {
        let Ty::Named(name) = ty else { return Ok(()) };
        let kind = value["kind"].as_str().unwrap_or_default();
        let allowed = match name.as_str() {
            "ResultId" => false,
            "Expr" => matches!(
                kind,
                "Constant"
                    | "CountObjects"
                    | "CountDistinct"
                    | "Aggregate"
                    | "CountManaSymbols"
                    | "CountCounters"
                    | "ManaSpent"
                    | "PlayerNumber"
                    | "ObjectProperty"
                    | "Add"
                    | "Subtract"
                    | "Multiply"
                    | "Minimum"
                    | "Maximum"
            ),
            "ObjectSet" => matches!(
                kind,
                "Zone"
                    | "Matching"
                    | "Difference"
                    | "Union"
                    | "One"
                    | "Targets"
                    | "InZone"
                    | "Take"
                    | "LibraryTops"
            ),
            "ObjectRef" => {
                matches!(
                    kind,
                    "Source"
                        | "GrantingObject"
                        | "EventSubject"
                        | "EventOther"
                        | "Target"
                        | "Bound"
                        | "AttachedTo"
                        | "Query"
                ) || kind == "Candidate" && self.filters > 0
            }
            "PlayerRef" => matches!(
                kind,
                "AbilityController"
                    | "ControllerOf"
                    | "OwnerOf"
                    | "CasterOf"
                    | "Target"
                    | "Bound"
                    | "EventSubject"
                    | "EventOtherPlayer"
                    | "Active"
                    | "DefendingPlayerOf"
            ),
            "PlayerSet" => matches!(
                kind,
                "One"
                    | "All"
                    | "OpponentsOf"
                    | "OthersThan"
                    | "Union"
                    | "Intersection"
                    | "Matching"
                    | "Targets"
            ),
            "Information" => *value == "CurrentOrLastKnown",
            _ => kind != "Primitive",
        };
        self.safe &= allowed;
        if name == "ObjectFilter" {
            self.filters -= 1;
        }
        Ok(())
    }
}

fn signed_use(value: &Value, id: u64) -> Option<i64> {
    if value["kind"] == "BoundValue" && super::trace::number(&value["args"]) == Some(id) {
        return Some(1);
    }
    if value["kind"] == "Multiply"
        && let Some(parts) = value["args"].as_array()
        && let [negative, bound] = parts.as_slice()
        && negative == &json!({"kind":"Constant","args":-1})
        && bound["kind"] == "BoundValue"
        && super::trace::number(&bound["args"]) == Some(id)
    {
        return Some(-1);
    }
    None
}
fn constant(value: &Value) -> bool {
    value["kind"] == "Constant" && value["args"].as_i64().is_some()
}
fn lossless(instruction: &Value, tracing: bool) -> bool {
    let value = if tracing {
        super::trace::plain(instruction)
    } else {
        instruction.clone()
    };
    serde_json::from_value::<Instruction>(value.clone())
        .is_ok_and(|typed| serde_json::to_value(typed).is_ok_and(|roundtrip| roundtrip == value))
}
fn candidate(first: &Value, next: &Value, uses: &Uses, tracing: bool) -> Option<(u64, Value)> {
    if first["kind"] != "Effect"
        || first["args"]["operation"]["kind"] != "BindValue"
        || next["kind"] != "Effect"
        || next["args"]["operation"]["kind"] != "CreateContinuousEffect"
        || !lossless(first, tracing)
        || !lossless(next, tracing)
    {
        return None;
    }
    let id = super::trace::number(&first["args"]["result"])?;
    let args = &next["args"]["operation"]["args"];
    if args["evaluation"] != "Snapshot" || args["affected"].get("Objects").is_none() {
        return None;
    }
    let [change] = args["changes"].as_array()?.as_slice() else {
        return None;
    };
    if change["kind"] != "Pump" {
        return None;
    }
    let mut reads = 0;
    for field in ["power", "toughness"] {
        let operand = &change["args"][field];
        if signed_use(operand, id).is_some() {
            reads += 1;
        } else if !constant(operand) {
            return None;
        }
    }
    if reads == 0
        || uses.definitions.get(&id) != Some(&1)
        || uses.total.get(&id) != Some(&(reads + 1))
    {
        return None;
    }
    Some((id, first["args"]["operation"]["args"]["value"].clone()))
}
fn amount(value: &Value, sign: i64) -> Option<Value> {
    if sign == 1 {
        return Some(value.clone());
    }
    if constant(value) {
        return value["args"]
            .as_i64()?
            .checked_neg()
            .map(|n| json!({"kind":"Constant","args":n}));
    }
    Some(json!({"kind":"Multiply","args":[{"kind":"Constant","args":-1},value]}))
}
fn inline_array(
    items: &mut Vec<Value>,
    uses: &Uses,
    walker: &mut Walker<'_>,
    tracing: bool,
) -> Result<bool, String> {
    let mut changed = false;
    let mut index = 0;
    while index + 1 < items.len() {
        if let Some((id, mut definition)) =
            candidate(&items[index], &items[index + 1], uses, tracing)
        {
            let mut pure = Pure {
                safe: true,
                filters: 0,
            };
            walker.walk(
                &mut definition,
                &Ty::Named("Expr".into()),
                false,
                "",
                0,
                &mut pure,
            )?;
            if pure.safe {
                let stats = &items[index + 1]["args"]["operation"]["args"]["changes"][0]["args"];
                let replacements: Option<Vec<_>> = ["power", "toughness"]
                    .into_iter()
                    .map(|field| {
                        signed_use(&stats[field], id).map_or_else(
                            || Some(stats[field].clone()),
                            |sign| amount(&definition, sign),
                        )
                    })
                    .collect();
                if let Some(values) = replacements {
                    for (field, value) in ["power", "toughness"].into_iter().zip(values) {
                        items[index + 1]["args"]["operation"]["args"]["changes"][0]["args"]
                            [field] = value;
                    }
                    items.remove(index);
                    changed = true;
                }
            }
        }
        index += 1;
    }
    Ok(changed)
}

#[derive(Default)]
struct Normalize {
    changed: bool,
    owners: Vec<bool>,
    tracing: bool,
}
impl Visit for Normalize {
    fn enter(&mut self, value: &Value, ty: &Ty) {
        if matches!(ty, Ty::Named(name) if name == "Ability") {
            self.owners.push(false);
        } else if matches!(ty, Ty::Named(name) if name == "Operation")
            && value["kind"] == "BindValue"
            && let Some(needed) = self.owners.last_mut()
        {
            *needed = true;
        }
    }
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        _: bool,
        _: &str,
        walker: &mut Walker<'_>,
    ) -> Result<(), String> {
        if !matches!(ty, Ty::Named(name) if name == "Ability") {
            return Ok(());
        }
        if self.owners.pop() != Some(true) {
            return Ok(());
        }
        let mut uses = Uses::default();
        walker.walk(value, ty, false, "", 0, &mut uses)?;
        // Descendant Ability scopes may both shadow and capture outer labels.
        // Leave their enclosing ability alone, independent of numeric spelling;
        // inner abilities were already processed. Delayed Programs stay in this
        // proof so their captures count as later uses.
        if uses.abilities != 1 {
            return Ok(());
        }
        for path in &uses.programs {
            if let Some(items) = value.pointer_mut(path).and_then(Value::as_array_mut) {
                self.changed |= inline_array(items, &uses, walker, self.tracing)?;
            }
        }
        Ok(())
    }
}

pub(super) fn inline(
    value: &mut Value,
    schema: &crate::ir_schema::Schema,
    tracing: bool,
) -> Result<bool, String> {
    let mut visitor = Normalize {
        tracing,
        ..Normalize::default()
    };
    Walker::new(schema).walk(
        value,
        &Ty::Vec(Box::new(Ty::Named("Ability".into()))),
        false,
        "",
        0,
        &mut visitor,
    )?;
    Ok(visitor.changed)
}
