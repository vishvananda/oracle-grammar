//! Printing by a card's own choices. A set of alternatives cannot say that two uses of one
//! rule on a card want different ones, so an ordered key names every choice of one
//! derivation. Replaying it takes no search, and the words still come from the value: an
//! edited operand prints as edited, and a key that no longer fits the value prints nothing.
//! Only the spelling of the card's own name comes from the key, since the value has none.
use crate::grammar::{Class, Grammar, RuleId, Sym};
use crate::pattern::Bindings;
use crate::render::{class_token, word_token};
use crate::tokens::Token;
use serde_json::Value;

/// Reads the steps of [`crate::render::Matcher::derivation`], which close each rule after
/// its children, from the last: a rule first, then its symbols from right to left.
pub struct Replay<'a> {
    grammar: &'a Grammar,
    trail: &'a [(RuleId, usize)],
    /// How many steps are left to read.
    left: usize,
    /// How each mention of the card's name is spelled, read from the last.
    spellings: &'a [u8],
    /// Tokens from last to first.
    out: Vec<Token>,
}

impl<'a> Replay<'a> {
    /// The tokens this trail prints for `value` from `rule`, if it fits.
    pub fn run(
        grammar: &'a Grammar,
        rule: RuleId,
        value: &Value,
        trail: &'a [(RuleId, usize)],
        spellings: &'a [u8],
    ) -> Option<Vec<Token>> {
        let mut replay = Self {
            grammar,
            trail,
            left: trail.len(),
            spellings,
            out: Vec::new(),
        };
        if !replay.node(rule, value) || replay.left != 0 {
            return None;
        }
        replay.out.reverse();
        Some(replay.out)
    }

    fn node(&mut self, rule: RuleId, value: &Value) -> bool {
        let Some(at) = self.left.checked_sub(1) else {
            return false;
        };
        let (named, index) = self.trail[at];
        if named != rule {
            return false;
        }
        self.left = at;
        let grammar = self.grammar;
        let definition = &grammar.rules[rule as usize];
        let Some(alternative) = definition.alternatives.get(index) else {
            return false;
        };
        let Some(pattern) = &definition.sem else {
            return match alternative.syms.first() {
                Some(Sym::Rule(child)) => self.node(*child, value),
                Some(sym) => self.terminal(sym, Some(value)),
                None => false,
            };
        };
        // A pattern with a splice can bind more than one way; the trail decides which.
        let (left, written, spellings) = (self.left, self.out.len(), self.spellings);
        for bindings in pattern.unify(value, Bindings::new()) {
            if self.symbols(rule, index, &bindings) {
                return true;
            }
            self.left = left;
            self.out.truncate(written);
            self.spellings = spellings;
        }
        false
    }

    fn symbols(&mut self, rule: RuleId, index: usize, bindings: &Bindings<'_, '_>) -> bool {
        let grammar = self.grammar;
        let alternative = &grammar.rules[rule as usize].alternatives[index];
        for (sym, hole) in alternative.syms.iter().zip(&alternative.holes).rev() {
            let value = hole.as_ref().and_then(|name| bindings.get(name));
            let done = match sym {
                Sym::Rule(child) => self.node(*child, value.unwrap_or(&Value::Null)),
                other => self.terminal(other, value),
            };
            if !done {
                return false;
            }
        }
        true
    }

    fn terminal(&mut self, sym: &Sym, value: Option<&Value>) -> bool {
        let token = self.token(sym, value);
        let found = token.is_some();
        self.out.extend(token);
        found
    }

    fn token(&mut self, sym: &Sym, value: Option<&Value>) -> Option<Token> {
        match sym {
            Sym::Word(word) => Some(word_token(word)),
            Sym::Mark(mark) => Some(Token::Mark(*mark)),
            Sym::Class(Class::Newline) => Some(Token::Newline),
            Sym::Class(Class::SelfName) => {
                let (last, rest) = self.spellings.split_last()?;
                self.spellings = rest;
                Some(Token::SelfName(*last))
            }
            Sym::Class(_) => value.and_then(|value| class_token(sym, value)),
            Sym::Rule(_) => None,
        }
    }
}
