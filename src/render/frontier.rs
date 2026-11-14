//! Where an inverse derivation of an exact text stopped: the print
//! counterpart of the parse frontier.
use super::Matcher;
use crate::grammar::RuleId;

/// At most this many failures are kept at the furthest position.
const BLAMED: usize = 256;

/// What failed at the furthest target position any partial derivation reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Blame {
    /// No inverse of the rule's meaning unifies with the value it was given.
    Unify,
    /// The alternative's symbol did not continue the target text.
    Symbol { alternative: usize, symbol: usize },
}

#[derive(Debug, Clone, Default)]
pub struct Frontier {
    /// The furthest target token any partial derivation reached.
    pub at: usize,
    /// The rules that failed there, in the order they failed.
    pub blamed: Vec<(RuleId, Blame)>,
}

impl Matcher<'_> {
    pub(super) fn reached(&mut self, at: usize) {
        if at > self.frontier.at {
            self.frontier.at = at;
            self.frontier.blamed.clear();
        }
    }

    pub(super) fn blame(&mut self, at: usize, rule: RuleId, blame: Blame) {
        self.reached(at);
        let frontier = &mut self.frontier;
        if at == frontier.at
            && frontier.blamed.len() < BLAMED
            && !frontier.blamed.contains(&(rule, blame))
        {
            frontier.blamed.push((rule, blame));
        }
    }

    /// The furthest any search on this target got so far.
    pub fn frontier(&self) -> &Frontier {
        &self.frontier
    }
}
