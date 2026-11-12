//! Bounded, memoized all-readings recognition over tokens. A rule at a position
//! yields every distinct (end, value) pair, so derivations with one meaning
//! collapse and distinct meanings survive to be reported, never guessed between.
use crate::grammar::{Class, Grammar, RuleId, Sym};
use crate::pattern::Bindings;
use crate::starts::Look;
use crate::tokens::Token;
use rustc_hash::FxHashMap as HashMap;
use serde_json::Value;
use std::borrow::Cow;
use std::rc::Rc;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub tokens: usize,
    pub steps: u64,
    /// Unbound values per grammar rule, before contextual validation/deduplication.
    pub readings: usize,
    pub depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            tokens: 2048,
            steps: 2_000_000,
            readings: 1024,
            depth: 256,
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Counters {
    pub steps: u64,
    pub rule_calls: u64,
    pub memo_hits: u64,
    pub first_prunes: u64,
}

/// One way a rule matches from a start: where it ends and what it means.
#[derive(Debug, Clone)]
pub struct Reading {
    pub end: usize,
    pub value: Rc<Value>,
}

type Readings = Rc<Vec<Reading>>;

pub struct Parser<'a> {
    grammar: &'a Grammar,
    tokens: &'a [Token],
    /// Each token as the alternative tables read it, then the end of the text.
    looks: Vec<Look>,
    // A whole-input request can discard prefixes; a span request cannot.
    memo: HashMap<(RuleId, usize, bool), Readings>,
    /// What a rule that cannot start reads, shared so a refusal allocates nothing.
    nothing: Readings,
    limits: Limits,
    pub counters: Counters,
    /// The furthest token any terminal reached, for refusal messages.
    pub furthest: usize,
    diagnostics: bool,
    pub expected: Vec<String>,
    pub expected_truncated: bool,
    audit: bool,
    /// Rules that read nothing, to measure what a card depends on.
    off: Option<&'a RuleSet>,
}

/// A set of rules to switch off for one parse.
pub type RuleSet = rustc_hash::FxHashSet<RuleId>;

/// The field that carries a mined rule's name through an audit parse.
pub const AUDIT_TAG: &str = "$mined";

fn tagged(value: &Value, rule: &str) -> Value {
    let mut copy = value.clone();
    let holder = match &mut copy {
        Value::Array(items) => items.first_mut(),
        other => Some(other),
    };
    if let Some(Value::Object(object)) = holder {
        let tags = object
            .entry(AUDIT_TAG)
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(names) = tags {
            names.push(Value::from(rule));
        }
    }
    copy
}

/// Remove the audit tags from a value and return the rules they named.
pub fn take_tags(value: &mut Value, out: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            if let Some(Value::Array(names)) = object.remove(AUDIT_TAG) {
                out.extend(names.iter().filter_map(|n| n.as_str().map(str::to_string)));
            }
            object.values_mut().for_each(|v| take_tags(v, out));
        }
        Value::Array(items) => items.iter_mut().for_each(|v| take_tags(v, out)),
        _ => {}
    }
}

/// A partial match of one alternative.
#[derive(Clone)]
struct Partial {
    at: usize,
    bindings: Vec<(usize, Rc<Value>)>,
}

impl<'a> Parser<'a> {
    pub fn new(grammar: &'a Grammar, tokens: &'a [Token], limits: Limits) -> Self {
        Self {
            grammar,
            tokens,
            looks: (0..=tokens.len())
                .map(|at| grammar.starts.look(tokens.get(at)))
                .collect(),
            memo: HashMap::default(),
            nothing: Rc::new(Vec::new()),
            limits,
            counters: Counters::default(),
            furthest: 0,
            diagnostics: false,
            expected: Vec::new(),
            expected_truncated: false,
            audit: false,
            off: None,
        }
    }

    /// Read as if these rules were not in the grammar.
    pub fn without(mut self, off: &'a RuleSet) -> Self {
        self.off = Some(off);
        self
    }

    /// Tag each value with the mined rules that made it, for the rule audit.
    pub fn with_audit(mut self) -> Self {
        self.audit = true;
        self
    }

    /// Failure expectations are collected only for an explicit diagnostic request.
    pub fn with_diagnostics(mut self) -> Self {
        self.diagnostics = true;
        self
    }

    fn missed(&mut self, at: usize, expected: String) {
        if !self.diagnostics || at < self.furthest || self.expected.contains(&expected) {
            return;
        }
        if self.expected.len() < 32 {
            self.expected.push(expected);
        } else {
            self.expected_truncated = true;
        }
    }

    /// Every reading of this rule that starts at this token.
    pub fn readings_at(&mut self, rule: RuleId, at: usize) -> Result<Vec<Reading>, String> {
        Ok(self.rule(rule, at, 0, false)?.to_vec())
    }

    /// Every reading of this rule found anywhere in the text: its span and value.
    pub fn spans(&self, rule: RuleId) -> Vec<(usize, usize, Rc<Value>)> {
        let mut out = Vec::new();
        for ((found, start, whole), readings) in &self.memo {
            if *found == rule {
                // The unrestricted request contains every whole-input value.
                if *whole && self.memo.contains_key(&(rule, *start, false)) {
                    continue;
                }
                out.extend(
                    readings
                        .iter()
                        .map(|r| (*start, r.end, Rc::clone(&r.value))),
                );
            }
        }
        out
    }

    /// Every distinct value that consumes all the tokens from `rule`.
    pub fn whole(&mut self, rule: RuleId) -> Result<Vec<Rc<Value>>, String> {
        if self.tokens.len() > self.limits.tokens {
            return Err("token limit".into());
        }
        // Ordinary requests share the unrestricted memo. Only a reading-budget
        // overflow needs an EOF-constrained retry; retain its consumed work.
        let readings = match self.rule(rule, 0, 0, false) {
            Err(why) if why.starts_with("reading limit at ") => self.rule(rule, 0, 0, true)?,
            found => found?,
        };
        Ok(readings
            .iter()
            .filter(|r| r.end == self.tokens.len())
            .map(|r| Rc::clone(&r.value))
            .collect())
    }

    fn rule(
        &mut self,
        rule: RuleId,
        at: usize,
        depth: usize,
        whole: bool,
    ) -> Result<Readings, String> {
        if let Some(found) = self.memo.get(&(rule, at, whole)) {
            self.counters.memo_hits += 1;
            return Ok(Rc::clone(found));
        }
        if whole && let Some(found) = self.memo.get(&(rule, at, false)) {
            self.counters.memo_hits += 1;
            let readings = Rc::new(
                found
                    .iter()
                    .filter(|reading| reading.end == self.tokens.len())
                    .cloned()
                    .collect(),
            );
            self.memo.insert((rule, at, true), Rc::clone(&readings));
            return Ok(readings);
        }
        if depth > self.limits.depth {
            return Err("depth limit".into());
        }
        if self.off.is_some_and(|off| off.contains(&rule)) {
            return Ok(Rc::clone(&self.nothing));
        }
        let grammar = self.grammar;
        let candidates = grammar.starts.candidates(rule, self.looks[at]);
        if candidates.is_empty() {
            self.counters.first_prunes += 1;
            if self.diagnostics {
                self.missed(at, format!("rule:{}", grammar.rules[rule as usize].name));
            }
            return Ok(Rc::clone(&self.nothing));
        }
        self.counters.rule_calls += 1;
        let definition = &grammar.rules[rule as usize];
        let mut readings: Vec<Reading> = Vec::new();
        for index in candidates {
            let alternative = &definition.alternatives[*index as usize];
            let mut partials = vec![Partial {
                at,
                bindings: Vec::new(),
            }];
            for (index, sym) in alternative.syms.iter().enumerate() {
                let final_child = whole && index + 1 == alternative.syms.len();
                partials = self.advance(partials, index, sym, depth, final_child)?;
                if partials.is_empty() {
                    break;
                }
            }
            for partial in partials {
                if whole && partial.at != self.tokens.len() {
                    continue;
                }
                let value = match &definition.sem {
                    Some(pattern) => {
                        let mut bindings = Bindings::new();
                        for (index, value) in &partial.bindings {
                            if let Some(hole) = &alternative.holes[*index] {
                                bindings.insert(hole, Cow::Borrowed(&**value));
                            }
                        }
                        // A semantic constraint refuses this derivation. Other
                        // readings still count; only search limits abort them.
                        let Ok(value) = pattern.fill(&bindings, at) else {
                            continue;
                        };
                        Rc::new(value)
                    }
                    None => partial
                        .bindings
                        .first()
                        .map_or_else(|| Rc::new(Value::Null), |(_, v)| Rc::clone(v)),
                };
                let value = if self.audit
                    && (definition.name.starts_with("m_") || definition.name.starts_with("t_"))
                {
                    Rc::new(tagged(&value, &definition.name))
                } else {
                    value
                };
                if !readings
                    .iter()
                    .any(|r| r.end == partial.at && r.value == value)
                {
                    readings.push(Reading {
                        end: partial.at,
                        value,
                    });
                }
            }
            if readings.len() > self.limits.readings {
                return Err(format!("reading limit at {} token {at}", definition.name));
            }
        }
        let readings = Rc::new(readings);
        self.memo.insert((rule, at, whole), Rc::clone(&readings));
        Ok(readings)
    }

    fn advance(
        &mut self,
        partials: Vec<Partial>,
        index: usize,
        sym: &Sym,
        depth: usize,
        whole: bool,
    ) -> Result<Vec<Partial>, String> {
        let mut next = Vec::new();
        for partial in partials {
            self.counters.steps += 1;
            if self.counters.steps > self.limits.steps {
                return Err("step limit".into());
            }
            if let Sym::Rule(rule) = sym {
                let readings = self.rule(*rule, partial.at, depth + 1, whole)?;
                for reading in readings.iter() {
                    let mut extended = partial.clone();
                    extended.at = reading.end;
                    extended.bindings.push((index, Rc::clone(&reading.value)));
                    next.push(extended);
                }
            } else if let Some(value) = self.terminal(sym, partial.at) {
                let mut extended = partial;
                extended.at += 1;
                if extended.at > self.furthest {
                    self.expected.clear();
                    self.expected_truncated = false;
                }
                self.furthest = self.furthest.max(extended.at);
                if let Some(value) = value {
                    extended.bindings.push((index, Rc::new(value)));
                }
                next.push(extended);
            } else if self.diagnostics {
                self.missed(partial.at, format!("{sym:?}"));
            }
        }
        Ok(next)
    }

    /// `Some(None)` is a plain match; `Some(Some(v))` also carries a value.
    fn terminal(&self, sym: &Sym, at: usize) -> Option<Option<Value>> {
        let token = self.tokens.get(at)?;
        match (sym, token) {
            (Sym::Word(expected), Token::Word(word)) => (expected == word).then_some(None),
            (Sym::Word(expected), Token::Number(value) | Token::Grouped(value)) => {
                (expected.parse::<u32>().ok()? == *value).then_some(None)
            }
            (Sym::Mark(expected), Token::Mark(mark)) => (expected == mark).then_some(None),
            (Sym::Class(Class::Number), Token::Number(value) | Token::Grouped(value)) => {
                Some(Some(Value::from(*value)))
            }
            (Sym::Class(Class::Word), Token::Word(word)) => Some(Some(Value::from(word.clone()))),
            (Sym::Class(Class::Capitalized), Token::Word(word)) => word
                .starts_with(char::is_uppercase)
                .then(|| Some(Value::from(word.clone()))),
            (Sym::Class(Class::SelfName), Token::SelfName(_)) => Some(Some(Value::from("$self"))),
            (Sym::Class(Class::Newline), Token::Newline) => Some(None),
            _ => None,
        }
    }
}
