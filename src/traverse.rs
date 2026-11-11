//! Structural edits over already bounded semantic values. Callers explicitly
//! choose which scope edges they cross; replacement values are never revisited.
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Boundary {
    Transparent,
    Filter,
    Target,
    Holder,
    Lexical,
    Reflexive,
    Branch,
    Conditional,
    Ability,
    Mode,
}

pub(crate) fn boundary(value: &Value) -> Boundary {
    match value.get("$sym").and_then(Value::as_str) {
        Some("in_filter") => return Boundary::Filter,
        Some("target") => return Boundary::Target,
        Some("holder") => return Boundary::Holder,
        Some("scope") => return Boundary::Lexical,
        Some("reflexive") => return Boundary::Reflexive,
        Some("kicker_branch") => return Boundary::Branch,
        Some("conditional_ability") => return Boundary::Conditional,
        _ => {}
    }
    if matches!(
        value.get("kind").and_then(Value::as_str),
        Some("Spell" | "Activated" | "Triggered" | "Replacement" | "Static")
    ) {
        Boundary::Ability
    } else if value.get("weight").is_some() && value.get("announcement").is_some() {
        Boundary::Mode
    } else {
        Boundary::Transparent
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Visit {
    Children,
    Skip,
}

pub(crate) fn children(value: &mut Value, mut visit: impl FnMut(&mut Value)) {
    match value {
        Value::Object(fields) => fields.values_mut().for_each(&mut visit),
        Value::Array(items) => items.iter_mut().for_each(visit),
        _ => {}
    }
}

pub(crate) fn rewrite(value: &mut Value, visit: &mut impl FnMut(&mut Value) -> Visit) {
    if matches!(visit(value), Visit::Children) {
        children(value, |child| rewrite(child, visit));
    }
}

/// Full structural replacement, deliberately including nested scopes. Captured
/// expressions need this; lexical substitutions must choose their own boundaries.
pub(crate) fn replace(value: &mut Value, from: &Value, to: &Value) {
    rewrite(value, &mut |node| {
        if node == from {
            *node = to.clone();
            Visit::Skip
        } else {
            Visit::Children
        }
    });
}
