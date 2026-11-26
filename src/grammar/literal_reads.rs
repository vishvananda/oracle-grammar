//! Constant terminal productions can reuse their elaborated meaning.
use super::{Class, Rule, RuleId, Sym, literal_inverse::closed};
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

const DIRECT_VALUE: u32 = 1;
const FIRST_COMPILED: u32 = 2;
// A large hot-loaded grammar can always use ordinary evaluation.
const COMPILE_NODES: usize = 131_072;
const PATTERN_NODES: usize = 128;
const PATTERN_DEPTH: usize = 24;

#[derive(Debug, Default)]
pub(crate) struct LiteralReads {
    entries: Vec<u32>,
    values: Vec<Value>,
}

impl LiteralReads {
    pub(super) fn build(rules: &[Rule]) -> Self {
        let mut index = Self::default();
        let mut left = COMPILE_NODES;
        for rule in rules {
            let entry = index.compile(rule, &mut left).unwrap_or_default();
            index.entries.push(entry);
        }
        index
    }

    fn compile(&mut self, rule: &Rule, left: &mut usize) -> Option<u32> {
        let pattern = rule.sem.as_ref()?;
        if rule.alternatives.is_empty()
            || rule.alternatives.iter().any(|alternative| {
                alternative.holes.iter().any(Option::is_some)
                    || alternative.syms.iter().any(|sym| {
                        !matches!(
                            sym,
                            Sym::Word(_)
                                | Sym::Mark(_)
                                | Sym::Class(Class::SelfName | Class::Newline)
                        )
                    })
            })
        {
            return None;
        }
        if matches!(pattern, Pattern::Value(_)) {
            return Some(DIRECT_VALUE);
        }
        let budget = (*left).min(PATTERN_NODES);
        let mut remaining = budget;
        let constant = closed(pattern, 0, PATTERN_DEPTH, &mut remaining);
        *left -= budget - remaining;
        if !constant {
            return None;
        }
        let entry = u32::try_from(self.values.len())
            .ok()?
            .checked_add(FIRST_COMPILED)?;
        self.values.push(pattern.fill(&Bindings::new(), 0).ok()?);
        Some(entry)
    }

    pub(crate) fn value<'a>(&'a self, id: RuleId, rule: &'a Rule) -> Option<&'a Value> {
        match *self.entries.get(id as usize)? {
            0 => None,
            DIRECT_VALUE => match rule.sem.as_ref()? {
                Pattern::Value(value) => Some(value),
                _ => None,
            },
            index => self.values.get((index - FIRST_COMPILED) as usize),
        }
    }
}
