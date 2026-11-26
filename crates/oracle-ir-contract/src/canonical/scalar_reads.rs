//! Deterministic scalar reads and signed substitutions at one evaluation point.
use super::walk::{Visit, Walker};
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
                    | "AnnouncedX"
                    | "EventAmount"
                    | "PartySize"
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

pub(super) fn signed_use(value: &Value, id: u64) -> Option<i64> {
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
pub(super) fn constant(value: &Value) -> bool {
    value["kind"] == "Constant" && value["args"].as_i64().is_some()
}
pub(super) fn amount(value: &Value, sign: i64) -> Option<Value> {
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
pub(super) fn pure(value: &mut Value, walker: &mut Walker<'_>) -> Result<bool, String> {
    let mut proof = Pure {
        safe: true,
        filters: 0,
    };
    walker.walk(value, &Ty::Named("Expr".into()), false, "", 0, &mut proof)?;
    Ok(proof.safe)
}
