//! Presentation belongs to the same ability; it never introduces a rules scope.
use serde_json::Value;

pub(super) fn core(mut ability: &Value) -> &Value {
    while ability["kind"] == "WithPresentation" {
        ability = &ability["args"]["ability"];
    }
    ability
}

pub(super) fn core_mut(mut ability: &mut Value) -> &mut Value {
    while ability["kind"] == "WithPresentation" {
        ability = &mut ability["args"]["ability"];
    }
    ability
}

pub(super) fn forms(ability: &Value) -> Option<Vec<Value>> {
    if ability["kind"] != "WithPresentation" {
        return None;
    }
    Some(
        super::unbind_ability(&ability["args"]["ability"])
            .into_iter()
            .map(|inner| {
                let mut form = ability.clone();
                form["args"]["ability"] = inner;
                form
            })
            .collect(),
    )
}
