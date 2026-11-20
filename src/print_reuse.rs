//! Which cards a grammar change cannot reach, judged from the traces of their last print.
//!
//! A card's row carries over only when its printing and reading would go the same way
//! under the new grammar:
//! - no rule its print consulted or reached changed, except a pass-through rule that only
//!   gained or lost alternatives that none of its looked-up values can take;
//! - no changed or newly reached rule can fit a value the print looked up: the value's
//!   leading steps must follow the rule's pattern, and the value must hold every node
//!   every value the pattern prints holds (`print_reuse_fits.rs`);
//! - no changed alternative can read any span of any text the card read: each requires a
//!   word that none of those texts contains;
//! - the tokenizer asked the vocabulary about no word that came or went;
//! - every parse kept wide headroom under its step and depth limits, so alternatives
//!   that fail cannot exhaust a bound they used to clear.
//!
//! A later full print checks the conclusions; see `docs/print-reuse.md`.
use crate::grammar::{Alternative, Grammar, RuleId, Sym};
use crate::parse::Limits;
use crate::print_trace::word_key;
use fits::Reaching;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet, FxHasher};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

/// What one card's print consulted, as saved beside its row. Rules are ids in the
/// grammar it printed under, and every list is sorted.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Saved {
    pub rules: Vec<RuleId>,
    pub prefixes: Vec<u32>,
    pub features: Vec<u32>,
    pub words: Vec<u32>,
    pub pairs: Vec<u32>,
    pub vocabulary: Vec<u32>,
    pub steps: u64,
    pub depth: usize,
}

impl Saved {
    pub fn from_trace(trace: &crate::print_trace::Trace) -> Self {
        let sorted = |set: &HashSet<u32>| {
            let mut out: Vec<u32> = set.iter().copied().collect();
            out.sort_unstable();
            out
        };
        Self {
            rules: sorted(&trace.rules),
            prefixes: sorted(&trace.prefixes),
            features: sorted(&trace.features),
            words: sorted(&trace.words),
            pairs: sorted(&trace.pairs),
            vocabulary: sorted(&trace.vocabulary),
            steps: trace.steps,
            depth: trace.depth,
        }
    }
}

fn has(sorted: &[u32], item: u32) -> bool {
    sorted.binary_search(&item).is_ok()
}

#[derive(Debug)]
struct Open {
    /// The child's key, for reports.
    key: String,
    /// The child's id in the old grammar, if it has one.
    old: Option<RuleId>,
    /// Whether the alternative to the child is new.
    added: bool,
    reach: Reaching,
}

/// What a rule in the changed zone needs of the traced values.
#[derive(Debug, Default)]
struct Children {
    /// Old children of alternatives the new grammar lost.
    lost: Vec<RuleId>,
    /// The old seeds under them.
    removed: fits::Lost,
    /// New children whose reach may differ from the old grammar's.
    open: Vec<Open>,
}

/// Why a card cannot carry its row over, or `None` when it can.
pub type Verdict = Option<&'static str>;

/// A grammar with a stable key for each rule: its name, and for a later rule of the
/// same name, the name and how many came before it. `~` keeps these apart from the
/// `#` of anonymous group rules.
struct Side<'a> {
    grammar: &'a Grammar,
    keys: Vec<String>,
    ids: HashMap<String, RuleId>,
}

impl<'a> Side<'a> {
    fn new(grammar: &'a Grammar) -> Self {
        let mut seen: HashMap<&str, usize> = HashMap::default();
        let keys: Vec<String> = grammar
            .rules
            .iter()
            .map(|rule| {
                let before = seen.entry(rule.name.as_str()).or_default();
                *before += 1;
                if *before == 1 {
                    rule.name.clone()
                } else {
                    format!("{}~{}", rule.name, *before - 1)
                }
            })
            .collect();
        let ids = (0u32..)
            .zip(&keys)
            .map(|(id, key)| (key.clone(), id))
            .collect();
        Self { grammar, keys, ids }
    }

    fn key(&self, rule: RuleId) -> &str {
        &self.keys[rule as usize]
    }

    fn id(&self, key: &str) -> Option<RuleId> {
        self.ids.get(key).copied()
    }
}

/// The difference between two grammars, as the reuse checks read it.
pub struct Changes<'a> {
    old: Side<'a>,
    new: Side<'a>,
    strict: HashSet<RuleId>,
    /// What a value needs for each strict rule with a meaning to print it in either
    /// grammar; a print that consulted the rule without such a value is unaffected.
    strict_fits: HashMap<RuleId, Vec<fits::Fit>>,
    zone: HashMap<RuleId, Children>,
    /// What a text must hold for some changed alternative to read part of it, with the
    /// rule that alternative belongs to.
    required: Vec<(Vec<Needs>, String)>,
    vocabulary: Vec<u32>,
    limits: Limits,
    /// Old to new indices of the kept alternatives of each pass-through rule whose
    /// alternatives moved, by the name a derivation key spells.
    renumbered: HashMap<String, Vec<Option<usize>>>,
    /// Names of every other rule that changed, came or went.
    changed_names: HashSet<String>,
    /// New ids of the rules a fresh print may consult where the old one could not.
    fresh: Vec<RuleId>,
    pub summary: Summary,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Summary {
    /// Whether the word types and presentation labels are the same.
    pub tables_unchanged: bool,
    pub changed: usize,
    pub added: usize,
    pub removed: usize,
    pub hooks: usize,
    pub zone: usize,
    pub required: usize,
    pub vocabulary: usize,
}

fn sym_hash(side: &Side<'_>, sym: &Sym, hasher: &mut FxHasher) {
    match sym {
        Sym::Word(word) => (0u8, word).hash(hasher),
        Sym::Mark(mark) => (1u8, mark).hash(hasher),
        Sym::Class(class) => (2u8, class).hash(hasher),
        Sym::Rule(rule) => (3u8, side.key(*rule)).hash(hasher),
    }
}

/// An alternative's symbols and holes, spelled by rule keys.
fn alternative_key(side: &Side<'_>, alternative: &Alternative) -> u64 {
    let mut hasher = FxHasher::default();
    alternative.syms.len().hash(&mut hasher);
    for sym in &alternative.syms {
        sym_hash(side, sym, &mut hasher);
    }
    alternative.holes.hash(&mut hasher);
    hasher.finish()
}

/// An alternative as printing reads it: its symbols and what the word tables say of it.
fn printed_key(side: &Side<'_>, rule: RuleId, index: usize) -> u64 {
    let mut hasher = FxHasher::default();
    let alternative = &side.grammar.rules[rule as usize].alternatives[index];
    alternative_key(side, alternative).hash(&mut hasher);
    side.grammar.words.entries(rule, index).hash(&mut hasher);
    hasher.finish()
}

fn printed_keys(side: &Side<'_>, rule: RuleId) -> Vec<u64> {
    (0..side.grammar.rules[rule as usize].alternatives.len())
        .map(|index| printed_key(side, rule, index))
        .collect()
}

fn reading_keys(side: &Side<'_>, rule: RuleId) -> Vec<u64> {
    side.grammar.rules[rule as usize]
        .alternatives
        .iter()
        .map(|alternative| alternative_key(side, alternative))
        .collect()
}

/// Everything about a rule but its alternatives: meaning, type, preferences and inverses.
fn head_key(grammar: &Grammar, rule: RuleId) -> u64 {
    let definition = &grammar.rules[rule as usize];
    let mut hasher = FxHasher::default();
    format!("{:?}", definition.sem).hash(&mut hasher);
    format!("{:?}", definition.ty).hash(&mut hasher);
    definition.prefer.hash(&mut hasher);
    format!("{:?}", grammar.literal_inverses.get(&rule)).hash(&mut hasher);
    format!("{:?}", grammar.conjunction_inverses.get(&rule)).hash(&mut hasher);
    grammar.words.own_types(rule).hash(&mut hasher);
    hasher.finish()
}

fn reading_key(grammar: &Grammar, rule: RuleId) -> u64 {
    let definition = &grammar.rules[rule as usize];
    let mut hasher = FxHasher::default();
    format!("{:?}", definition.sem).hash(&mut hasher);
    format!("{:?}", definition.ty).hash(&mut hasher);
    hasher.finish()
}

/// Which rules some alternative of the grammar names as a child.
fn referenced(grammar: &Grammar) -> Vec<bool> {
    let mut named = vec![false; grammar.rules.len()];
    for rule in &grammar.rules {
        for alternative in &rule.alternatives {
            for sym in &alternative.syms {
                if let Sym::Rule(child) = sym {
                    named[*child as usize] = true;
                }
            }
        }
    }
    named
}

/// Alternatives of one rule present in both versions, if they keep their order.
fn common(old: &[u64], new: &[u64]) -> Option<(Vec<bool>, Vec<bool>)> {
    let mut count: HashMap<u64, (usize, usize)> = HashMap::default();
    for key in old {
        count.entry(*key).or_default().0 += 1;
    }
    for key in new {
        count.entry(*key).or_default().1 += 1;
    }
    let mark = |keys: &[u64]| {
        let mut left: HashMap<u64, usize> = count
            .iter()
            .map(|(key, (a, b))| (*key, (*a).min(*b)))
            .collect();
        keys.iter()
            .map(|key| {
                left.get_mut(key).is_some_and(|slot| {
                    let kept = *slot > 0;
                    *slot = slot.saturating_sub(1);
                    kept
                })
            })
            .collect::<Vec<bool>>()
    };
    let (kept_old, kept_new) = (mark(old), mark(new));
    let order = |keys: &[u64], kept: &[bool]| {
        keys.iter()
            .zip(kept)
            .filter(|(_, kept)| **kept)
            .map(|(key, _)| *key)
            .collect::<Vec<u64>>()
    };
    (order(old, &kept_old) == order(new, &kept_new)).then_some((kept_old, kept_new))
}

fn first_child(alternative: &Alternative) -> Option<RuleId> {
    match alternative.syms.first() {
        Some(Sym::Rule(child)) => Some(*child),
        _ => None,
    }
}

struct Pair<'s, 'a> {
    old: &'s Side<'a>,
    new: &'s Side<'a>,
    reads_old: Vec<Option<Needs>>,
    reads_new: Vec<Option<Needs>>,
    uses_old: reads::Uses,
    uses_new: reads::Uses,
    /// Rules some alternative of the same grammar refers to. A reading reaches an
    /// added or removed one only through such an alternative, which is itself added,
    /// removed or changed and needs everything the rule needs, so the rule's own
    /// alternatives add no requirement. A rule only the engine enters keeps them.
    referenced_old: Vec<bool>,
    referenced_new: Vec<bool>,
    reaching: HashMap<(bool, RuleId), Option<std::rc::Rc<fits::Seeds>>>,
}

impl Pair<'_, '_> {
    fn require(
        &self,
        required: &mut Vec<(Vec<Needs>, String)>,
        (old, rule): (bool, RuleId),
        alternative: &Alternative,
    ) {
        let (reads, uses, side) = if old {
            (&self.reads_old, &self.uses_old, self.old)
        } else {
            (&self.reads_new, &self.uses_new, self.new)
        };
        if let Some(needs) = reads::requirement(side.grammar, uses, reads, rule, alternative) {
            required.push((needs, side.key(rule).to_string()));
        }
    }
}

/// Rules that differ between the grammars, and the alternatives a reading may miss.
#[derive(Default)]
struct Found {
    strict: HashSet<RuleId>,
    hooks: HashSet<RuleId>,
    renumbered: HashMap<String, Vec<Option<usize>>>,
    changed_names: HashSet<String>,
    changed: HashSet<String>,
    required: Vec<(Vec<Needs>, String)>,
    summary: Summary,
}

impl Found {
    fn compare(&mut self, pair: &Pair<'_, '_>) {
        let (old, new) = (pair.old, pair.new);
        for (id, rule) in (0u32..).zip(&old.grammar.rules) {
            if new.id(old.key(id)).is_some() {
                continue;
            }
            self.summary.removed += 1;
            self.strict.insert(id);
            self.changed.insert(old.key(id).to_string());
            self.changed_names.insert(rule.name.clone());
            if !pair.referenced_old[id as usize] {
                for alternative in &rule.alternatives {
                    pair.require(&mut self.required, (true, id), alternative);
                }
            }
        }
        for (id, rule) in (0u32..).zip(&new.grammar.rules) {
            let Some(before) = old.id(new.key(id)) else {
                self.summary.added += 1;
                self.changed.insert(new.key(id).to_string());
                self.changed_names.insert(rule.name.clone());
                if !pair.referenced_new[id as usize] {
                    for alternative in &rule.alternatives {
                        pair.require(&mut self.required, (false, id), alternative);
                    }
                }
                continue;
            };
            let (old_printed, new_printed) = (printed_keys(old, before), printed_keys(new, id));
            let same_head = head_key(old.grammar, before) == head_key(new.grammar, id);
            if same_head && old_printed == new_printed {
                continue;
            }
            self.summary.changed += 1;
            self.changed.insert(new.key(id).to_string());
            let was = &old.grammar.rules[before as usize];
            self.require_changed(pair, (before, was), (id, rule));
            let kept = common(&old_printed, &new_printed).filter(|(kept_old, kept_new)| {
                was.sem.is_none()
                    && rule.sem.is_none()
                    && same_head
                    && only_children(was, kept_old)
                    && only_children(rule, kept_new)
            });
            let unique = |grammar: &Grammar, name: &str| {
                grammar.names.contains_key(name)
                    && grammar.rules.iter().filter(|r| r.name == name).count() == 1
            };
            match kept {
                Some((kept_old, kept_new))
                    if unique(old.grammar, &was.name) && unique(new.grammar, &rule.name) =>
                {
                    self.hooks.insert(before);
                    self.renumbered
                        .insert(was.name.clone(), renumbering(&kept_old, &kept_new));
                }
                Some(_) => {
                    self.hooks.insert(before);
                    self.changed_names.insert(was.name.clone());
                }
                None => {
                    self.strict.insert(before);
                    self.changed_names.insert(was.name.clone());
                }
            }
        }
    }

    /// The alternatives of a changed rule that a reading under one grammar may take and
    /// under the other may not.
    fn require_changed(
        &mut self,
        pair: &Pair<'_, '_>,
        (before, was): (RuleId, &crate::grammar::Rule),
        (after, now): (RuleId, &crate::grammar::Rule),
    ) {
        let same_reading =
            reading_key(pair.old.grammar, before) == reading_key(pair.new.grammar, after);
        let kept = common(
            &reading_keys(pair.old, before),
            &reading_keys(pair.new, after),
        )
        .filter(|_| same_reading)
        .unwrap_or_else(|| {
            (
                vec![false; was.alternatives.len()],
                vec![false; now.alternatives.len()],
            )
        });
        for (alternative, kept) in was.alternatives.iter().zip(&kept.0) {
            if !kept {
                pair.require(&mut self.required, (true, before), alternative);
            }
        }
        for (alternative, kept) in now.alternatives.iter().zip(&kept.1) {
            if !kept {
                pair.require(&mut self.required, (false, after), alternative);
            }
        }
    }
}

/// Where each kept old alternative sits among the new ones.
fn renumbering(kept_old: &[bool], kept_new: &[bool]) -> Vec<Option<usize>> {
    let mut targets = (0..kept_new.len()).filter(|index| kept_new[*index]);
    kept_old
        .iter()
        .map(|kept| if *kept { targets.next() } else { None })
        .collect()
}

/// Whether every alternative outside the common ones passes its value to a child rule.
fn only_children(rule: &crate::grammar::Rule, kept: &[bool]) -> bool {
    rule.alternatives
        .iter()
        .zip(kept)
        .all(|(alternative, kept)| *kept || first_child(alternative).is_some())
}

impl<'a> Changes<'a> {
    pub fn between(old: &'a Grammar, new: &'a Grammar, limits: Limits) -> Self {
        let (old, new) = (Side::new(old), Side::new(new));
        let summary = Summary {
            tables_unchanged: old.grammar.words.types() == new.grammar.words.types()
                && old.grammar.presentation_labels == new.grammar.presentation_labels,
            ..Summary::default()
        };
        let (strict, strict_fits, zone, required, renumbered, changed_names, summary) = {
            let mut pair = Pair {
                old: &old,
                new: &new,
                reads_old: required_words(old.grammar),
                reads_new: required_words(new.grammar),
                uses_old: reads::Uses::new(old.grammar),
                uses_new: reads::Uses::new(new.grammar),
                referenced_old: referenced(old.grammar),
                referenced_new: referenced(new.grammar),
                reaching: HashMap::default(),
            };
            let mut found = Found {
                summary,
                ..Found::default()
            };
            found.compare(&pair);
            let zone = Self::zone(&mut pair, &found.changed);
            let strict_fits = fits::changed(&pair, &found.strict);
            found.summary.zone = zone.len();
            let Found {
                strict,
                hooks,
                mut required,
                renumbered,
                changed_names,
                mut summary,
                ..
            } = found;
            required.sort_unstable();
            // Keep one entry per requirement, naming every rule it came from.
            required.dedup_by(|later, kept| {
                let same = later.0 == kept.0;
                if same && !kept.1.split('|').any(|name| name == later.1) {
                    kept.1 = format!("{}|{}", kept.1, later.1);
                }
                same
            });
            summary.required = required.len();
            summary.hooks = hooks.len();
            (
                strict,
                strict_fits,
                zone,
                required,
                renumbered,
                changed_names,
                summary,
            )
        };
        let mut summary = summary;
        let mut vocabulary: Vec<u32> = old
            .grammar
            .vocabulary
            .symmetric_difference(&new.grammar.vocabulary)
            .map(|word| word_key(word))
            .collect();
        vocabulary.sort_unstable();
        vocabulary.dedup();
        summary.vocabulary = vocabulary.len();
        let fresh = keys::fresh(&old, &new, &strict, &zone);
        Self {
            old,
            new,
            strict,
            strict_fits,
            zone,
            required,
            vocabulary,
            limits,
            renumbered,
            changed_names,
            fresh,
            summary,
        }
    }

    /// Rules whose pass-through descendants differ in either grammar, with the checks
    /// each needs of the children through which its reach may differ.
    fn zone(pair: &mut Pair<'_, '_>, changed: &HashSet<String>) -> HashMap<RuleId, Children> {
        let mut parents: HashMap<String, Vec<String>> = HashMap::default();
        for side in [pair.old, pair.new] {
            for (id, rule) in (0u32..).zip(&side.grammar.rules) {
                if rule.sem.is_some() {
                    continue;
                }
                for child in rule.alternatives.iter().filter_map(first_child) {
                    parents
                        .entry(side.key(child).to_string())
                        .or_default()
                        .push(side.key(id).to_string());
                }
            }
        }
        let mut zone: HashSet<String> = HashSet::default();
        let mut stack: Vec<String> = changed.iter().cloned().collect();
        while let Some(key) = stack.pop() {
            if !zone.contains(&key) {
                stack.extend(parents.get(&key).into_iter().flatten().cloned());
                zone.insert(key);
            }
        }
        let mut out = HashMap::default();
        for key in &zone {
            let (Some(before), Some(after)) = (pair.old.id(key), pair.new.id(key)) else {
                continue;
            };
            let was = &pair.old.grammar.rules[before as usize];
            let now = &pair.new.grammar.rules[after as usize];
            if was.sem.is_some() || now.sem.is_some() {
                continue;
            }
            let (kept_old, kept_new) = common(
                &printed_keys(pair.old, before),
                &printed_keys(pair.new, after),
            )
            .unwrap_or_else(|| {
                (
                    vec![false; was.alternatives.len()],
                    vec![false; now.alternatives.len()],
                )
            });
            let mut children = Children::default();
            for (alternative, kept) in was.alternatives.iter().zip(&kept_old) {
                if let (false, Some(child)) = (kept, first_child(alternative)) {
                    children.lost.push(child);
                }
            }
            for (alternative, kept) in now.alternatives.iter().zip(&kept_new) {
                let Some(child) = first_child(alternative) else {
                    continue;
                };
                let child_key = pair.new.key(child);
                if *kept && !zone.contains(child_key) {
                    continue;
                }
                let old = pair.old.id(child_key);
                let reach = pair.reaching(child, old.filter(|_| *kept), changed);
                children.open.push(Open {
                    key: child_key.to_string(),
                    old,
                    added: !kept,
                    reach,
                });
            }
            children.removed = pair.lost_under(&children.lost);
            out.insert(before, children);
        }
        fits::fill(&mut out, pair.old.grammar, pair.new.grammar);
        out
    }

    /// Whether a card whose last print left this trace prints and reads the same now.
    pub fn verdict(&self, saved: &Saved) -> Verdict {
        if !self.summary.tables_unchanged {
            return Some("global tables changed");
        }
        if saved.steps > self.limits.steps / 4 || saved.depth > self.limits.depth / 2 {
            return Some("parse headroom");
        }
        if saved
            .vocabulary
            .iter()
            .any(|word| has(&self.vocabulary, *word))
        {
            return Some("vocabulary");
        }
        if self.required_hit(saved).is_some() {
            return Some("a changed alternative can read its text");
        }
        self.zone_hit(saved).map(|(why, _, _)| why)
    }

    /// The consulted rule and child behind a print-side verdict, for diagnosis.
    pub fn zone_cause(&self, saved: &Saved) -> Option<String> {
        self.zone_hit(saved).map(|(_, rule, child)| {
            let rule = self.old.key(rule);
            match child {
                Some(child) => format!("{rule} > {child}"),
                None => rule.to_string(),
            }
        })
    }

    fn zone_hit(&self, saved: &Saved) -> Option<(&'static str, RuleId, Option<&str>)> {
        let consulted = |rule: RuleId| has(&saved.rules, rule);
        for &rule in &saved.rules {
            if self.strict.contains(&rule)
                && self
                    .strict_fits
                    .get(&rule)
                    .is_none_or(|fits| fits.iter().any(|fit| fit.may_fit(saved)))
            {
                return Some(("a consulted rule changed", rule, None));
            }
            let Some(children) = self.zone.get(&rule) else {
                continue;
            };
            let consulted_child = children.lost.iter().any(|child| consulted(*child));
            if !children.lost.is_empty() && children.removed.may_matter(saved, consulted_child) {
                return Some(("a consulted alternative was removed", rule, None));
            }
            for open in &children.open {
                let found = |why| Some((why, rule, Some(open.key.as_str())));
                // A kept child the print consulted is checked as a zone rule of its own.
                // A child under a new alternative reaches every seed anew.
                if !open.added && open.old.is_some_and(consulted) {
                    continue;
                }
                if open.reach.may_fit(saved) {
                    return found(match open.reach {
                        Reaching::Anything => "a changed child takes any value",
                        Reaching::Seeds { .. } => "a changed child can fit a looked-up value",
                    });
                }
            }
        }
        None
    }

    /// The rule of the first changed alternative whose words and adjacent pairs all
    /// occur in the texts a card read.
    pub fn required_hit(&self, saved: &Saved) -> Option<&str> {
        self.required_hits(saved).next()
    }

    /// Every changed alternative's rule a card's texts could read, for diagnosis.
    pub fn required_hits<'s, 't>(
        &'s self,
        saved: &'t Saved,
    ) -> impl Iterator<Item = &'s str> + use<'s, 't, 'a> {
        self.required
            .iter()
            .filter(move |(needs, _)| needs.iter().any(|needs| needs.met(saved)))
            .map(|(_, rule)| rule.as_str())
    }
}

#[path = "print_reuse_fits.rs"]
mod fits;
#[path = "print_reuse_keys.rs"]
mod keys;
#[path = "print_reuse_reads.rs"]
mod reads;
pub(crate) use reads::Needs;
use reads::required_words;

#[cfg(test)]
#[path = "print_reuse_tests.rs"]
mod tests;
