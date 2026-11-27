//! The inverse direction: every derivation of a value, read from the same
//! declarations the parser instantiates. Recursion is bounded by the value and
//! each node has finitely many productions, so enumeration always terminates.
use crate::derivation::{Derivation, Node, Rope};
use crate::grammar::{Alternative, Class, Grammar, RuleId, Sym};
use crate::heads::{Reach, Values};
use crate::pattern::Bindings;
use crate::rtoken::{CompiledStep, RToken};
use crate::style::Model;
use crate::tokens::Token;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use serde_json::Value;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

mod branches;
mod context;
mod frontier;
pub mod memo;
mod prefer;
pub(crate) mod words;

pub use frontier::{Blame, Frontier};
pub use memo::FaceMemo;

type Found = Rc<Vec<Derivation>>;

#[derive(Clone)]
struct MemoEntry {
    found: Found,
    truncated: bool,
    taint: usize,
    cap_min: usize,
    native_dep: bool,
    consulted: Option<Rc<[RuleId]>>,
}

/// How many derivations a join may hold before the style model cuts them to the cap.
const WIDEN: usize = 4;
/// What each distinct alternative of a card's key is worth against everything else.
const KEY_WEIGHT: f32 = 1000.0;
/// What a `@prefer` is worth, taken or passed over: more than any style score.
const PREFER_WEIGHT: f32 = 100.0;

/// What printing did, to compare a change by more than the clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct Work {
    /// Distinct rule and value pairs tried.
    pub nodes: u64,
    /// Patterns unified, and how many matched.
    pub unified: u64,
    pub matched: u64,
    /// Alternatives walked, and derivations built by joining their symbols.
    pub walked: u64,
    pub joined: u64,
    /// Derivations ranked, and the tokens read to rank them.
    pub ranked: u64,
    pub read: u64,
    /// Rankings that dropped something.
    pub cuts: u64,
    /// Derivation-tree nodes visited to retain distinct ability productions.
    pub branch_nodes: u64,
}

impl Work {
    pub fn add(&mut self, other: &Self) {
        self.nodes += other.nodes;
        self.unified += other.unified;
        self.matched += other.matched;
        self.walked += other.walked;
        self.joined += other.joined;
        self.ranked += other.ranked;
        self.read += other.read;
        self.cuts += other.cuts;
        self.branch_nodes += other.branch_nodes;
    }
}

pub struct Renderer<'a> {
    pub work: Work,
    grammar: &'a Grammar,
    style: &'a Model,
    /// Alternatives a card's key names. A rule it names takes no others.
    key: HashSet<(RuleId, usize)>,
    keyed_rules: HashSet<RuleId>,
    /// Dense index `0..key.len()` for O(1) bitset key weighting when `key.len() <= 128`.
    key_index: HashMap<(RuleId, usize), u16>,
    /// 64-bit bloom filter of `keyed_rules` for O(1) disjointness checks on shared entries.
    key_bloom: u64,
    /// The most derivations kept for any one node.
    cap: usize,
    values: Values,
    /// `(derivations, truncated, min_cycle_depth, cap_min, consulted)` per `(rule, value_id, context)`.
    memo: HashMap<(RuleId, u32, usize), MemoEntry>,
    /// Currently active recursion stack entries mapped to their call depth.
    open: HashMap<(RuleId, u32, usize), usize>,
    open_stack: Vec<RuleId>,
    open_bloom: u64,
    depth: usize,
    min_cycle: usize,
    /// Largest `cap` threshold observed inside the current subtree (for cap-range validity).
    sub_cap_min: usize,
    /// Whether any rule consulted inside the current subtree has a native inverse.
    sub_native_dep: bool,
    /// Consulted rules accumulated for the current subtree (`None` on overflow).
    consult_stack: Vec<RuleId>,
    consult_overflow: bool,
    /// Generation-stamped O(N) dedup table for `finish_consulted`.
    consult_seen: Vec<u32>,
    consult_gen: u32,
    /// Shared unkeyed cache carried across prints of the same face.
    shared: Option<HashMap<(RuleId, u32, usize), memo::SharedEntry>>,
    nothing: Found,
    /// Set when a cap dropped a derivation, so a count is a lower bound.
    pub truncated: bool,
    /// The spelling of the card's name to print.
    self_name: u8,
    /// Card types, for `@prefer type(...)`.
    contexts: Vec<Vec<String>>,
    /// The card's best word for itself in each context.
    best: Vec<u32>,
    context: usize,
    /// The words of the card's name, when it is known: a seeded rule that calls the card
    /// by a name prints only for the card it names.
    name: Option<HashSet<String>>,
    /// The card's name itself, which `named <this card>` says and no other name does.
    card_name: Option<String>,
    literal_inverses: bool,
    /// Reusable scratch buffer for materializing `Rope<RToken>` without heap allocations.
    tok_buf: Vec<RToken>,
    /// Dynamic word strings from `Class::Word`/`Capitalized` not present in `grammar.word_table`.
    dyn_words: Vec<String>,
    dyn_ids: HashMap<String, u32>,
}

impl<'a> Renderer<'a> {
    pub fn new(grammar: &'a Grammar, style: &'a Model, cap: usize, context: Vec<String>) -> Self {
        let context = words::card_context(context);
        Self {
            work: Work::default(),
            grammar,
            style,
            key: HashSet::default(),
            keyed_rules: HashSet::default(),
            key_index: HashMap::default(),
            key_bloom: 0,
            cap,
            values: Values::default(),
            memo: HashMap::default(),
            open: HashMap::default(),
            open_stack: Vec::with_capacity(32),
            open_bloom: 0,
            depth: 0,
            min_cycle: usize::MAX,
            sub_cap_min: 0,
            sub_native_dep: false,
            consult_stack: Vec::new(),
            consult_overflow: false,
            consult_seen: Vec::new(),
            consult_gen: 0,
            shared: None,
            nothing: Rc::new(Vec::new()),
            truncated: false,
            self_name: 0,
            best: vec![grammar.words.best(&context)],
            contexts: vec![context],
            context: 0,
            name: None,
            card_name: None,
            literal_inverses: true,
            tok_buf: Vec::with_capacity(64),
            dyn_words: Vec::new(),
            dyn_ids: HashMap::default(),
        }
    }

    /// Print the card's name as this entry of its [`crate::tokens::Names`].
    pub fn with_self_name(mut self, index: u8) -> Self {
        self.self_name = index;
        self
    }

    /// Print for the card of this name.
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(words::name_words(name));
        self.card_name = Some(name.to_string());
        self
    }

    pub(crate) fn with_literal_inverses(mut self, enabled: bool) -> Self {
        self.literal_inverses = enabled;
        self
    }

    /// Hold every rule the key names to the alternatives it names.
    pub fn with_key(mut self, key: HashSet<(RuleId, usize)>) -> Self {
        self.keyed_rules = key.iter().map(|(rule, _)| *rule).collect();
        self.key_index.clear();
        if key.len() <= 128 {
            for (idx, &step) in key.iter().enumerate() {
                self.key_index.insert(step, idx as u16);
            }
        }
        self.key_bloom = self
            .keyed_rules
            .iter()
            .fold(0u64, |acc, &r| acc | memo::rule_bit(r));
        self.key = key;
        self
    }

    /// Attach a per-face memo so repeated prints of the same face share values,
    /// contexts, and cycle-free unkeyed subtrees.
    pub(crate) fn with_face_memo(mut self, mut face: FaceMemo) -> Self {
        face.align(self.card_name.as_deref(), self.self_name);
        let initial_ctx = self.contexts.swap_remove(0);
        self.values = face.values;
        self.contexts = face.contexts;
        self.best = face.best;
        self.dyn_words = face.dyn_words;
        self.dyn_ids = face.dyn_ids;
        self.context = self.intern_context(initial_ctx);
        self.shared = Some(face.unkeyed);
        self
    }

    /// Reclaim the per-face memo after rendering finishes.
    pub(crate) fn into_face_memo(self, parse: crate::parse::ParseMemo) -> FaceMemo {
        FaceMemo {
            values: self.values,
            contexts: self.contexts,
            best: self.best,
            dyn_words: self.dyn_words,
            dyn_ids: self.dyn_ids,
            unkeyed: self.shared.unwrap_or_default(),
            parse,
            card_name: self.card_name,
            self_name: self.self_name,
        }
    }

    /// Materialize a finished derivation's compact tokens into public [`Token`]s.
    pub fn tokens_of(&self, derivation: &Derivation) -> Vec<Token> {
        let mut rtoks = Vec::with_capacity(derivation.length);
        derivation.tokens.items_into(&mut rtoks);
        rtoks
            .into_iter()
            .map(|tok| self.grammar.word_table.to_token(tok, &self.dyn_words))
            .collect()
    }

    #[inline]
    fn key_named_count(&self, derivation: &Derivation) -> usize {
        if self.key.is_empty() {
            0
        } else if self.key.len() <= 128 {
            (derivation.key_bits[0].count_ones() + derivation.key_bits[1].count_ones()) as usize
        } else {
            let steps = derivation.productions.items();
            let named: HashSet<&(RuleId, usize)> = steps
                .iter()
                .filter(|step| self.key.contains(*step))
                .collect();
            named.len()
        }
    }

    /// What the key, `@prefer` and the canonical style rules add to a text's rank.
    pub fn weight(&self, derivation: &Derivation, tokens: &[Token]) -> f32 {
        let named = self.key_named_count(derivation);
        KEY_WEIGHT * named as f32
            + PREFER_WEIGHT * (derivation.preferred as f32 - derivation.missed as f32)
            - crate::style::broken_rules(tokens, &self.contexts[self.context])
    }

    #[inline]
    fn weight_rtokens(&self, derivation: &Derivation, tokens: &[RToken]) -> f32 {
        let named = self.key_named_count(derivation);
        KEY_WEIGHT * named as f32
            + PREFER_WEIGHT * (derivation.preferred as f32 - derivation.missed as f32)
            - crate::style::broken_rules_rtokens(
                tokens,
                derivation.style_flags,
                &self.contexts[self.context],
                &self.grammar.word_table,
                &self.dyn_words,
            )
    }

    /// `found` best first, with the alternative each came from, keeping one tree of each
    /// text: a beam full of one text in many derivations has no room for another text.
    fn scored(&mut self, found: Vec<Derivation>, opens: &[usize]) -> Vec<(usize, Derivation)> {
        let mut scored: Vec<(f32, u64, usize, Derivation)> = Vec::with_capacity(found.len());
        for (at, derivation) in found.into_iter().enumerate() {
            derivation.tokens.items_into(&mut self.tok_buf);
            let mut hasher = rustc_hash::FxHasher::default();
            self.tok_buf.hash(&mut hasher);
            let hash = hasher.finish();
            self.work.ranked += 1;
            self.work.read += self.tok_buf.len() as u64;
            let score = self.style.score_rtokens(
                &self.tok_buf,
                &self.grammar.word_table,
                &self.dyn_words,
                false,
            ) + self.weight_rtokens(&derivation, &self.tok_buf);
            let alternative = opens.partition_point(|open| *open <= at);
            scored.push((score, hash, alternative, derivation));
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut seen = HashSet::default();
        scored
            .into_iter()
            .filter(|(_, text, ..)| seen.insert(*text))
            .map(|(_, _, alternative, derivation)| (alternative, derivation))
            .collect()
    }

    fn ranked(&mut self, found: Vec<Derivation>) -> Vec<Derivation> {
        self.scored(found, &[])
            .into_iter()
            .map(|(_, derivation)| derivation)
            .collect()
    }

    /// `found` ranked, with the best text that passes over a better word for the card
    /// moved up behind the best, so that a cut product keeps it, as
    /// [`Self::best_of_each`] does.
    fn ranked_heads(&mut self, found: Vec<Derivation>) -> Vec<Derivation> {
        let mut ranked = self.ranked(found);
        let fewest = ranked.iter().map(|derivation| derivation.missed).min();
        if let Some(at) = ranked.iter().position(|d| Some(d.missed) > fewest)
            && at > 1
        {
            let passing = ranked.remove(at);
            ranked.insert(1, passing);
        }
        ranked
    }

    /// Derivations of `value` from `rule`, likeliest first. Empty means this
    /// grammar has no text for the value.
    pub fn render(&mut self, rule: RuleId, value: &Value) -> Found {
        let found = self.derive(rule, value);
        Rc::new(self.ranked(found.to_vec()))
    }

    /// Whether any rule's own pattern prints this value, whatever rule asked for it.
    pub fn printable(&mut self, value: &Value) -> bool {
        let (id, reach) = self.values.id(&self.grammar.net, value);
        let fits = reach.fits.clone();
        fits.iter()
            .any(|rule| !self.render_id(*rule, value, (id, &reach)).is_empty())
    }

    /// The rules whose own pattern prints this value.
    pub fn printers(&mut self, value: &Value) -> Vec<RuleId> {
        let (id, reach) = self.values.id(&self.grammar.net, value);
        let fits = reach.fits.clone();
        fits.into_iter()
            .filter(|rule| !self.render_id(*rule, value, (id, &reach)).is_empty())
            .collect()
    }

    /// Derivations in no order, which only a cut or the finished text needs.
    fn derive(&mut self, rule: RuleId, value: &Value) -> Found {
        let (id, reach) = self.values.id(&self.grammar.net, value);
        self.render_id(rule, value, (id, &reach))
    }

    fn render_id(&mut self, rule: RuleId, value: &Value, (id, reach): (u32, &Reach)) -> Found {
        let outer = self.context;
        if let Some(context) = context::own(value) {
            self.context = self.intern_context(context);
        }
        let found = self.render_contextual(rule, value, (id, reach));
        self.context = outer;
        found
    }

    fn intern_context(&mut self, context: Vec<String>) -> usize {
        self.contexts
            .iter()
            .position(|existing| existing == &context)
            .unwrap_or_else(|| {
                self.best.push(self.grammar.words.best(&context));
                self.contexts.push(context);
                self.contexts.len() - 1
            })
    }

    #[inline]
    fn record_consulted(&mut self, rules: Option<&[RuleId]>) {
        if self.shared.is_none() || self.consult_overflow {
            return;
        }
        match rules {
            Some(list) => {
                if self.consult_stack.len() + list.len() > 8192 {
                    self.consult_overflow = true;
                } else {
                    self.consult_stack.extend_from_slice(list);
                }
            }
            None => self.consult_overflow = true,
        }
    }

    fn lookup_memo(&mut self, key: (RuleId, u32, usize)) -> Option<Found> {
        if let Some(entry) = self.memo.get(&key) {
            let found = Rc::clone(&entry.found);
            let trunc = entry.truncated;
            let taint = entry.taint;
            let cap_min = entry.cap_min;
            let native_dep = entry.native_dep;
            let consulted = entry.consulted.clone();
            self.truncated |= trunc;
            self.min_cycle = self.min_cycle.min(taint);
            self.sub_cap_min = self.sub_cap_min.max(cap_min);
            self.sub_native_dep |= native_dep;
            self.record_consulted(consulted.as_deref());
            return Some(found);
        }
        if let Some(shared) = &self.shared
            && let Some(entry) = shared.get(&key)
            && entry.fits_cap(self.cap)
            && entry.fits_native(self.literal_inverses)
            && entry.disjoint_from(self.key_bloom, &self.keyed_rules)
            && entry.disjoint_from_open(self.open_bloom, &self.open_stack)
        {
            let found = Rc::clone(&entry.found);
            let trunc = entry.truncated;
            let cap_min = entry.cap_min;
            let native_dep = entry.native.is_some();
            let consulted = entry.consulted.clone();
            self.truncated |= trunc;
            self.sub_cap_min = self.sub_cap_min.max(cap_min);
            self.sub_native_dep |= native_dep;
            self.record_consulted(consulted.as_deref());
            self.memo.insert(
                key,
                MemoEntry {
                    found: Rc::clone(&found),
                    truncated: trunc,
                    taint: usize::MAX,
                    cap_min,
                    native_dep,
                    consulted,
                },
            );
            return Some(found);
        }
        None
    }

    fn finish_consulted(
        &mut self,
        consult_start: usize,
        outer_overflow: bool,
    ) -> (Option<Rc<[RuleId]>>, u64) {
        if self.shared.is_none() {
            return (None, 0);
        }
        if self.consult_overflow {
            self.consult_stack.truncate(consult_start);
            return (None, 0);
        }
        let slice = &mut self.consult_stack[consult_start..];
        let out = if slice.len() <= 1 {
            let bloom = slice.first().map_or(0, |&r| memo::rule_bit(r));
            (Some(Rc::<[RuleId]>::from(&*slice)), bloom)
        } else {
            if self.consult_seen.len() < self.grammar.rules.len() {
                self.consult_seen.resize(self.grammar.rules.len(), 0);
            }
            self.consult_gen = self.consult_gen.wrapping_add(1);
            if self.consult_gen == 0 {
                self.consult_seen.fill(0);
                self.consult_gen = 1;
            }
            let gen_id = self.consult_gen;
            let mut len = 0usize;
            let mut bloom = 0u64;
            let mut overflowed = false;
            for i in 0..slice.len() {
                let r = slice[i];
                let slot = &mut self.consult_seen[r as usize];
                if *slot != gen_id {
                    *slot = gen_id;
                    if len == memo::MAX_CONSULTED {
                        overflowed = true;
                        break;
                    }
                    slice[len] = r;
                    bloom |= memo::rule_bit(r);
                    len += 1;
                }
            }
            if overflowed {
                (None, 0)
            } else {
                let unique = &mut slice[..len];
                unique.sort_unstable();
                (Some(Rc::<[RuleId]>::from(&*unique)), bloom)
            }
        };
        self.consult_stack.truncate(consult_start);
        self.consult_overflow = outer_overflow;
        if let Some(list) = &out.0 {
            self.record_consulted(Some(list));
        } else {
            self.consult_overflow = true;
        }
        out
    }

    fn store_shared(
        &mut self,
        key: (RuleId, u32, usize),
        found: &Found,
        (sub_trunc, my_cap_min, my_native_dep): (bool, usize, bool),
        (consulted, bloom): (Option<Rc<[RuleId]>>, u64),
    ) {
        let Some(shared) = &mut self.shared else {
            return;
        };
        let unkeyed = self.key.is_empty()
            || consulted.as_deref().is_some_and(|list| {
                (bloom & self.key_bloom) == 0 || list.iter().all(|r| !self.keyed_rules.contains(r))
            });
        if !unkeyed {
            return;
        }
        let native = my_native_dep.then_some(self.literal_inverses);
        let replace = match shared.get(&key) {
            None => true,
            Some(prev) => {
                // Keep native-agnostic or `native = true` entries over `native = false`;
                // within compatible native modes, prefer untruncated over truncated.
                let compatible =
                    prev.native == native || (prev.native.is_some() && native.is_none());
                compatible
                    && (prev.truncated && !sub_trunc || prev.native.is_some() && native.is_none())
            }
        };
        if replace {
            shared.insert(
                key,
                memo::SharedEntry {
                    found: Rc::clone(found),
                    truncated: sub_trunc,
                    cap_min: my_cap_min,
                    cap_max: sub_trunc.then_some(self.cap),
                    native,
                    bloom,
                    consulted,
                },
            );
        }
    }

    fn render_contextual(
        &mut self,
        rule: RuleId,
        value: &Value,
        (id, reach): (u32, &Reach),
    ) -> Found {
        crate::print_trace::rule(rule);
        if !self.grammar.net.reaches(reach, rule) {
            return Rc::clone(&self.nothing);
        }
        let key = (rule, id, self.context);
        if let Some(&at) = self.open.get(&key) {
            self.min_cycle = self.min_cycle.min(at);
            return Rc::clone(&self.nothing);
        }
        if let Some(found) = self.lookup_memo(key) {
            return found;
        }
        // Cut a cycle through a pass-through rule: it adds no tokens, so it can
        // never produce a text the direct route does not.
        let my_depth = self.depth;
        self.depth += 1;
        self.open.insert(key, my_depth);
        self.open_stack.push(rule);
        let outer_open_bloom = self.open_bloom;
        self.open_bloom |= memo::rule_bit(rule);
        let outer_cycle = self.min_cycle;
        self.min_cycle = usize::MAX;
        let outer_trunc = self.truncated;
        self.truncated = false;
        let outer_cap_min = self.sub_cap_min;
        self.sub_cap_min = 0;
        let outer_native_dep = self.sub_native_dep;
        self.sub_native_dep = false;
        let consult_start = self.consult_stack.len();
        let outer_overflow = self.consult_overflow;
        self.consult_overflow = false;
        if self.shared.is_some() {
            self.consult_stack.push(rule);
        }
        self.work.nodes += 1;
        let grammar = self.grammar;
        let definition = &grammar.rules[rule as usize];
        let mut all = Vec::new();
        let mut opens = Vec::new();
        match &definition.sem {
            Some(pattern) => {
                self.sub_native_dep |= grammar.has_native_inverse(rule);
                let mut solutions = grammar.print_unify_guided(rule, value, self.literal_inverses);
                // `named <this card>` says this card's name and no other.
                solutions.retain(|bindings| {
                    let said = bindings.get(crate::pattern::CARD_NAME);
                    said.zip(self.card_name.as_deref())
                        .is_none_or(|(said, name)| said.as_str() == Some(name))
                });
                self.work.unified += 1;
                self.work.matched += u64::from(!solutions.is_empty());
                if !solutions.is_empty() {
                    let contexts: Vec<_> = solutions
                        .iter()
                        .map(|bindings| {
                            context::holes(pattern, value, bindings, &self.contexts[self.context])
                        })
                        .collect();
                    for (index, alternative) in definition.alternatives.iter().enumerate() {
                        if self.allowed(rule, index) {
                            let found = solutions
                                .iter()
                                .zip(&contexts)
                                .flat_map(|(bindings, contexts)| {
                                    self.alternative(rule, index, alternative, bindings, contexts)
                                })
                                .collect();
                            opens.push(all.len());
                            self.take((rule, index), alternative, value, found, &mut all);
                        }
                    }
                }
            }
            None => {
                for index in grammar.net.alternatives(reach, rule) {
                    let index = *index as usize;
                    if self.allowed(rule, index) {
                        let alternative = &definition.alternatives[index];
                        let found = self.passthrough(alternative, value, (id, reach));
                        opens.push(all.len());
                        self.take((rule, index), alternative, value, found, &mut all);
                    }
                }
            }
        }
        self.sub_cap_min = self.sub_cap_min.max(all.len());
        let found = Rc::new(self.best_of_each(rule, all, &opens));
        self.open.remove(&key);
        self.open_stack.pop();
        self.open_bloom = outer_open_bloom;
        self.depth = my_depth;
        let sub_trunc = self.truncated;
        let my_cycle = self.min_cycle;
        let my_cap_min = if sub_trunc {
            self.cap
        } else {
            self.sub_cap_min
        };
        let my_native_dep = self.sub_native_dep;
        self.truncated = outer_trunc || sub_trunc;
        self.sub_cap_min = outer_cap_min.max(my_cap_min);
        self.sub_native_dep = outer_native_dep || my_native_dep;
        let propagated_cycle = if my_cycle < my_depth {
            my_cycle
        } else {
            usize::MAX
        };
        self.min_cycle = outer_cycle.min(propagated_cycle);
        let (consulted, bloom) = self.finish_consulted(consult_start, outer_overflow);
        self.memo.insert(
            key,
            MemoEntry {
                found: Rc::clone(&found),
                truncated: sub_trunc,
                taint: propagated_cycle,
                cap_min: my_cap_min,
                native_dep: my_native_dep,
                consulted: consulted.clone(),
            },
        );
        if my_cycle == usize::MAX {
            self.store_shared(
                key,
                &found,
                (sub_trunc, my_cap_min, my_native_dep),
                (consulted, bloom),
            );
        }
        found
    }

    /// At most the beam's width, reserving representatives of ability productions
    /// through aliases and card-list wrappers, or alternatives for other values.
    /// A rule is ranked with no text around it, and what reads best alone may read worst
    /// in a sentence: every `this <noun>` beats `it` until there is something to refer to,
    /// so the best text that passes over a better word for the card keeps a place too.
    /// `opens` says where each alternative starts in `found`.
    fn best_of_each(
        &mut self,
        rule: RuleId,
        found: Vec<Derivation>,
        opens: &[usize],
    ) -> Vec<Derivation> {
        if found.len() <= self.cap {
            return found;
        }
        self.work.cuts += 1;
        self.truncated = true;
        let mut seen = HashSet::default();
        let mut reserved = self.cap / 2;
        let mut first = Vec::new();
        let mut rest = Vec::new();
        let fewest = found.iter().map(|derivation| derivation.missed).min();
        let mut passing = true;
        for (alternative, derivation) in self.scored(found, opens) {
            let distinct = if reserved > 0 {
                let (branch, nodes) = branches::of(self.grammar, rule, alternative, &derivation);
                self.work.branch_nodes += nodes;
                seen.insert(branch)
            } else {
                false
            };
            let passes = passing && Some(derivation.missed) > fewest;
            if distinct || passes {
                reserved = reserved.saturating_sub(usize::from(distinct));
                passing &= !passes;
                first.push(derivation);
            } else {
                rest.push(derivation);
            }
        }
        rest.truncate(self.cap - first.len());
        first.append(&mut rest);
        first
    }

    fn allowed(&self, rule: RuleId, index: usize) -> bool {
        if self.keyed_rules.contains(&rule) {
            return self.key.contains(&(rule, index));
        }
        let name = self.name.as_ref();
        name.is_none_or(|name| self.grammar.words.fit((rule, index), name))
    }

    /// Add what one alternative derived to its rule's derivations. Where a value is the
    /// card itself, the innermost such production weighs the word its text says for the
    /// card, once: `it` or `this creature` for a legendary card passes over the card's
    /// best word. A text weighs the words it says, not the ones it avoids: `another
    /// creature` passes over nothing against `a creature other than this creature`.
    fn take(
        &self,
        step: (RuleId, usize),
        alternative: &Alternative,
        value: &Value,
        found: Vec<Derivation>,
        all: &mut Vec<Derivation>,
    ) {
        if found.is_empty() {
            return;
        }
        let preferred = prefer::preferred(self.grammar, alternative, value);
        // The card's best word for itself, where its context says what it is. A word for
        // something it is not, as `this Saga` is for a creature, misnames it as well as
        // passing over that word.
        // A seeded pronoun for the card is a word for it too: `on her` passes over
        // `this land`.
        let best = self.best[self.context];
        let word = self.grammar.words.of(step.0, &self.contexts[self.context]);
        let misnames = u32::from(word == Some(0) && best > 0);
        let word = word.unwrap_or(0);
        let closes = self.grammar.rules[step.0 as usize].sem.is_some();
        let pronoun = self.grammar.words.by_pronoun(step);
        let best = (closes && (words::is_the_card(value) || pronoun)).then_some(best);
        let leaf = Rope::leaf(step);
        let key_bit = self.key_index.get(&step).copied();
        for mut derivation in found {
            derivation.productions = derivation.productions.join(&leaf);
            if let Some(bit) = key_bit {
                derivation.key_bits[(bit >> 6) as usize] |= 1u64 << (bit & 63);
            }
            derivation.preferred += preferred;
            derivation.missed += misnames;
            derivation.word = derivation.word.max(word);
            if let Some(best) = best
                && derivation.references == 0
            {
                derivation.missed += u32::from(derivation.word < best);
                derivation.references = 1;
            }
            if closes {
                derivation.close(step.0, step.1);
            }
            all.push(derivation);
        }
    }

    fn intern_word(&mut self, word: &str) -> (RToken, u64) {
        if let Some(&id) = self.grammar.word_table.ids.get(word) {
            let flags = self.grammar.word_table.word_flags[id as usize];
            return (RToken::Word(id), flags);
        }
        let flags = crate::style::word_flags(word);
        let id = if let Some(&id) = self.dyn_ids.get(word) {
            id
        } else {
            let id = u32::try_from(self.dyn_words.len()).unwrap_or(u32::MAX);
            self.dyn_words.push(word.to_string());
            self.dyn_ids.insert(word.to_string(), id);
            id
        };
        (RToken::Dynamic(id), flags)
    }

    fn class_rtoken(
        &mut self,
        class: Class,
        value: &Value,
    ) -> Option<(RToken, u64, Option<Value>)> {
        match (class, value) {
            (Class::Number, Value::Number(number)) => {
                let n = u32::try_from(number.as_u64()?).ok()?;
                Some((RToken::Number(n), 0, Some(Value::from(n))))
            }
            (Class::Word | Class::Capitalized, Value::String(word)) => {
                let (tok, flags) = self.intern_word(word);
                Some((tok, flags, Some(Value::from(word.clone()))))
            }
            (Class::SelfName, _) => Some((
                RToken::SelfName(self.self_name),
                crate::style::F_SELF_NAME,
                Some(Value::from("$self")),
            )),
            _ => None,
        }
    }

    fn passthrough(
        &mut self,
        alternative: &Alternative,
        value: &Value,
        at: (u32, &Reach),
    ) -> Vec<Derivation> {
        match alternative.syms.first() {
            Some(Sym::Rule(rule)) => self.render_id(*rule, value, at).to_vec(),
            Some(Sym::Class(class)) => self
                .class_rtoken(*class, value)
                .map(|(tok, flags, val)| Derivation::token(tok, flags, val))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }

    fn join_rule(
        &mut self,
        partial: Vec<Derivation>,
        sym_idx: usize,
        rule: RuleId,
        hole: Option<&str>,
        bindings: &Bindings<'_, '_>,
        contexts: &[(&str, Vec<String>)],
    ) -> Vec<Derivation> {
        let limit = self.cap * WIDEN;
        let value = hole.and_then(|name| bindings.get(name));
        let outer = self.context;
        if let Some((_, context)) = contexts.iter().find(|(name, _)| hole == Some(*name)) {
            self.context = self.intern_context(context.clone());
        }
        let mut pieces = self.derive(rule, value.unwrap_or(&Value::Null));
        self.context = outer;
        if pieces.is_empty() {
            return Vec::new();
        }
        let mut partial = partial;
        let prod = partial.len() * pieces.len();
        self.sub_cap_min = self.sub_cap_min.max(prod.div_ceil(WIDEN));
        // The product has to fit, so when it does not, keep the best of each side.
        if prod > limit {
            self.truncated = true;
            partial = self.ranked_heads(partial);
            pieces = Rc::new(self.ranked(pieces.to_vec()));
        }
        let mut next = Vec::with_capacity(limit.min(partial.len() * pieces.len()));
        'heads: for head in &partial {
            for piece in pieces.iter() {
                if next.len() >= limit {
                    break 'heads;
                }
                self.work.joined += 1;
                next.push(head.then(piece, sym_idx));
            }
        }
        next
    }

    fn alternative(
        &mut self,
        rule_id: RuleId,
        alt_idx: usize,
        alternative: &Alternative,
        bindings: &Bindings<'_, '_>,
        contexts: &[(&str, Vec<String>)],
    ) -> Vec<Derivation> {
        self.work.walked += 1;
        let mut partial = vec![Derivation::default()];
        if let Some(steps) = self
            .grammar
            .word_table
            .compiled_alts
            .get(rule_id as usize)
            .and_then(|alts| alts.get(alt_idx))
        {
            for step in steps {
                match step {
                    CompiledStep::Literal { tokens, flags } => {
                        let rope = Rope::chunk(std::sync::Arc::clone(tokens));
                        let len = tokens.len();
                        for d in &mut partial {
                            d.append_literal(&rope, len, *flags);
                        }
                        self.work.joined += partial.len() as u64;
                    }
                    CompiledStep::SelfName { sym } => {
                        let tok = RToken::SelfName(self.self_name);
                        let node = Rc::new(Node::Value(Value::from("$self")));
                        for d in &mut partial {
                            d.append_token(
                                tok,
                                crate::style::F_SELF_NAME,
                                *sym,
                                Some(Rc::clone(&node)),
                            );
                        }
                        self.work.joined += partial.len() as u64;
                    }
                    CompiledStep::Class { sym, class, hole } => {
                        let value = hole.as_deref().and_then(|name| bindings.get(name));
                        let Some((tok, flags, val)) =
                            value.and_then(|v| self.class_rtoken(*class, v))
                        else {
                            return Vec::new();
                        };
                        let node = val.map(|v| Rc::new(Node::Value(v)));
                        for d in &mut partial {
                            d.append_token(tok, flags, *sym, node.clone());
                        }
                        self.work.joined += partial.len() as u64;
                    }
                    CompiledStep::Rule { sym, rule, hole } => {
                        partial = self.join_rule(
                            partial,
                            *sym,
                            *rule,
                            hole.as_deref(),
                            bindings,
                            contexts,
                        );
                        if partial.is_empty() {
                            return partial;
                        }
                    }
                }
            }
            return partial;
        }
        // Fallback for synthetic grammars constructed directly in unit tests.
        for (index, (sym, hole)) in alternative.syms.iter().zip(&alternative.holes).enumerate() {
            match sym {
                Sym::Word(word) => {
                    let (tok, flags) = RToken::number_literal(word)
                        .map(|number| (number, 0))
                        .unwrap_or_else(|| self.intern_word(word));
                    let rope = Rope::leaf(tok);
                    for d in &mut partial {
                        d.append_literal(&rope, 1, flags);
                    }
                    self.work.joined += partial.len() as u64;
                }
                Sym::Mark(mark) => {
                    let rope = Rope::leaf(RToken::Mark(*mark));
                    let flags = crate::style::mark_flags(*mark);
                    for d in &mut partial {
                        d.append_literal(&rope, 1, flags);
                    }
                    self.work.joined += partial.len() as u64;
                }
                Sym::Class(Class::Newline) => {
                    let rope = Rope::leaf(RToken::Newline);
                    for d in &mut partial {
                        d.append_literal(&rope, 1, 0);
                    }
                    self.work.joined += partial.len() as u64;
                }
                Sym::Class(Class::SelfName) => {
                    let tok = RToken::SelfName(self.self_name);
                    let node = Rc::new(Node::Value(Value::from("$self")));
                    for d in &mut partial {
                        d.append_token(
                            tok,
                            crate::style::F_SELF_NAME,
                            index,
                            Some(Rc::clone(&node)),
                        );
                    }
                    self.work.joined += partial.len() as u64;
                }
                Sym::Class(class) => {
                    let value = hole.as_deref().and_then(|name| bindings.get(name));
                    let Some((tok, flags, val)) = value.and_then(|v| self.class_rtoken(*class, v))
                    else {
                        return Vec::new();
                    };
                    let node = val.map(|v| Rc::new(Node::Value(v)));
                    for d in &mut partial {
                        d.append_token(tok, flags, index, node.clone());
                    }
                    self.work.joined += partial.len() as u64;
                }
                Sym::Rule(rule) => {
                    partial =
                        self.join_rule(partial, index, *rule, hole.as_deref(), bindings, contexts);
                    if partial.is_empty() {
                        return partial;
                    }
                }
            }
        }
        partial
    }
}

/// Whether a value can render as exactly these tokens. This proves the inverse
/// reaches a text without enumerating every other text on the way.
pub struct Matcher<'a> {
    grammar: &'a Grammar,
    target: &'a [Token],
    values: Values,
    memo: HashMap<(RuleId, usize, u32), Rc<Vec<usize>>>,
    nothing: Rc<Vec<usize>>,
    frontier: Frontier,
    track_frontier: bool,
}

type Open = HashSet<(RuleId, usize, usize, u32)>;

impl<'a> Matcher<'a> {
    pub fn new(grammar: &'a Grammar, target: &'a [Token]) -> Self {
        Self {
            grammar,
            target,
            values: Values::default(),
            memo: HashMap::default(),
            nothing: Rc::new(Vec::new()),
            frontier: Frontier::default(),
            track_frontier: false,
        }
    }

    pub fn with_frontier(mut self) -> Self {
        self.track_frontier = true;
        self
    }

    pub fn reaches(&mut self, rule: RuleId, value: &Value) -> bool {
        self.ends(rule, value, 0).contains(&self.target.len())
    }

    /// The alternatives of one derivation of the whole target.
    pub fn derivation(&mut self, rule: RuleId, value: &Value) -> Vec<(RuleId, usize)> {
        let mut steps = Vec::new();
        let mut open = HashSet::default();
        self.pick(rule, value, (0, self.target.len()), &mut steps, &mut open);
        steps
    }

    fn pick(
        &mut self,
        rule: RuleId,
        value: &Value,
        span: (usize, usize),
        steps: &mut Vec<(RuleId, usize)>,
        open: &mut Open,
    ) -> bool {
        let (id, reach) = self.values.id(&self.grammar.net, value);
        self.pick_id(rule, (value, id, &reach), span, steps, open)
    }

    fn pick_id(
        &mut self,
        rule: RuleId,
        (value, id, reach): (&Value, u32, &Reach),
        span: (usize, usize),
        steps: &mut Vec<(RuleId, usize)>,
        open: &mut Open,
    ) -> bool {
        let key = (rule, span.0, span.1, id);
        let ends = self.ends_id(rule, (value, id, reach), span.0);
        if !ends.contains(&span.1) || !open.insert(key) {
            return false;
        }
        let grammar = self.grammar;
        let definition = &grammar.rules[rule as usize];
        let held = steps.len();
        let mut found = false;
        match &definition.sem {
            Some(_) => {
                let solutions = if self.track_frontier {
                    grammar.print_unify(rule, value, true)
                } else {
                    grammar.print_unify_guided(rule, value, true)
                };
                'alternatives: for (index, alternative) in
                    definition.alternatives.iter().enumerate()
                {
                    for bindings in &solutions {
                        if self.pick_from(alternative, bindings, (0, span), steps, open) {
                            steps.push((rule, index));
                            found = true;
                            break 'alternatives;
                        }
                        steps.truncate(held);
                    }
                }
            }
            None => {
                for index in grammar.net.alternatives(reach, rule) {
                    found = match definition.alternatives[*index as usize].syms.first() {
                        Some(Sym::Rule(child)) => {
                            self.pick_id(*child, (value, id, reach), span, steps, open)
                        }
                        Some(sym) => self.terminal(sym, Some(value), span.0) == Some(span.1),
                        None => false,
                    };
                    if found {
                        steps.push((rule, *index as usize));
                        break;
                    }
                    steps.truncate(held);
                }
            }
        }
        open.remove(&key);
        found
    }

    /// Walk an alternative's symbols from `from.0` across the span `from.1`.
    fn pick_from(
        &mut self,
        alternative: &Alternative,
        bindings: &Bindings<'_, '_>,
        from: (usize, (usize, usize)),
        steps: &mut Vec<(RuleId, usize)>,
        open: &mut Open,
    ) -> bool {
        let (index, (at, end)) = from;
        let Some(sym) = alternative.syms.get(index) else {
            return at == end;
        };
        let hole = alternative.holes.get(index).and_then(Option::as_ref);
        let value = hole.and_then(|name| bindings.get(name));
        let held = steps.len();
        for to in self.step(sym, value, at) {
            let walked = match sym {
                Sym::Rule(child) => {
                    let inner = value.unwrap_or(&Value::Null);
                    self.pick(*child, inner, (at, to), steps, open)
                }
                _ => true,
            };
            if walked && self.pick_from(alternative, bindings, (index + 1, (to, end)), steps, open)
            {
                return true;
            }
            steps.truncate(held);
        }
        false
    }

    fn step(&mut self, sym: &Sym, value: Option<&Value>, at: usize) -> Vec<usize> {
        match sym {
            Sym::Rule(rule) => self.ends(*rule, value.unwrap_or(&Value::Null), at).to_vec(),
            other => self.terminal(other, value, at).into_iter().collect(),
        }
    }

    fn ends(&mut self, rule: RuleId, value: &Value, at: usize) -> Rc<Vec<usize>> {
        let (id, reach) = self.values.id(&self.grammar.net, value);
        self.ends_id(rule, (value, id, &reach), at)
    }

    fn ends_id(
        &mut self,
        rule: RuleId,
        (value, id, reach): (&Value, u32, &Reach),
        at: usize,
    ) -> Rc<Vec<usize>> {
        crate::print_trace::rule(rule);
        if !self.grammar.net.reaches(reach, rule) {
            return Rc::clone(&self.nothing);
        }
        if let Some(found) = self.memo.get(&(rule, at, id)) {
            return Rc::clone(found);
        }
        self.memo.insert((rule, at, id), Rc::clone(&self.nothing));
        let grammar = self.grammar;
        let definition = &grammar.rules[rule as usize];
        let mut out = Vec::new();
        match &definition.sem {
            Some(_) => {
                let solutions = if self.track_frontier {
                    grammar.print_unify(rule, value, true)
                } else {
                    grammar.print_unify_guided(rule, value, true)
                };
                if solutions.is_empty() {
                    self.blame(at, rule, Blame::Unify);
                }
                for (index, alternative) in definition.alternatives.iter().enumerate() {
                    for bindings in &solutions {
                        out.extend(self.sequence((rule, index), alternative, bindings, at));
                    }
                }
            }
            None => {
                for index in grammar.net.alternatives(reach, rule) {
                    match definition.alternatives[*index as usize].syms.first() {
                        Some(Sym::Rule(child)) => {
                            let ends = self.ends_id(*child, (value, id, reach), at);
                            out.extend(ends.iter().copied());
                        }
                        Some(sym) => out.extend(self.terminal(sym, Some(value), at)),
                        None => {}
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        let found = Rc::new(out);
        self.memo.insert((rule, at, id), Rc::clone(&found));
        found
    }

    fn sequence(
        &mut self,
        (rule, index): (RuleId, usize),
        alternative: &Alternative,
        bindings: &Bindings<'_, '_>,
        at: usize,
    ) -> Vec<usize> {
        let mut positions = vec![at];
        for (symbol, (sym, hole)) in alternative.syms.iter().zip(&alternative.holes).enumerate() {
            let value = hole.as_ref().and_then(|name| bindings.get(name));
            let mut next = Vec::new();
            for &position in &positions {
                match sym {
                    Sym::Rule(child) => next.extend(
                        self.ends(*child, value.unwrap_or(&Value::Null), position)
                            .iter()
                            .copied(),
                    ),
                    other => next.extend(self.terminal(other, value, position)),
                }
            }
            next.sort_unstable();
            next.dedup();
            if next.is_empty() {
                let blame = Blame::Symbol {
                    alternative: index,
                    symbol,
                };
                for position in positions {
                    self.blame(position, rule, blame);
                }
                return next;
            }
            if let Some(last) = next.last() {
                self.reached(*last);
            }
            positions = next;
        }
        positions
    }

    fn terminal(&self, sym: &Sym, value: Option<&Value>, at: usize) -> Option<usize> {
        let token = self.target.get(at)?;
        let wanted = match sym {
            Sym::Word(word) => word_token(word),
            Sym::Mark(mark) => Token::Mark(*mark),
            Sym::Class(Class::Newline) => Token::Newline,
            Sym::Class(Class::SelfName) => {
                return matches!(token, Token::SelfName(_)).then_some(at + 1);
            }
            Sym::Class(_) => class_token(sym, value?)?,
            Sym::Rule(_) => return None,
        };
        // A number reads the same with or without thousands separators.
        let same = match (&wanted, token) {
            (Token::Number(number), Token::Grouped(grouped))
            | (Token::Grouped(grouped), Token::Number(number)) => number == grouped,
            _ => wanted == *token,
        };
        same.then_some(at + 1)
    }
}

pub(crate) fn word_token(word: &str) -> Token {
    match word.parse() {
        Ok(n) => Token::Number(n),
        Err(_) => crate::tokens::grouped_literal(word)
            .map_or_else(|| Token::Word(word.to_string()), Token::Grouped),
    }
}

pub(crate) fn class_token(sym: &Sym, value: &Value) -> Option<Token> {
    match (sym, value) {
        (Sym::Class(Class::Number), Value::Number(number)) => {
            Some(Token::Number(u32::try_from(number.as_u64()?).ok()?))
        }
        (Sym::Class(Class::Word | Class::Capitalized), Value::String(word)) => {
            Some(Token::Word(word.clone()))
        }
        (Sym::Class(Class::SelfName), _) => Some(Token::SelfName(0)),
        _ => None,
    }
}
