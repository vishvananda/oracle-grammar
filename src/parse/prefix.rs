//! Replay a deterministic prefix before evaluating the rest of its alternative.
use super::Parser;
use crate::grammar::{PrefixSpan, RuleId};

#[derive(Default)]
pub(super) struct PrefixProgress<'g> {
    path: &'g [u32],
    matched: usize,
}

impl<'g> Parser<'g> {
    pub(super) fn prefix_spans(&self, rule: RuleId) -> &'g [PrefixSpan] {
        if self.audit || self.diagnostics || self.off.is_some() {
            return &[];
        }
        self.grammar.terminal_prefixes.rule(rule)
    }

    pub(super) fn prefix(
        &mut self,
        spans: &[PrefixSpan],
        alternative: u32,
        at: usize,
        previous: &mut PrefixProgress<'g>,
    ) -> Result<Option<usize>, String> {
        if spans.is_empty() {
            return Ok(Some(0));
        }
        let trie = &self.grammar.terminal_prefixes;
        let path = trie.path(spans, alternative);
        if path.is_empty() {
            return Ok(Some(0));
        }

        // A cached comparison is still charged as an attempted terminal. Stop
        // at the same point as ordinary evaluation when the budget runs out.
        let remaining = self.limits.steps.saturating_sub(self.counters.steps);
        let budget = remaining.min(path.len() as u64) as usize;
        let mut matched = 0;
        while matched < budget {
            let node = path[matched];
            if previous.path.get(matched) == Some(&node) {
                if matched == previous.matched {
                    break;
                }
            } else if !trie.matches(node, self.looks[at + matched]) {
                break;
            }
            matched += 1;
        }
        let failed = matched < budget;
        let charged = matched + usize::from(failed);
        self.counters.steps += charged as u64;
        if charged > 0 {
            self.max_read = self.max_read.max(at + charged - 1);
        }
        if matched > 0 {
            self.furthest = self.furthest.max(at + matched);
            self.sub_furthest = self.sub_furthest.max(at + matched);
        }
        if !failed && budget < path.len() {
            self.counters.steps += 1;
            return Err("step limit".into());
        }
        *previous = PrefixProgress { path, matched };
        Ok((!failed).then_some(matched))
    }
}
