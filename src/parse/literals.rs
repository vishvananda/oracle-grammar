//! Evaluate a compiled terminal-only rule in grammar order.
use super::{Parser, Rc, Reading, Value};
use crate::grammar::{Class, Rule, Sym};
use crate::tokens::Token;

impl Parser<'_> {
    pub(super) fn literal_rule(
        &mut self,
        rule: &Rule,
        candidates: &[u32],
        at: usize,
        whole: bool,
        value: &Value,
    ) -> Result<Vec<Reading>, String> {
        let mut readings: Vec<Reading> = Vec::new();
        let mut constant = None;
        for index in candidates {
            let mut pos = at;
            for sym in &rule.alternatives[*index as usize].syms {
                self.counters.steps += 1;
                if self.counters.steps > self.limits.steps {
                    return Err("step limit".into());
                }
                self.max_read = self.max_read.max(pos);
                let matched = if *sym == Sym::Class(Class::SelfName) {
                    matches!(self.tokens.get(pos), Some(Token::SelfName(_)))
                } else {
                    self.terminal(sym, pos).is_some()
                };
                if !matched {
                    break;
                }
                pos += 1;
                self.furthest = self.furthest.max(pos);
                self.sub_furthest = self.sub_furthest.max(pos);
            }
            if pos - at == rule.alternatives[*index as usize].syms.len()
                && (!whole || pos == self.tokens.len())
                && !readings.iter().any(|reading| reading.end == pos)
            {
                let value = constant.get_or_insert_with(|| Rc::new(value.clone()));
                readings.push(Reading::new(pos, Rc::clone(value)));
                if readings.len() > self.limits.readings {
                    return Err(format!("reading limit at {} token {at}", rule.name));
                }
            }
        }
        Ok(readings)
    }
}
