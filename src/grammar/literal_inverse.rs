//! Closed literal productions also print their checked native normal form.
//! No text or independently declared meaning is retained here: the original
//! pattern still fills every derivation in the forward direction.
use super::{Class, Rule, RuleId, Sym};
use crate::engine::{canonical_abilities, validate_abilities};
use crate::model::ir_schema::Ty;
use crate::pattern::{Bindings, Item, Pattern};
use rustc_hash::FxHashMap;
use serde_json::Value;

const NODES: usize = 8192;
const DEPTH: usize = 64;

pub(super) fn prepare(rules: &[Rule]) -> FxHashMap<RuleId, Pattern> {
    (0..)
        .zip(rules)
        .filter_map(|(id, rule)| native(rule).map(|value| (id, Pattern::Value(value))))
        .collect()
}

fn native(rule: &Rule) -> Option<Value> {
    if !matches!(&rule.ty, Some(Ty::Named(name)) if name == "Ability")
        || rule.alternatives.is_empty()
        || !rule.alternatives.iter().all(|alternative| {
            alternative.syms.iter().all(|sym| {
                matches!(
                    sym,
                    Sym::Word(_) | Sym::Mark(_) | Sym::Class(Class::SelfName | Class::Newline)
                )
            })
        })
    {
        return None;
    }
    let pattern = rule.sem.as_ref()?;
    let mut left = NODES;
    if !closed(pattern, 0, &mut left) {
        return None;
    }
    let value = pattern.fill(&Bindings::new(), 0).ok()?;
    let original = Value::Array(vec![value]);
    let mut abilities = original.clone();
    // Do not repair invalid literal operands or unresolved contextual symbols.
    validate_abilities(&abilities).ok()?;
    crate::bind::bind_ability(&mut abilities[0], "").ok()?;
    validate_abilities(&abilities).ok()?;
    let canonical = canonical_abilities(&abilities).ok()?;
    (canonical != original).then(|| canonical[0].clone())
}

fn closed(pattern: &Pattern, depth: usize, left: &mut usize) -> bool {
    if depth > DEPTH || *left == 0 {
        return false;
    }
    *left -= 1;
    match pattern {
        Pattern::Value(_) => true,
        Pattern::Object(fields) => fields
            .iter()
            .all(|(_, value)| closed(value, depth + 1, left)),
        Pattern::List(items) => items.iter().all(|item| match item {
            Item::One(value) => closed(value, depth + 1, left),
            Item::Splice(..) => false,
        }),
        Pattern::Hole(..) | Pattern::Symbol(..) => false,
    }
}
