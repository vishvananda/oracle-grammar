//! Within-face parse and bind memoization shared across repeated reparses of
//! candidate texts for the same card face (such as `verified_whole` and
//! `check_round_trip`).
//!
//! Each cached rule entry records the exact starting offset `at` and the exact
//! observed token window `[at..end_ex)` (plus whether EOF was inspected),
//! referencing a stored token buffer by index so lookups compare exact tokens
//! without copying. Exact `at` guarantees every `{"$sym": ..., "at": at}` node
//! is identical with zero rewriting. Conservative step and relative-depth bounds
//! preserve resource limits and `print_trace::parse_work` safety headroom.
//!
//! Per-ability binding and post-bind validation/canonicalization caches collapse
//! Cartesian products of ambiguous multi-ability parses and share bound results
//! across candidate wordings whose lexical `at` offsets have already been bound away.
use super::Readings;
use crate::grammar::RuleId;
use crate::tokens::Token;
use rustc_hash::FxHashMap as HashMap;
use serde_json::Value;
use std::rc::Rc;

const MAX_PER_KEY: usize = 24;
const MAX_TOTAL_ENTRIES: usize = 16_384;
const MAX_ABILITY_ENTRIES: usize = 2048;
const MAX_CANONICAL_ENTRIES: usize = 512;

#[derive(Clone)]
pub(crate) struct CachedRule {
    pub(crate) text_id: u32,
    pub(crate) end_ex: u32,
    pub(crate) read_eof: bool,
    pub(crate) readings: Readings,
    pub(crate) steps: u64,
    pub(crate) rel_depth: u16,
    pub(crate) furthest: u32,
}

type AbilityCacheEntry = (Value, Result<Rc<Value>, String>);
type CanonicalCacheEntry = (Value, Result<Value, String>);

#[derive(Default)]
pub struct ParseMemo {
    card_name: Option<String>,
    pub(crate) texts: Vec<Vec<Token>>,
    /// Keyed by `(u64::from(rule) << 32) | (at as u64)`.
    pub(crate) entries: HashMap<u64, Vec<CachedRule>>,
    rule_bloom: u64,
    at_bloom: u64,
    pub(crate) total_entries: usize,
    /// Cached per-ability `bind::bind_held_ability(item, card_name, attachment)` results.
    abilities: HashMap<(u128, u8), Vec<AbilityCacheEntry>>,
    ability_count: usize,
    /// Cached post-`bind_surface` validate + canonicalize results (independent of `at`).
    canonical: HashMap<u128, Vec<CanonicalCacheEntry>>,
    canonical_count: usize,
}

#[inline]
pub(crate) fn shared_key(rule: RuleId, at: usize) -> u64 {
    (u64::from(rule) << 32) | (at as u64)
}

#[inline]
fn bit(v: usize) -> u64 {
    1u64 << (v & 63)
}

impl ParseMemo {
    pub(crate) fn align(&mut self, name: &str) {
        if self.card_name.as_deref() != Some(name) {
            self.texts.clear();
            self.entries.clear();
            self.rule_bloom = 0;
            self.at_bloom = 0;
            self.total_entries = 0;
            self.abilities.clear();
            self.ability_count = 0;
            self.canonical.clear();
            self.canonical_count = 0;
            self.card_name = Some(name.to_string());
        }
    }

    #[inline]
    pub(crate) fn lookup(
        &self,
        rule: RuleId,
        at: usize,
        tokens: &[Token],
        budget_steps: u64,
        budget_depth: usize,
    ) -> Option<&CachedRule> {
        if (self.rule_bloom & bit(rule as usize)) == 0 || (self.at_bloom & bit(at)) == 0 {
            return None;
        }
        let list = self.entries.get(&shared_key(rule, at))?;
        for entry in list {
            let end_ex = entry.end_ex as usize;
            if end_ex > tokens.len()
                || (entry.read_eof && end_ex != tokens.len())
                || entry.steps > budget_steps
                || usize::from(entry.rel_depth) > budget_depth
            {
                continue;
            }
            let Some(prev) = self.texts.get(entry.text_id as usize) else {
                continue;
            };
            if end_ex > at && prev[end_ex - 1] != tokens[end_ex - 1] {
                continue;
            }
            if tokens[at..end_ex] == prev[at..end_ex] {
                return Some(entry);
            }
        }
        None
    }

    pub(crate) fn insert(
        &mut self,
        rule: RuleId,
        at: usize,
        tokens: &[Token],
        entry: CachedRule,
    ) -> bool {
        if self.total_entries >= MAX_TOTAL_ENTRIES {
            return false;
        }
        let list = self.entries.entry(shared_key(rule, at)).or_default();
        if list.len() >= MAX_PER_KEY {
            return false;
        }
        let end_ex = entry.end_ex as usize;
        for existing in list.iter() {
            if existing.end_ex == entry.end_ex
                && existing.read_eof == entry.read_eof
                && let Some(prev) = self.texts.get(existing.text_id as usize)
                && tokens[at..end_ex] == prev[at..end_ex]
            {
                return false;
            }
        }
        self.rule_bloom |= bit(rule as usize);
        self.at_bloom |= bit(at);
        list.push(entry);
        self.total_entries += 1;
        true
    }

    pub(crate) fn get_ability(
        &self,
        digest: u128,
        attachment: u8,
        value: &Value,
    ) -> Option<&Result<Rc<Value>, String>> {
        self.abilities
            .get(&(digest, attachment))?
            .iter()
            .find_map(|(k, res)| (k == value).then_some(res))
    }

    pub(crate) fn insert_ability(
        &mut self,
        digest: u128,
        attachment: u8,
        value: Value,
        res: Result<Rc<Value>, String>,
    ) {
        if self.ability_count >= MAX_ABILITY_ENTRIES {
            return;
        }
        let list = self.abilities.entry((digest, attachment)).or_default();
        if list.len() < 4 && !list.iter().any(|(k, _)| k == &value) {
            list.push((value, res));
            self.ability_count += 1;
        }
    }

    pub(crate) fn get_canonical(
        &self,
        digest: u128,
        value: &Value,
    ) -> Option<&Result<Value, String>> {
        self.canonical
            .get(&digest)?
            .iter()
            .find_map(|(k, res)| (k == value).then_some(res))
    }

    pub(crate) fn insert_canonical(
        &mut self,
        digest: u128,
        value: Value,
        res: Result<Value, String>,
    ) {
        if self.canonical_count >= MAX_CANONICAL_ENTRIES {
            return;
        }
        let list = self.canonical.entry(digest).or_default();
        if list.len() < 4 && !list.iter().any(|(k, _)| k == &value) {
            list.push((value, res));
            self.canonical_count += 1;
        }
    }
}
