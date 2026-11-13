//! A sole-use scalar capture immediately before a pure Snapshot pump is inline.
use super::walk::{Uses, Visit, Walker, lossless};
use crate::ir_schema::Ty;
use serde_json::{Value, json};

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
fn candidate(first: &Value, next: &Value, uses: &Uses, tracing: bool) -> Option<(u64, Value)> {
    if first["kind"] != "Effect"
        || first["args"]["operation"]["kind"] != "BindValue"
        || next["kind"] != "Effect"
        || next["args"]["operation"]["kind"] != "CreateContinuousEffect"
        || lossless::<crate::ir::Instruction>(first, tracing).is_none()
        || lossless::<crate::ir::Instruction>(next, tracing).is_none()
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
pub(super) fn inline_array(
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
