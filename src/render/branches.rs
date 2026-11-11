//! Keep ability and condition producers visible through typed program wrappers.
//! Reserving only a wrapper's first text can hide every other production below
//! it, even when a wider beam could hold a representative of each one.
use crate::derivation::{Derivation, Node};
use crate::grammar::{Grammar, RuleId};
use crate::model::ir_schema::Ty;
use crate::pattern::Pattern;
use smallvec::SmallVec;

type Roots = SmallVec<[RuleId; 4]>;

#[derive(Hash, PartialEq, Eq)]
pub(super) enum Branch {
    Alternative(usize),
    Semantic(Roots),
}

pub(super) fn of(
    grammar: &Grammar,
    rule: RuleId,
    alternative: usize,
    derivation: &Derivation,
) -> (Branch, u64) {
    let supported = |ty: &Ty| matches!(ty, Ty::Named(name) if matches!(name.as_str(), "Ability" | "Condition" | "Instruction" | "Program"));
    // Pass-through rules do not own a type or a node. Their derivation carries
    // the semantic production below the aliases, including its checked type.
    let ty = grammar.rules[rule as usize].ty.as_ref().or_else(|| {
        let Node::Rule { rule, .. } = derivation.node.as_deref()? else {
            return None;
        };
        grammar.rules[*rule as usize].ty.as_ref()
    });
    let legacy_ability = |ty: &Ty| matches!(ty, Ty::Named(name) if name == "Ability");
    let legacy = match ty {
        Some(Ty::Vec(inner)) => legacy_ability(inner),
        Some(ty) => legacy_ability(ty),
        _ => false,
    };
    let complete = match ty {
        Some(Ty::Vec(inner)) => supported(inner),
        Some(ty) => supported(ty),
        _ => false,
    };
    let mut roots = Roots::new();
    let mut visited = 0;
    if complete && let Some(node) = derivation.node.as_deref() {
        let mut original = Roots::new();
        if legacy && !semantic_roots(grammar, node, &mut original, &mut visited, false) {
            original.clear();
        }
        if !semantic_roots(grammar, node, &mut roots, &mut visited, true) {
            roots = original;
        }
    }
    let branch = if roots.is_empty() {
        Branch::Alternative(alternative)
    } else {
        Branch::Semantic(roots)
    };
    (branch, visited)
}

// This key affects only representative selection inside the existing beam.
// On overflow retain the old stop-at-Ability signature, not a partial condition key.
fn semantic_roots(
    grammar: &Grammar,
    node: &Node,
    roots: &mut Roots,
    visited: &mut u64,
    conditions: bool,
) -> bool {
    let mut stack = vec![(node, false)];
    while let Some((node, inside_ability)) = stack.pop() {
        if *visited >= 8192 {
            return false;
        }
        *visited += 1;
        let Node::Rule { rule, kids, .. } = node else {
            continue;
        };
        let definition = &grammar.rules[*rule as usize];
        let named = match &definition.ty {
            Some(Ty::Named(name)) => Some(name.as_str()),
            _ => None,
        };
        let owns = definition.sem.is_some() && !matches!(&definition.sem, Some(Pattern::Hole(..)));
        let ability = named == Some("Ability") && owns;
        if owns && (conditions && named == Some("Condition") || ability && !inside_ability) {
            if roots.len() >= 128 {
                return false;
            }
            roots.push(*rule);
        }
        if ability && !conditions {
            continue;
        }
        // Continue through the ability producer: its nested condition producer
        // is the distinction that must survive Program/If/list beam cuts.
        if stack.len().saturating_add(kids.len()) > 1024 {
            return false;
        }
        for kid in kids.iter().rev() {
            stack.push((&kid.node, inside_ability || ability));
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derivation::{Kid, Rope};
    use crate::grammar::Rule;
    use crate::render::Renderer;
    use crate::style::Model;
    use crate::tokens::Token;
    use serde_json::json;
    use std::rc::Rc;

    fn grammar() -> Grammar {
        let mut g = Grammar::default();
        for (name, ty) in [
            ("ability", "Ability"),
            ("program", "Program"),
            ("instruction", "Instruction"),
            ("condition_alias", "Condition"),
            ("generic", "Condition"),
            ("explicit", "Condition"),
        ] {
            g.rules.push(Rule {
                name: name.into(),
                alternatives: vec![],
                sem: if name.ends_with("alias") {
                    None
                } else {
                    Some(Pattern::Value(json!(null)))
                },
                ty: Some(Ty::Named(ty.into())),
                prefer: vec![],
            });
        }
        g
    }
    fn wrapped(root: RuleId, leaf: RuleId, text: &str, preferred: u32) -> Derivation {
        let mut node = Rc::new(Node::Rule {
            rule: leaf,
            alternative: 0,
            kids: vec![],
        });
        for rule in [3, 2, 1, 0].into_iter().filter(|rule| *rule >= root) {
            node = Rc::new(Node::Rule {
                rule,
                alternative: 0,
                kids: vec![Kid {
                    sym: 0,
                    offset: 0,
                    node,
                }],
            });
        }
        Derivation {
            tokens: Rope::leaf(Token::Word(text.into())),
            node: Some(node),
            preferred,
            ..Derivation::default()
        }
    }
    #[test]
    fn condition_producer_survives_alias_instruction_program_and_ability_beams() {
        let g = grammar();
        let model = Model::default();
        for rule in [0, 1, 2, 3] {
            let mut renderer = Renderer::new(&g, &model, 4, vec![]);
            let mut found: Vec<_> = (0..5)
                .map(|i| wrapped(rule, 4, &format!("generic{i}"), 10))
                .collect();
            found.push(wrapped(rule, 5, "explicit", 0));
            let kept = renderer.best_of_each(rule, found, &[0]);
            assert_eq!(kept.len(), 4);
            assert!(
                kept.iter()
                    .any(|d| d.tokens.items() == vec![Token::Word("explicit".into())])
            );
            assert!(renderer.work.branch_nodes > 0);
        }
    }
    #[test]
    fn conditions_do_not_replace_the_owning_ability_key() {
        let g = grammar();
        let d = wrapped(0, 5, "explicit", 0);
        let (branch, _) = of(&g, 0, 0, &d);
        let Branch::Semantic(roots) = branch else {
            panic!("semantic roots")
        };
        assert_eq!(roots.as_slice(), &[0, 5]);
    }
    #[test]
    fn oversized_body_keeps_original_outer_ability_signature() {
        let g = grammar();
        let leaf = Rc::new(Node::Rule {
            rule: 5,
            alternative: 0,
            kids: vec![],
        });
        let node = Rc::new(Node::Rule {
            rule: 0,
            alternative: 0,
            kids: (0..1025)
                .map(|_| Kid {
                    sym: 0,
                    offset: 0,
                    node: Rc::clone(&leaf),
                })
                .collect(),
        });
        let d = Derivation {
            node: Some(node),
            ..Derivation::default()
        };
        let (branch, visited) = of(&g, 0, 0, &d);
        let Branch::Semantic(roots) = branch else {
            panic!("old ability signature")
        };
        assert_eq!(roots.as_slice(), &[0]);
        assert!(visited <= 8192);
    }
    #[test]
    fn actual_full_phoenix_native_prints_and_reparses_without_spelling_key() {
        use crate::engine::{Engine, canonical_abilities};
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/additional-graveyard-trigger-native.json"
        ))
        .unwrap();
        let c = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["name"]
                    .as_str()
                    .is_some_and(|name| name.contains("Phoenix"))
            })
            .unwrap();
        let engine = Engine::load().unwrap();
        let expected = canonical_abilities(&c["normal_expected_readings"][0]).unwrap();
        let name = c["name"].as_str().unwrap();
        let got = engine.parse(name, c["text"].as_str().unwrap()).unwrap();
        assert!(got.readings.contains(&expected));
        let types: Vec<String> = serde_json::from_value(c["types"].clone()).unwrap();
        let printed = engine
            .print_keyed(name, &expected, &types, &[])
            .unwrap()
            .expect("native Phoenix default");
        assert!(
            engine
                .parse(name, &printed)
                .unwrap()
                .readings
                .contains(&expected)
        );
    }
}
