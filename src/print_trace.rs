//! What printing one card consulted, so that a later grammar can tell whether the card
//! still prints the same without printing it again.
//!
//! Printing reads the grammar in a few places: the rules a value reaches through the
//! discrimination tree, the rules it asks to print a value, and the definitions of those
//! rules. A trace keeps the rules it asked or reached, and the leading steps of every value
//! it looked up. A rule whose pattern could match none of those prefixes, and which is
//! not among those rules, cannot change what this card prints.
use crate::grammar::RuleId;
use crate::pattern::{Item, Pattern};
use crate::tokens::Token;
use rustc_hash::{FxHashSet as HashSet, FxHasher};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::hash::Hasher;

/// The most leading steps of a value or pattern that a trace keeps.
pub const PREFIX_STEPS: usize = 8;

/// The longest path from a looked-up value's root that a feature names.
pub const FEATURE_DEPTH: usize = 12;

/// The path step to any element of a list.
pub const ELEMENT: &str = "*";

/// What one card's printing consulted.
#[derive(Debug, Default, Clone)]
pub struct Trace {
    /// Rules asked to print a value, reached by one, or read while ranking.
    pub rules: HashSet<RuleId>,
    /// Hashes of every value's first one to [`PREFIX_STEPS`] steps.
    pub prefixes: HashSet<u32>,
    /// Every node of every looked-up value within [`FEATURE_DEPTH`] of its root, as
    /// [`feature_key`] spells it.
    pub features: HashSet<u32>,
    /// Distinct values looked up.
    pub values: u64,
    /// Hashes of every token of every text read, as [`token_key`] spells them.
    pub words: HashSet<u32>,
    /// Hashes of every two adjacent tokens of every text read, as [`pair_key`] joins them.
    pub pairs: HashSet<u32>,
    /// Hashes of the words the tokenizer looked up in the vocabulary.
    pub vocabulary: HashSet<u32>,
    /// The most steps one parse took.
    pub steps: u64,
    /// The deepest rule call of any parse.
    pub depth: usize,
}

impl Trace {
    fn absorb(&mut self, other: &Self) {
        self.rules.extend(&other.rules);
        self.prefixes.extend(&other.prefixes);
        self.features.extend(&other.features);
        self.values += other.values;
        self.words.extend(&other.words);
        self.pairs.extend(&other.pairs);
        self.vocabulary.extend(&other.vocabulary);
        self.steps = self.steps.max(other.steps);
        self.depth = self.depth.max(other.depth);
    }
}

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static TRACE: RefCell<Trace> = RefCell::new(Trace::default());
}

/// Run `work` and return what its printing consulted on this thread.
pub fn record<T>(work: impl FnOnce() -> T) -> (T, Trace) {
    let outer = ACTIVE.with(|active| active.replace(true));
    let saved = TRACE.with(|trace| trace.replace(Trace::default()));
    let out = work();
    let trace = TRACE.with(|trace| trace.replace(saved));
    ACTIVE.with(|active| active.set(outer));
    if outer {
        TRACE.with(|current| current.borrow_mut().absorb(&trace));
    }
    (out, trace)
}

fn active() -> bool {
    ACTIVE.with(Cell::get)
}

/// Printing read this rule.
pub(crate) fn rule(rule: RuleId) {
    if active() {
        TRACE.with(|trace| trace.borrow_mut().rules.insert(rule));
    }
}

/// A value was looked up and reached these rules.
pub(crate) fn lookup<'a>(value: &Value, reached: impl IntoIterator<Item = &'a RuleId>) {
    if !active() {
        return;
    }
    let prefixes = value_prefixes(value);
    TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        trace.values += 1;
        trace.rules.extend(reached);
        trace.prefixes.extend(prefixes);
        value_features(value, &mut Vec::new(), &mut trace.features);
    });
}

/// A text was read as these tokens.
pub(crate) fn tokens(tokens: &[Token]) {
    if active() {
        TRACE.with(|trace| {
            let mut trace = trace.borrow_mut();
            trace.words.extend(tokens.iter().map(token_key));
            trace.pairs.extend(
                tokens
                    .windows(2)
                    .map(|two| pair_key(token_key(&two[0]), token_key(&two[1]))),
            );
        });
    }
}

/// The tokenizer asked whether the grammar has this word.
pub(crate) fn vocabulary_query(word: &str) {
    if active() {
        TRACE.with(|trace| trace.borrow_mut().vocabulary.insert(word_key(word)));
    }
}

/// One parse finished after this much work.
pub(crate) fn parse_work(steps: u64, depth: usize) {
    if active() {
        TRACE.with(|trace| {
            let mut trace = trace.borrow_mut();
            trace.steps = trace.steps.max(steps);
            trace.depth = trace.depth.max(depth);
        });
    }
}

/// The hash a word or mark has wherever a trace or a required set holds it.
pub fn word_key(word: &str) -> u32 {
    let full = hash_text(word);
    (full ^ (full >> 32)) as u32
}

/// Two adjacent tokens, or two adjacent terminals of one alternative.
pub fn pair_key(first: u32, second: u32) -> u32 {
    let mut hasher = FxHasher::default();
    hasher.write_u32(first);
    hasher.write_u32(second);
    let full = hasher.finish();
    (full ^ (full >> 32)) as u32
}

/// The key a grammar literal requires of a token, if only one spelling matches it: a
/// literal that reads as a number in another spelling also matches that number.
pub fn literal_key(literal: &str) -> Option<u32> {
    match literal.parse::<u32>() {
        Ok(number) if number.to_string() != literal => None,
        _ => Some(word_key(literal)),
    }
}

/// A token as the word a grammar terminal must spell to match it. A self name and a
/// line break have keys no terminal spells.
pub fn token_key(token: &Token) -> u32 {
    match token {
        Token::Word(word) => word_key(word),
        Token::Number(number) | Token::Grouped(number) => word_key(&number.to_string()),
        Token::Mark(mark) => word_key(mark.encode_utf8(&mut [0; 4])),
        Token::SelfName(_) => word_key("\u{0}self"),
        Token::Newline => word_key("\u{0}newline"),
    }
}

/// A node a value holds at a path from its root: its kind (`O`bject, `L`ist, `S`tring,
/// `N`umber, or `X` for null and booleans) and a scalar's spelling.
pub fn feature_key<'a>(path: impl IntoIterator<Item = &'a str>, kind: u8, text: &str) -> u32 {
    let mut hasher = FxHasher::default();
    for step in path {
        hasher.write(step.as_bytes());
        hasher.write_u8(0xff);
    }
    hasher.write_u8(0xfe);
    hasher.write_u8(kind);
    hasher.write(text.as_bytes());
    let full = hasher.finish();
    (full ^ (full >> 32)) as u32
}

/// The kind and spelling [`feature_key`] gives a node.
pub fn node_tag(value: &Value) -> (u8, std::borrow::Cow<'_, str>) {
    match value {
        Value::Object(_) => (b'O', "".into()),
        Value::Array(_) => (b'L', "".into()),
        Value::String(text) => (b'S', text.as_str().into()),
        Value::Number(number) => (b'N', number.to_string().into()),
        Value::Null => (b'X', "0".into()),
        Value::Bool(false) => (b'X', "1".into()),
        Value::Bool(true) => (b'X', "2".into()),
    }
}

/// Every node of `value` within [`FEATURE_DEPTH`] steps of its root, `path` being the
/// steps already taken.
fn value_features<'a>(value: &'a Value, path: &mut Vec<&'a str>, out: &mut HashSet<u32>) {
    let (kind, text) = node_tag(value);
    out.insert(feature_key(path.iter().copied(), kind, &text));
    if path.len() >= FEATURE_DEPTH {
        return;
    }
    match value {
        Value::Object(object) => {
            for (name, inner) in object {
                path.push(name);
                value_features(inner, path, out);
                path.pop();
            }
        }
        Value::Array(items) => {
            for item in items {
                path.push(ELEMENT);
                value_features(item, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

/// One step of a value's preorder path, as the discrimination tree takes it, spelled
/// without the tree's interned numbers so that two grammars agree on it.
enum Step<'a> {
    Exact(u8, &'a str, u64),
    Hole,
}

fn value_steps<'a>(value: &'a Value, out: &mut Vec<Step<'a>>) {
    if out.len() >= PREFIX_STEPS {
        return;
    }
    match value {
        Value::String(text) => out.push(Step::Exact(b'S', text, 0)),
        Value::Number(number) => {
            let text = number.to_string();
            out.push(Step::Exact(b'N', "", hash_text(&text)));
        }
        Value::Array(items) => {
            out.push(Step::Exact(b'L', "", items.len() as u64));
            for item in items {
                value_steps(item, out);
            }
        }
        Value::Object(object) => {
            out.push(Step::Exact(b'O', "", object.len() as u64));
            let mut fields: Vec<(&String, &Value)> = object.iter().collect();
            fields.sort_unstable_by_key(|(name, _)| *name);
            for (name, item) in fields {
                if out.len() >= PREFIX_STEPS {
                    return;
                }
                out.push(Step::Exact(b'K', name, 0));
                value_steps(item, out);
            }
        }
        Value::Null => out.push(Step::Exact(b'X', "", 0)),
        Value::Bool(false) => out.push(Step::Exact(b'X', "", 1)),
        Value::Bool(true) => out.push(Step::Exact(b'X', "", 2)),
    }
}

fn pattern_steps<'a>(pattern: &'a Pattern, out: &mut Vec<Step<'a>>) {
    if out.len() >= PREFIX_STEPS {
        return;
    }
    match pattern {
        Pattern::Value(value) => value_steps(value, out),
        Pattern::Hole(..) | Pattern::Symbol(..) => out.push(Step::Hole),
        Pattern::Object(fields) => {
            out.push(Step::Exact(b'O', "", fields.len() as u64));
            let mut fields: Vec<&(String, Pattern)> = fields.iter().collect();
            fields.sort_unstable_by_key(|(name, _)| name);
            for (name, inner) in fields {
                if out.len() >= PREFIX_STEPS {
                    return;
                }
                out.push(Step::Exact(b'K', name, 0));
                pattern_steps(inner, out);
            }
        }
        Pattern::List(pieces) => {
            if !pieces.iter().all(|piece| matches!(piece, Item::One(_))) {
                out.push(Step::Hole);
                return;
            }
            out.push(Step::Exact(b'L', "", pieces.len() as u64));
            for piece in pieces {
                if let Item::One(inner) = piece {
                    pattern_steps(inner, out);
                }
            }
        }
    }
}

fn hash_text(text: &str) -> u64 {
    let mut hasher = FxHasher::default();
    hasher.write(text.as_bytes());
    hasher.finish()
}

/// Hashes of each leading run of exact steps, longest last.
fn prefix_hashes(steps: &[Step]) -> Vec<u32> {
    let mut hasher = FxHasher::default();
    let mut out = Vec::new();
    for step in steps.iter().take(PREFIX_STEPS) {
        let Step::Exact(tag, text, number) = step else {
            break;
        };
        hasher.write_u8(*tag);
        hasher.write(text.as_bytes());
        hasher.write_u8(0xff);
        hasher.write_u64(*number);
        let full = hasher.finish();
        out.push((full ^ (full >> 32)) as u32);
    }
    out
}

/// The hashes of a value's leading steps, one per prefix length.
pub fn value_prefixes(value: &Value) -> Vec<u32> {
    let mut steps = Vec::new();
    value_steps(value, &mut steps);
    prefix_hashes(&steps)
}

/// The hash of the exact steps a pattern needs before its first hole, capped at
/// [`PREFIX_STEPS`]. `None` when it starts with a hole and so fits every value.
pub fn pattern_prefix(pattern: &Pattern) -> Option<u32> {
    let mut steps = Vec::new();
    pattern_steps(pattern, &mut steps);
    prefix_hashes(&steps).last().copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_pattern_prefix_is_one_of_the_prefixes_of_every_value_it_can_fit() {
        let value = json!({"DealDamage": {"amount": 3, "target": {"kind": "Any"}}});
        let fitting = Pattern::Object(vec![(
            "DealDamage".into(),
            Pattern::Object(vec![
                (
                    "amount".into(),
                    Pattern::Hole("a".into(), crate::model::ir_schema::Ty::Opaque),
                ),
                (
                    "target".into(),
                    Pattern::Hole("t".into(), crate::model::ir_schema::Ty::Opaque),
                ),
            ]),
        )]);
        let other = Pattern::Object(vec![(
            "GainLife".into(),
            Pattern::Hole("x".into(), crate::model::ir_schema::Ty::Opaque),
        )]);
        let prefixes = value_prefixes(&value);
        assert!(prefixes.contains(&pattern_prefix(&fitting).unwrap()));
        assert!(!prefixes.contains(&pattern_prefix(&other).unwrap()));
        assert_eq!(
            pattern_prefix(&Pattern::Hole(
                "x".into(),
                crate::model::ir_schema::Ty::Opaque
            )),
            None
        );
    }

    #[test]
    fn recording_is_scoped_to_the_work_and_merges_into_an_outer_trace() {
        let ((), outer) = record(|| {
            rule(1);
            let ((), inner) = record(|| rule(2));
            assert_eq!(inner.rules.len(), 1);
        });
        assert_eq!(outer.rules.len(), 2);
        rule(3);
        let ((), empty) = record(|| {});
        assert!(empty.rules.is_empty());
    }
}
