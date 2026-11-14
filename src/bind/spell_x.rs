//! Within a spell's own casting frame, Source.SpellX is its announced X,
//! including a copied spell's copied choice. Independent frames retain their
//! explicit source/history/activation expressions.
use crate::model::ir::{
    Ability, Condition, Cost, Expr, Information, ObjectRef, Operation, Property,
};
use crate::model::visit::{VisitorMut, WalkMut};
use serde_json::Value;

pub(crate) fn lower(value: &mut Value) -> Result<(), String> {
    if let Some(native) = transformed(value, Form::Bound)? {
        *value = native;
    }
    Ok(())
}

/// Older declarations spell their numeric casting conditions with Source.SpellX.
/// Keep that bounded inverse view beside the preferred native operands. Reparse
/// still normalizes it to the requested AnnouncedX, with the current thresholds.
pub(crate) fn projection(value: &Value) -> Result<Option<Value>, String> {
    transformed(value, Form::ConditionSpelling)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Bound,
    ConditionSpelling,
}

fn transformed(value: &Value, form: Form) -> Result<Option<Value>, String> {
    if !matches!(
        value["kind"].as_str(),
        Some(
            "Spell"
                | "Scoped"
                | "WithPresentation"
                | "CostBranch"
                | "CostOptions"
                | "Conditional"
                | "Gift"
        )
    ) {
        return Ok(None);
    }
    crate::functions::branch_bounds(value)?;
    if !contains(value, form) {
        return Ok(None);
    }
    let mut ability: Ability = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    if serde_json::to_value(&ability).map_err(|e| e.to_string())? != *value {
        return Err("spell X normalization requires complete native fields".into());
    }
    if normalize(&mut ability, form) {
        return serde_json::to_value(ability)
            .map(Some)
            .map_err(|e| e.to_string());
    }
    Ok(None)
}

fn contains(value: &Value, form: Form) -> bool {
    let found = match form {
        Form::Bound => value["kind"] == "ObjectProperty" && value["args"]["property"] == "SpellX",
        Form::ConditionSpelling => {
            value["kind"] == "Compare" && value["args"]["left"]["kind"] == "AnnouncedX"
        }
    };
    if found {
        return true;
    }
    match value {
        Value::Object(fields) => fields.values().any(|v| contains(v, form)),
        Value::Array(items) => items.iter().any(|v| contains(v, form)),
        _ => false,
    }
}

fn normalize(ability: &mut Ability, form: Form) -> bool {
    match ability {
        Ability::Spell { announcement, body } => {
            let mut own = OwnSpell {
                changed: false,
                form,
            };
            announcement.walk_mut(&mut own);
            body.walk_mut(&mut own);
            own.changed
        }
        Ability::Scoped { ability, .. } | Ability::WithPresentation { ability, .. } => {
            normalize(ability, form)
        }
        Ability::CostBranch { paid, unpaid, .. } => normalize(paid, form) | normalize(unpaid, form),
        Ability::CostOptions { options, .. } => {
            options.iter_mut().fold(false, |changed, (_, ability)| {
                normalize(ability, form) | changed
            })
        }
        Ability::Conditional {
            then, otherwise, ..
        } => then
            .iter_mut()
            .chain(otherwise)
            .fold(false, |changed, ability| normalize(ability, form) | changed),
        Ability::Gift {
            promised,
            unpromised,
            ..
        } => promised
            .iter_mut()
            .chain(unpromised)
            .fold(false, |changed, ability| normalize(ability, form) | changed),
        _ => false,
    }
}

struct OwnSpell {
    changed: bool,
    form: Form,
}

impl VisitorMut for OwnSpell {
    fn ability(&mut self, _: &mut Ability) -> bool {
        false
    }
    fn cost(&mut self, _: &mut Cost) -> bool {
        false
    }
    fn operation(&mut self, operation: &mut Operation) -> bool {
        !matches!(
            operation,
            Operation::CreateTrigger { .. }
                | Operation::CreateDelayedTrigger { .. }
                | Operation::ReflexiveTrigger { .. }
                | Operation::CreateReplacement { .. }
        )
    }
    fn expr(&mut self, expression: &mut Expr) -> bool {
        if self.form == Form::Bound
            && matches!(
                expression,
                Expr::ObjectProperty {
                    object: ObjectRef::Source,
                    property: Property::SpellX,
                    information: Information::CurrentOrLastKnown
                }
            )
        {
            *expression = Expr::AnnouncedX;
            self.changed = true;
            return false;
        }
        true
    }
    fn condition(&mut self, condition: &mut Condition) -> bool {
        if self.form == Form::ConditionSpelling
            && let Condition::Compare { left, .. } = condition
            && matches!(left, Expr::AnnouncedX)
        {
            *left = Expr::ObjectProperty {
                object: ObjectRef::Source,
                property: Property::SpellX,
                information: Information::CurrentOrLastKnown,
            };
            self.changed = true;
        }
        true
    }
}
