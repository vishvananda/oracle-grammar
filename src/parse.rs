//! Bounded, memoized all-readings recognition over tokens. A rule at a position
//! yields every distinct (end, value) pair, so derivations with one meaning
//! collapse and distinct meanings survive to be reported, never guessed between.
use crate::grammar::{Class, Grammar, RuleId, Sym};
use crate::pattern::Bindings;
use crate::starts::Look;
use crate::tokens::Token;
use rustc_hash::FxHashMap as HashMap;
use serde_json::Value;
use smallvec::{SmallVec, smallvec};
use std::borrow::Cow;
use std::rc::Rc;

mod literals;
mod memo;
mod prefix;
pub use memo::ParseMemo;

#[derive(Clone)]
struct LocalMemo {
    readings: Readings,
    max_read: usize,
    min_dep_seq: u32,
    rel_depth: u16,
    furthest: usize,
}

#[inline]
fn memo_key(rule: RuleId, at: usize, whole: bool) -> u64 {
    (u64::from(rule) << 32) | ((at as u64) << 1) | u64::from(whole)
}

#[inline]
fn unpack_memo_key(key: u64) -> (RuleId, usize, bool) {
    (
        (key >> 32) as RuleId,
        ((key >> 1) & 0x7fff_ffff) as usize,
        (key & 1) != 0,
    )
}

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
    /// Cached non-zero `untagged_hash(&value) | 1`; `0` until needed for dedup.
    hash: std::cell::Cell<u64>,
}

impl Reading {
    #[inline]
    fn new(end: usize, value: Rc<Value>) -> Self {
        Self {
            end,
            value,
            hash: std::cell::Cell::new(0),
        }
    }

    #[inline]
    fn get_hash(&self) -> u64 {
        let h = self.hash.get();
        if h != 0 {
            return h;
        }
        let computed = untagged_hash(&self.value) | 1;
        self.hash.set(computed);
        computed
    }
}

type Readings = Rc<Vec<Reading>>;

pub struct Parser<'a> {
    grammar: &'a Grammar,
    tokens: &'a [Token],
    /// Each token as the alternative tables read it, then the end of the text.
    looks: Vec<Look>,
    // Packed `(rule, at, whole)` key: whole-input requests can discard prefixes.
    memo: HashMap<u64, LocalMemo>,
    /// Optional within-face cross-candidate parse memo.
    shared: Option<ParseMemo>,
    text_id: u32,
    stored_shared: bool,
    max_read: usize,
    entry_seq: u32,
    min_dep_seq: u32,
    sub_deepest: usize,
    sub_furthest: usize,
    /// What a rule that cannot start reads, shared so a refusal allocates nothing.
    nothing: Readings,
    limits: Limits,
    pub counters: Counters,
    /// The furthest token any terminal reached, for refusal messages.
    pub furthest: usize,
    diagnostics: bool,
    pub expected: Vec<String>,
    pub expected_truncated: bool,
    /// The deepest rule call so far, which a print trace keeps as headroom evidence.
    deepest: usize,
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

/// Equal once audit tags are ignored.
fn same_untagged(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            let width = |map: &serde_json::Map<String, Value>| {
                map.len() - usize::from(map.contains_key(AUDIT_TAG))
            };
            width(left) == width(right)
                && left
                    .iter()
                    .filter(|(key, _)| *key != AUDIT_TAG)
                    .all(|(key, value)| {
                        right
                            .get(key)
                            .is_some_and(|other| same_untagged(value, other))
                    })
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(value, other)| same_untagged(value, other))
        }
        _ => left == right,
    }
}

/// A hash that agrees with `same_untagged`, and so with `==` on untagged values. Map
/// entries combine in any order, as map equality ignores order, and audit tags are
/// skipped. Readings compare hashes first, so one rule's many readings do not cost a
/// deep comparison for every pair.
fn untagged_hash(value: &Value) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = rustc_hash::FxHasher::default();
    match value {
        Value::Null => 0u8.hash(&mut hasher),
        Value::Bool(flag) => (1u8, flag).hash(&mut hasher),
        Value::Number(number) => (2u8, number).hash(&mut hasher),
        Value::String(text) => (3u8, text).hash(&mut hasher),
        Value::Array(items) => {
            (4u8, items.len()).hash(&mut hasher);
            items
                .iter()
                .for_each(|item| untagged_hash(item).hash(&mut hasher));
        }
        Value::Object(object) => {
            let entries = object.iter().filter(|(key, _)| *key != AUDIT_TAG).fold(
                0u64,
                |sum, (key, value)| {
                    let mut entry = rustc_hash::FxHasher::default();
                    (key, untagged_hash(value)).hash(&mut entry);
                    sum.wrapping_add(entry.finish())
                },
            );
            (5u8, entries).hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// How many seeded-rule tags a value carries.
fn tag_count(value: &Value) -> usize {
    match value {
        Value::Object(object) => object
            .iter()
            .map(|(key, value)| match (key == AUDIT_TAG, value) {
                (true, Value::Array(names)) => names.len(),
                (true, _) => 0,
                (false, value) => tag_count(value),
            })
            .sum(),
        Value::Array(items) => items.iter().map(tag_count).sum(),
        _ => 0,
    }
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

impl Drop for Parser<'_> {
    fn drop(&mut self) {
        crate::print_trace::parse_work(self.counters.steps, self.deepest);
    }
}

/// A partial match of one alternative.
#[derive(Clone)]
struct Partial {
    at: usize,
    bindings: SmallVec<[(usize, Rc<Value>); 2]>,
}

fn push_reading(readings: &mut Vec<Reading>, candidate: Reading, audit: bool) {
    let end = candidate.end;
    if !readings.iter().any(|r| r.end == end) {
        readings.push(candidate);
        return;
    }
    let hash = candidate.get_hash();
    for reading in readings.iter_mut() {
        if reading.end != end {
            continue;
        }
        let same = reading.get_hash() == hash
            && if audit {
                same_untagged(&reading.value, &candidate.value)
            } else {
                reading.value == candidate.value
            };
        if same {
            if audit && tag_count(&candidate.value) < tag_count(&reading.value) {
                reading.value = candidate.value;
                reading.hash.set(hash);
            }
            return;
        }
    }
    readings.push(candidate);
}

impl<'a> Parser<'a> {
    pub fn new(grammar: &'a Grammar, tokens: &'a [Token], limits: Limits) -> Self {
        Self {
            grammar,
            tokens,
            looks: (0..=tokens.len())
                .map(|at| grammar.starts.look(tokens.get(at)))
                .collect(),
            memo: HashMap::with_capacity_and_hasher(
                (tokens.len() * 16).min(16_384),
                Default::default(),
            ),
            shared: None,
            text_id: 0,
            stored_shared: false,
            max_read: 0,
            entry_seq: 1,
            min_dep_seq: u32::MAX,
            sub_deepest: 0,
            sub_furthest: 0,
            nothing: Rc::new(Vec::new()),
            limits,
            counters: Counters::default(),
            furthest: 0,
            diagnostics: false,
            expected: Vec::new(),
            expected_truncated: false,
            deepest: 0,
            audit: false,
            off: None,
        }
    }

    /// Read as if these rules were not in the grammar.
    pub fn without(mut self, off: &'a RuleSet) -> Self {
        self.off = Some(off);
        self
    }

    /// Tag each value with the mined rules that made it, for the rule audit. Each distinct
    /// value keeps one derivation: the one with the fewest tags.
    pub fn with_audit(mut self) -> Self {
        self.audit = true;
        self
    }

    /// Failure expectations are collected only for an explicit diagnostic request.
    pub fn with_diagnostics(mut self) -> Self {
        self.diagnostics = true;
        self
    }

    pub(crate) fn with_memo(mut self, memo: ParseMemo) -> Self {
        self.text_id = u32::try_from(memo.texts.len()).unwrap_or(u32::MAX);
        self.shared = Some(memo);
        self
    }

    pub(crate) fn has_memo(&self) -> bool {
        self.shared.is_some()
    }

    pub(crate) fn take_memo(&mut self) -> ParseMemo {
        let mut memo = self.shared.take().unwrap_or_default();
        if self.stored_shared {
            memo.texts.push(self.tokens.to_vec());
            self.stored_shared = false;
        }
        memo
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
        for (&packed, entry) in &self.memo {
            let (found, start, whole) = unpack_memo_key(packed);
            if found == rule {
                // The unrestricted request contains every whole-input value.
                if whole && self.memo.contains_key(&memo_key(rule, start, false)) {
                    continue;
                }
                out.extend(
                    entry
                        .readings
                        .iter()
                        .map(|r| (start, r.end, Rc::clone(&r.value))),
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

    fn lookup_shared(&mut self, rule: RuleId, at: usize, depth: usize) -> Option<Readings> {
        let step_cap = self.limits.steps / 4;
        if self.counters.steps >= step_cap || self.off.is_some() || self.audit || self.diagnostics {
            return None;
        }
        let shared = self.shared.as_ref()?;
        let budget_steps = step_cap.saturating_sub(self.counters.steps);
        let budget_depth = self.limits.depth.saturating_sub(depth);
        let entry = shared.lookup(rule, at, self.tokens, budget_steps, budget_depth)?;
        if entry.text_id >= self.text_id {
            return None;
        }
        let readings = Rc::clone(&entry.readings);
        let max_read = if entry.read_eof {
            self.tokens.len()
        } else {
            (entry.end_ex as usize).saturating_sub(1)
        };
        let steps = entry.steps;
        let rel_depth = entry.rel_depth;
        let furthest = entry.furthest as usize;
        let my_seq = self.entry_seq;
        self.entry_seq = self.entry_seq.wrapping_add(1).max(1);
        self.counters.steps += steps;
        self.counters.memo_hits += 1;
        self.max_read = self.max_read.max(max_read);
        self.min_dep_seq = self.min_dep_seq.min(my_seq);
        self.sub_deepest = self.sub_deepest.max(depth + usize::from(rel_depth));
        self.deepest = self.deepest.max(depth + usize::from(rel_depth));
        self.sub_furthest = self.sub_furthest.max(furthest);
        self.furthest = self.furthest.max(furthest);
        self.memo.insert(
            memo_key(rule, at, false),
            LocalMemo {
                readings: Rc::clone(&readings),
                max_read,
                min_dep_seq: my_seq,
                rel_depth,
                furthest,
            },
        );
        Some(readings)
    }

    fn finish_rule(
        &mut self,
        (rule, at, depth, whole): (RuleId, usize, usize, bool),
        (outer_max_read, outer_min_dep, outer_deepest, outer_furthest, start_seq, start_steps): (
            usize,
            u32,
            usize,
            usize,
            u32,
            u64,
        ),
        readings: Readings,
    ) -> Readings {
        let my_max_read = self.max_read;
        let my_seq = self.entry_seq;
        self.entry_seq = self.entry_seq.wrapping_add(1).max(1);
        let delta_steps = self.counters.steps - start_steps;
        // When every memo dependency was created after this call began, `delta_steps`
        // is the exact cold step count of this subtree with zero DAG overcounting.
        let self_contained = self.min_dep_seq >= start_seq;
        let stored_min_dep = self.min_dep_seq.min(my_seq);
        let my_rel_depth = self
            .sub_deepest
            .saturating_sub(depth)
            .min(u16::MAX as usize) as u16;
        let my_furthest = self.sub_furthest;
        self.max_read = outer_max_read.max(my_max_read);
        self.min_dep_seq = outer_min_dep.min(stored_min_dep);
        self.sub_deepest = outer_deepest.max(self.sub_deepest);
        self.sub_furthest = outer_furthest.max(my_furthest);
        self.memo.insert(
            memo_key(rule, at, whole),
            LocalMemo {
                readings: Rc::clone(&readings),
                max_read: my_max_read,
                min_dep_seq: stored_min_dep,
                rel_depth: my_rel_depth,
                furthest: my_furthest,
            },
        );
        let end_ex = (my_max_read + 1).min(self.tokens.len());
        if self_contained
            && !whole
            && delta_steps >= 16
            && delta_steps <= self.limits.steps / 8
            && end_ex.saturating_sub(at) >= 4
            && self.off.is_none()
            && !self.audit
            && !self.diagnostics
            && let Some(shared) = self.shared.as_mut()
        {
            let read_eof = my_max_read >= self.tokens.len();
            let inserted = shared.insert(
                rule,
                at,
                self.tokens,
                memo::CachedRule {
                    text_id: self.text_id,
                    end_ex: end_ex as u32,
                    read_eof,
                    readings: Rc::clone(&readings),
                    steps: delta_steps,
                    rel_depth: my_rel_depth,
                    furthest: my_furthest as u32,
                },
            );
            self.stored_shared |= inserted;
        }
        readings
    }

    fn rule(
        &mut self,
        rule: RuleId,
        at: usize,
        depth: usize,
        whole: bool,
    ) -> Result<Readings, String> {
        let key = memo_key(rule, at, whole);
        if let Some(found) = self.memo.get(&key) {
            self.counters.memo_hits += 1;
            self.max_read = self.max_read.max(found.max_read);
            self.min_dep_seq = self.min_dep_seq.min(found.min_dep_seq);
            self.sub_deepest = self.sub_deepest.max(depth + usize::from(found.rel_depth));
            self.deepest = self.deepest.max(depth + usize::from(found.rel_depth));
            self.sub_furthest = self.sub_furthest.max(found.furthest);
            self.furthest = self.furthest.max(found.furthest);
            return Ok(Rc::clone(&found.readings));
        }
        if whole && let Some(found) = self.memo.get(&memo_key(rule, at, false)) {
            self.counters.memo_hits += 1;
            let max_read = found.max_read;
            let min_dep_seq = found.min_dep_seq;
            let rel_depth = found.rel_depth;
            let furthest = found.furthest;
            let readings = Rc::new(
                found
                    .readings
                    .iter()
                    .filter(|reading| reading.end == self.tokens.len())
                    .cloned()
                    .collect(),
            );
            self.max_read = self.max_read.max(max_read);
            self.min_dep_seq = self.min_dep_seq.min(min_dep_seq);
            self.sub_deepest = self.sub_deepest.max(depth + usize::from(rel_depth));
            self.deepest = self.deepest.max(depth + usize::from(rel_depth));
            self.sub_furthest = self.sub_furthest.max(furthest);
            self.furthest = self.furthest.max(furthest);
            self.memo.insert(
                key,
                LocalMemo {
                    readings: Rc::clone(&readings),
                    max_read,
                    min_dep_seq,
                    rel_depth,
                    furthest,
                },
            );
            return Ok(readings);
        }
        if depth > self.limits.depth {
            return Err("depth limit".into());
        }
        self.deepest = self.deepest.max(depth);
        if self.off.is_some_and(|off| off.contains(&rule)) {
            return Ok(Rc::clone(&self.nothing));
        }
        self.max_read = self.max_read.max(at);
        let grammar = self.grammar;
        let candidates = grammar.starts.candidates(rule, self.looks[at]);
        if candidates.is_empty() {
            self.counters.first_prunes += 1;
            if self.diagnostics {
                self.missed(at, format!("rule:{}", grammar.rules[rule as usize].name));
            }
            return Ok(Rc::clone(&self.nothing));
        }
        if !whole && let Some(readings) = self.lookup_shared(rule, at, depth) {
            return Ok(readings);
        }
        self.counters.rule_calls += 1;
        let saved = (
            self.max_read,
            self.min_dep_seq,
            self.sub_deepest,
            self.sub_furthest,
            self.entry_seq,
            self.counters.steps,
        );
        self.max_read = at;
        self.min_dep_seq = u32::MAX;
        self.sub_deepest = depth;
        self.sub_furthest = 0;
        let definition = &grammar.rules[rule as usize];
        if !self.audit
            && !self.diagnostics
            && let Some(value) = grammar.literal_reads.value(rule, definition)
        {
            let readings = self.literal_rule(rule, definition, candidates, at, whole, value)?;
            return Ok(self.finish_rule((rule, at, depth, whole), saved, Rc::new(readings)));
        }
        // Direct zero-copy fast path for a single-candidate pass-through rule.
        if definition.sem.is_none()
            && !self.audit
            && let [only] = candidates
            && let [Sym::Rule(child)] = definition.alternatives[*only as usize].syms.as_slice()
        {
            self.counters.steps += 1;
            if self.counters.steps > self.limits.steps {
                return Err("step limit".into());
            }
            let child_readings = self.rule(*child, at, depth + 1, whole)?;
            return Ok(self.finish_rule((rule, at, depth, whole), saved, child_readings));
        }
        let readings = self.eval_alternatives(rule, candidates, (at, depth, whole))?;
        Ok(self.finish_rule((rule, at, depth, whole), saved, Rc::new(readings)))
    }

    fn eval_alternatives(
        &mut self,
        rule: RuleId,
        candidates: &[u32],
        (at, depth, whole): (usize, usize, bool),
    ) -> Result<Vec<Reading>, String> {
        let definition = &self.grammar.rules[rule as usize];
        let prefixes = self.prefix_spans(rule);
        let mut previous = prefix::PrefixProgress::default();
        let mut readings: Vec<Reading> = Vec::new();
        let tag_seeded = self.audit && crate::grammar::seeded(&definition.name);
        for index in candidates {
            let alternative = &definition.alternatives[*index as usize];
            if definition.sem.is_none()
                && let [Sym::Rule(child)] = alternative.syms.as_slice()
            {
                self.counters.steps += 1;
                if self.counters.steps > self.limits.steps {
                    return Err("step limit".into());
                }
                let child_readings = self.rule(*child, at, depth + 1, whole)?;
                for reading in child_readings.iter() {
                    if whole && reading.end != self.tokens.len() {
                        continue;
                    }
                    let candidate = if tag_seeded {
                        Reading::new(
                            reading.end,
                            Rc::new(tagged(&reading.value, &definition.name)),
                        )
                    } else {
                        reading.clone()
                    };
                    push_reading(&mut readings, candidate, self.audit);
                }
                if readings.len() > self.limits.readings {
                    return Err(format!("reading limit at {} token {at}", definition.name));
                }
                continue;
            }
            let Some(consumed) = self.prefix(prefixes, *index, at, &mut previous)? else {
                continue;
            };
            let mut partials: SmallVec<[Partial; 2]> = smallvec![Partial {
                at: at + consumed,
                bindings: SmallVec::new(),
            }];
            for (sym_idx, sym) in alternative.syms.iter().enumerate().skip(consumed) {
                let final_child = whole && sym_idx + 1 == alternative.syms.len();
                let need_binding = definition.sem.is_none() || alternative.holes[sym_idx].is_some();
                partials =
                    self.advance(partials, sym_idx, sym, need_binding, (depth, final_child))?;
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
                        for (sym_idx, value) in &partial.bindings {
                            if let Some(hole) = &alternative.holes[*sym_idx] {
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
                let value = if tag_seeded {
                    Rc::new(tagged(&value, &definition.name))
                } else {
                    value
                };
                push_reading(&mut readings, Reading::new(partial.at, value), self.audit);
            }
            if readings.len() > self.limits.readings {
                return Err(format!("reading limit at {} token {at}", definition.name));
            }
        }
        Ok(readings)
    }

    fn advance(
        &mut self,
        mut partials: SmallVec<[Partial; 2]>,
        index: usize,
        sym: &Sym,
        need_binding: bool,
        (depth, whole): (usize, bool),
    ) -> Result<SmallVec<[Partial; 2]>, String> {
        if partials.len() == 1 {
            self.counters.steps += 1;
            if self.counters.steps > self.limits.steps {
                return Err("step limit".into());
            }
            if let Sym::Rule(rule) = sym {
                let readings = self.rule(*rule, partials[0].at, depth + 1, whole)?;
                return match readings.as_slice() {
                    [] => {
                        partials.clear();
                        Ok(partials)
                    }
                    [reading] => {
                        partials[0].at = reading.end;
                        if need_binding {
                            partials[0]
                                .bindings
                                .push((index, Rc::clone(&reading.value)));
                        }
                        Ok(partials)
                    }
                    slice => {
                        let mut base = partials.pop();
                        let mut next = SmallVec::with_capacity(slice.len());
                        for (i, reading) in slice.iter().enumerate() {
                            let mut extended = if i + 1 == slice.len() {
                                base.take().expect("last partial")
                            } else {
                                base.as_ref().expect("partial").clone()
                            };
                            extended.at = reading.end;
                            if need_binding {
                                extended.bindings.push((index, Rc::clone(&reading.value)));
                            }
                            next.push(extended);
                        }
                        Ok(next)
                    }
                };
            }
            let pos = partials[0].at;
            self.max_read = self.max_read.max(pos);
            if let Some(value) = self.terminal(sym, pos) {
                partials[0].at += 1;
                if partials[0].at > self.furthest {
                    self.expected.clear();
                    self.expected_truncated = false;
                }
                self.furthest = self.furthest.max(partials[0].at);
                self.sub_furthest = self.sub_furthest.max(partials[0].at);
                if need_binding && let Some(value) = value {
                    partials[0].bindings.push((index, Rc::new(value)));
                }
                return Ok(partials);
            }
            if self.diagnostics {
                self.missed(pos, format!("{sym:?}"));
            }
            partials.clear();
            return Ok(partials);
        }
        let mut next = SmallVec::with_capacity(partials.len());
        for partial in partials {
            self.counters.steps += 1;
            if self.counters.steps > self.limits.steps {
                return Err("step limit".into());
            }
            if let Sym::Rule(rule) = sym {
                let readings = self.rule(*rule, partial.at, depth + 1, whole)?;
                let mut base = Some(partial);
                for (i, reading) in readings.iter().enumerate() {
                    let mut extended = if i + 1 == readings.len() {
                        base.take().expect("last partial")
                    } else {
                        base.as_ref().expect("partial").clone()
                    };
                    extended.at = reading.end;
                    if need_binding {
                        extended.bindings.push((index, Rc::clone(&reading.value)));
                    }
                    next.push(extended);
                }
            } else {
                let pos = partial.at;
                self.max_read = self.max_read.max(pos);
                if let Some(value) = self.terminal(sym, pos) {
                    let mut extended = partial;
                    extended.at += 1;
                    if extended.at > self.furthest {
                        self.expected.clear();
                        self.expected_truncated = false;
                    }
                    self.furthest = self.furthest.max(extended.at);
                    self.sub_furthest = self.sub_furthest.max(extended.at);
                    if need_binding && let Some(value) = value {
                        extended.bindings.push((index, Rc::new(value)));
                    }
                    next.push(extended);
                } else if self.diagnostics {
                    self.missed(pos, format!("{sym:?}"));
                }
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
                (crate::tokens::literal_number(expected)? == *value).then_some(None)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn equal_readings_hash_alike_whatever_their_key_order_tags_or_zero_sign() {
        let pairs = [
            (
                json!({"a": 1, "b": [true, null]}),
                json!({"b": [true, null], "a": 1}),
            ),
            (
                json!({"x": {"kind": "It"}}),
                json!({"x": {"kind": "It", AUDIT_TAG: ["m_rule"]}}),
            ),
            (json!([0.0, {"n": -0.0}]), json!([-0.0, {"n": 0.0}])),
        ];
        for (left, right) in pairs {
            assert!(same_untagged(&left, &right), "{left} {right}");
            assert_eq!(
                untagged_hash(&left),
                untagged_hash(&right),
                "{left} {right}"
            );
        }
    }

    #[test]
    fn readings_that_differ_hash_apart_in_practice() {
        let values = [
            json!({"a": 1}),
            json!({"a": 2}),
            json!({"b": 1}),
            json!([1, 2]),
            json!([2, 1]),
            json!({"a": [1]}),
            json!("a"),
            json!(1),
            json!(1.0),
            json!(null),
        ];
        let hashes: std::collections::BTreeSet<u64> = values.iter().map(untagged_hash).collect();
        assert_eq!(hashes.len(), values.len());
    }
}
