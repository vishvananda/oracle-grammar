//! What a text must hold for a changed alternative to read part of it.
//!
//! A reading through an alternative is one derivation of the card's text from a rule
//! the engine enters. Unless the alternative's rule is one of those, the derivation
//! uses it at some place the grammar names it, so the text holds the words around that
//! place too; and it holds one derivation of each child, so a child whose derivations
//! share no word can be spelled out as its alternatives. Each way is a sentential form,
//! and a text that meets the words and pairs of none of them cannot be read through
//! the alternative.
use super::{Saved, has};
use crate::grammar::{Alternative, Grammar, Rule, RuleId, Sym};
use crate::pattern::Pattern;
use crate::print_trace::{literal_key, pair_key, word_key};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use serde_json::Value;
use std::cmp::Reverse;

/// The rules the engine parses a whole text from, besides those no alternative names.
const ENTRIES: &[&str] = &["card", "card_text"];
/// The most forms one requirement keeps; past it, the forms found so far stand.
const MOST_FORMS: usize = 512;
/// How many pass-through uses the search climbs to find words around a rule.
const MOST_CLIMB: usize = 4;
/// How many times one requirement spells a child out as its alternatives.
const MOST_STEPS: usize = 256;
/// A child whose derivations share at most this many words is spelled out.
const WEAK: usize = 1;
/// The most rules the search for a rule's owners visits.
const MOST_OWNERS: usize = 4096;

/// Where each rule of one grammar is named, and which rules a parse may start from.
pub(super) struct Uses {
    sites: Vec<Vec<(RuleId, usize, usize)>>,
    entries: Vec<bool>,
}

impl Uses {
    pub(super) fn new(grammar: &Grammar) -> Self {
        let mut sites = vec![Vec::new(); grammar.rules.len()];
        for (rule, definition) in (0u32..).zip(&grammar.rules) {
            for (index, alternative) in definition.alternatives.iter().enumerate() {
                for (at, sym) in alternative.syms.iter().enumerate() {
                    if let Sym::Rule(child) = sym {
                        sites[*child as usize].push((rule, index, at));
                    }
                }
            }
        }
        let mut entries: Vec<bool> = sites.iter().map(Vec::is_empty).collect();
        for name in ENTRIES {
            if let Some(id) = grammar.names.get(*name) {
                entries[*id as usize] = true;
            }
        }
        Self { sites, entries }
    }

    /// The rules whose prints render `rule`: the nearest rule with a telling meaning on
    /// every way up through the places that name it. The scoreboard prints from `card`
    /// and `card_text`, and renders any other rule only as a child of an alternative it
    /// renders. `None` when the way up meets a rule a print or parse may start from, or
    /// passes `MOST_OWNERS` rules.
    pub(super) fn owners(&self, grammar: &Grammar, rule: RuleId) -> Option<Vec<RuleId>> {
        let (mut stack, mut seen, mut out) = (vec![rule], HashSet::default(), Vec::new());
        while let Some(at) = stack.pop() {
            if !seen.insert(at) {
                continue;
            }
            if seen.len() > MOST_OWNERS || self.entries[at as usize] {
                return None;
            }
            for &(parent, _, _) in &self.sites[at as usize] {
                if telling(&grammar.rules[parent as usize]) {
                    out.push(parent);
                } else {
                    stack.push(parent);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        Some(out)
    }
}

/// Whether a rule's meaning says something of the values it prints. A child its
/// alternative binds to nothing is printed from null, which most prints look up, so
/// a meaning of `()` fits nearly every print.
pub(super) fn telling(rule: &Rule) -> bool {
    rule.sem
        .as_ref()
        .is_some_and(|sem| !matches!(sem, Pattern::Value(Value::Null)))
}

/// The words and pairs of each form a text may hold where `alternative` of `rule`
/// reads part of it; `None` when the alternative derives nothing.
pub(super) fn requirement(
    grammar: &Grammar,
    uses: &Uses,
    reads: &[Option<Needs>],
    rule: RuleId,
    alternative: &Alternative,
) -> Option<Vec<Needs>> {
    let own = alternative.syms.clone();
    let mut forms = Vec::new();
    if !climb(grammar, uses, rule, &own, &mut vec![rule], &mut forms) {
        forms = vec![own];
    }
    let mut out: Vec<Needs> = expand(grammar, reads, forms)
        .iter()
        .filter_map(|form| sym_words(form, reads))
        .collect();
    out.sort_unstable();
    out.dedup();
    (!out.is_empty()).then_some(out)
}

/// The form inside each place the grammar names `rule`, climbing through uses that add
/// no symbols. `false` when the forms pass the bound.
fn climb(
    grammar: &Grammar,
    uses: &Uses,
    rule: RuleId,
    form: &[Sym],
    path: &mut Vec<RuleId>,
    out: &mut Vec<Vec<Sym>>,
) -> bool {
    if uses.entries[rule as usize] {
        out.push(form.to_vec());
    }
    for &(parent, index, at) in &uses.sites[rule as usize] {
        if out.len() >= MOST_FORMS {
            return false;
        }
        let syms = &grammar.rules[parent as usize].alternatives[index].syms;
        let mut wider = Vec::with_capacity(syms.len() + form.len());
        wider.extend_from_slice(&syms[..at]);
        wider.extend_from_slice(form);
        wider.extend_from_slice(&syms[at + 1..]);
        if syms.len() == 1 && path.len() < MOST_CLIMB && !path.contains(&parent) {
            path.push(parent);
            let within = climb(grammar, uses, parent, &wider, path, out);
            path.pop();
            if !within {
                return false;
            }
        } else {
            out.push(wider);
        }
    }
    out.len() <= MOST_FORMS
}

/// Spell out the form with the fewest words, at its most telling weak child, while the
/// forms fit the bound: a text reads through the form only by one of the child's
/// alternatives. The most telling child is the one whose alternatives' fewest words
/// are the most. A form that derives nothing is dropped.
fn expand(grammar: &Grammar, reads: &[Option<Needs>], forms: Vec<Vec<Sym>>) -> Vec<Vec<Sym>> {
    let strength = |form: &[Sym]| sym_words(form, reads).map(|needs| needs.words.len());
    let mut gains: HashMap<RuleId, usize> = HashMap::default();
    let mut forms: Vec<Form> = forms
        .into_iter()
        .filter_map(|syms| {
            Some(Form {
                words: strength(&syms)?,
                done: false,
                syms,
            })
        })
        .collect();
    for _ in 0..MOST_STEPS {
        let Some(index) = (0..forms.len())
            .filter(|index| !forms[*index].done)
            .min_by_key(|index| forms[*index].words)
        else {
            break;
        };
        let room = MOST_FORMS.saturating_sub(forms.len() - 1);
        let Some((at, child)) = spelled_child(grammar, reads, &mut gains, &forms[index].syms, room)
        else {
            forms[index].done = true;
            continue;
        };
        let form = forms.swap_remove(index).syms;
        for alternative in &grammar.rules[child as usize].alternatives {
            let mut syms = Vec::with_capacity(form.len() + alternative.syms.len());
            syms.extend_from_slice(&form[..at]);
            syms.extend_from_slice(&alternative.syms);
            syms.extend_from_slice(&form[at + 1..]);
            if let Some(words) = strength(&syms) {
                forms.push(Form {
                    words,
                    done: false,
                    syms,
                });
            }
        }
    }
    forms.into_iter().map(|form| form.syms).collect()
}

struct Form {
    syms: Vec<Sym>,
    words: usize,
    done: bool,
}

/// The weak child of a form to spell out, by place and id, if one has at most `room`
/// alternatives: the most telling, then the one with fewer alternatives, then the first.
fn spelled_child(
    grammar: &Grammar,
    reads: &[Option<Needs>],
    gains: &mut HashMap<RuleId, usize>,
    form: &[Sym],
    room: usize,
) -> Option<(usize, RuleId)> {
    let mut best: Option<((usize, Reverse<usize>), usize, RuleId)> = None;
    for (at, sym) in form.iter().enumerate() {
        let Sym::Rule(child) = sym else { continue };
        let Some(own) = reads[*child as usize]
            .as_ref()
            .map(|needs| needs.words.len())
        else {
            continue;
        };
        let alternatives = &grammar.rules[*child as usize].alternatives;
        if own > WEAK || alternatives.len() > room {
            continue;
        }
        let gain = *gains.entry(*child).or_insert_with(|| {
            alternatives
                .iter()
                .filter_map(|alternative| sym_words(&alternative.syms, reads))
                .map(|needs| needs.words.len())
                .min()
                .unwrap_or(0)
        });
        let rank = (gain, Reverse(alternatives.len()));
        if best.as_ref().is_none_or(|(most, _, _)| rank > *most) {
            best = Some((rank, at, *child));
        }
    }
    best.map(|(_, at, child)| (at, child))
}

/// The words, and the pairs of adjacent tokens, that every reading through a rule or
/// alternative spans.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Needs {
    pub(crate) words: Vec<u32>,
    pub(crate) pairs: Vec<u32>,
}

impl Needs {
    pub(super) fn shared(self, other: &Self) -> Self {
        Self {
            words: self
                .words
                .into_iter()
                .filter(|w| has(&other.words, *w))
                .collect(),
            pairs: self
                .pairs
                .into_iter()
                .filter(|p| has(&other.pairs, *p))
                .collect(),
        }
    }

    pub(super) fn met(&self, saved: &Saved) -> bool {
        self.words.iter().all(|word| has(&saved.words, *word))
            && self.pairs.iter().all(|pair| has(&saved.pairs, *pair))
    }
}

/// Words every derivation of each rule must contain; `None` when the rule derives nothing.
/// The greatest fixed point is sound: by induction on a derivation's height, its yield
/// holds every word its alternative's symbols require.
pub(super) fn required_words(grammar: &Grammar) -> Vec<Option<Needs>> {
    let mut required: Vec<Option<Needs>> = vec![None; grammar.rules.len()];
    loop {
        let mut changed = false;
        for rule in 0..grammar.rules.len() {
            let mut met: Option<Needs> = None;
            for alternative in &grammar.rules[rule].alternatives {
                let Some(needs) = alternative_words(alternative, &required) else {
                    continue;
                };
                met = Some(match met {
                    None => needs,
                    Some(before) => before.shared(&needs),
                });
            }
            if met != required[rule] {
                required[rule] = met;
                changed = true;
            }
        }
        if !changed {
            return required;
        }
    }
}

/// The words an alternative requires, or `None` if a symbol derives nothing yet.
fn alternative_words(alternative: &Alternative, required: &[Option<Needs>]) -> Option<Needs> {
    sym_words(&alternative.syms, required)
}

/// The words a sentential form requires, or `None` if a symbol derives nothing yet.
fn sym_words(syms: &[Sym], required: &[Option<Needs>]) -> Option<Needs> {
    let mut needs = Needs::default();
    let mut before = None;
    for sym in syms {
        let key = match sym {
            Sym::Word(word) => literal_key(word),
            Sym::Mark(mark) => Some(word_key(mark.encode_utf8(&mut [0; 4]))),
            Sym::Class(_) => None,
            Sym::Rule(child) => {
                let child = required[*child as usize].as_ref()?;
                needs.words.extend(&child.words);
                needs.pairs.extend(&child.pairs);
                None
            }
        };
        if let (Some(first), Some(second)) = (before, key) {
            needs.pairs.push(pair_key(first, second));
        }
        needs.words.extend(key);
        before = key;
    }
    for list in [&mut needs.words, &mut needs.pairs] {
        list.sort_unstable();
        list.dedup();
    }
    Some(needs)
}
