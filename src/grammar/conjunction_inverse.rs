//! Declared conjunctions also print their canonical form. An operand of a
//! literal `AllOf` list can be `Any` or a conjunction itself, which the
//! canonical IR folds away, and the canonical order need not be the declared
//! one. Seeded lists were copied from one card's IR, so other cards' operands
//! meet this most often. When the declared pattern does not unify, printing
//! deals the folded conjuncts back out among the list's members, as `@and`
//! does. The forward direction is unchanged, and reparse still proves every
//! printed text.
use super::{Rule, RuleId};
use crate::pattern::{Item, Pattern};
use rustc_hash::FxHashMap;

/// Lists longer than this keep only their declared order.
const MEMBERS: usize = 4;

/// Rules with a literal inverse already print their native normal form.
pub(super) fn prepare(
    rules: &[Rule],
    literal: &FxHashMap<RuleId, Pattern>,
) -> FxHashMap<RuleId, Pattern> {
    (0..)
        .zip(rules)
        .filter(|(id, _)| !literal.contains_key(id))
        .filter_map(|(id, rule)| {
            let mut changed = false;
            let relaxed = relax(rule.sem.as_ref()?, &mut changed);
            changed.then_some((id, relaxed))
        })
        .collect()
}

fn members(fields: &[(String, Pattern)]) -> Option<&[Item]> {
    match fields {
        [(kind, Pattern::Value(name)), (args, Pattern::List(items))]
            if kind == "kind"
                && name == "AllOf"
                && args == "args"
                && (2..=MEMBERS).contains(&items.len())
                && items.iter().all(|item| matches!(item, Item::One(_))) =>
        {
            Some(items)
        }
        _ => None,
    }
}

fn relax(pattern: &Pattern, changed: &mut bool) -> Pattern {
    match pattern {
        Pattern::Object(fields) => {
            if let Some(items) = members(fields) {
                *changed = true;
                let args = items
                    .iter()
                    .filter_map(|item| match item {
                        Item::One(inner) => Some(relax(inner, changed)),
                        Item::Splice(..) => None,
                    })
                    .collect();
                return Pattern::Symbol("and".into(), args);
            }
            Pattern::Object(
                fields
                    .iter()
                    .map(|(name, inner)| (name.clone(), relax(inner, changed)))
                    .collect(),
            )
        }
        Pattern::List(items) => Pattern::List(
            items
                .iter()
                .map(|item| match item {
                    Item::One(inner) => Item::One(relax(inner, changed)),
                    Item::Splice(..) => item.clone(),
                })
                .collect(),
        ),
        Pattern::Symbol(name, args) => Pattern::Symbol(
            name.clone(),
            args.iter().map(|arg| relax(arg, changed)).collect(),
        ),
        Pattern::Value(_) | Pattern::Hole(..) => pattern.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ir_schema::Ty;
    use crate::pattern::Bindings;
    use serde_json::json;

    fn all_of(items: Vec<Pattern>) -> Pattern {
        Pattern::Object(vec![
            ("kind".into(), Pattern::Value(json!("AllOf"))),
            (
                "args".into(),
                Pattern::List(items.into_iter().map(Item::One).collect()),
            ),
        ])
    }

    #[test]
    fn a_folded_seeded_conjunction_deals_back_out() {
        let noun = Pattern::Hole("noun".into(), Ty::Named("ObjectFilter".into()));
        let controlled = Pattern::Value(json!({"kind":"ControlledBy","args":{"kind":"You"}}));
        let mut changed = false;
        let relaxed = relax(&all_of(vec![noun, controlled]), &mut changed);
        assert!(changed);
        // "permanent you control": the noun is `Any` and the IR keeps one filter.
        let folded = json!({"kind":"ControlledBy","args":{"kind":"You"}});
        let found = relaxed.unify(&folded, Bindings::new());
        assert!(
            found
                .iter()
                .any(|b| b.get("noun") == Some(&json!({"kind":"Any"})))
        );
        // A canonical order other than the copied one still deals out.
        let reordered = json!({"kind":"AllOf","args":[
            {"kind":"ControlledBy","args":{"kind":"You"}},
            {"kind":"CardType","args":"Creature"}]});
        let found = relaxed.unify(&reordered, Bindings::new());
        assert!(
            found
                .iter()
                .any(|b| b.get("noun") == Some(&json!({"kind":"CardType","args":"Creature"})))
        );
    }
}
