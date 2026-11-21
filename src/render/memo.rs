//! Within-face memoization shared across default, round-trip, productions,
//! widened, and keyed prints of the same face within a single trace scope.
//!
//! Only cycle-free subtrees (`min_cycle == usize::MAX`) are stored. Each entry
//! records the exact sorted set of rules consulted (with a 64-bit bloom prefilter),
//! whether any consulted rule has a native literal/conjunction inverse, and the
//! beam-cap range `[cap_min, cap_max]` over which its derivations are exact.
//! A later print reuses an entry when its cap fits, its native mode fits, none of
//! the consulted rules are keyed, and none are currently open on the recursion stack.
use crate::derivation::Derivation;
use crate::grammar::RuleId;
use crate::heads::Values;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::rc::Rc;

pub(crate) type Found = Rc<Vec<Derivation>>;

/// Cap on tracked consulted rules per cached subtree; larger roots skip key-disjoint
/// and nested-stack sharing while remaining shareable at the top level when unkeyed.
pub(crate) const MAX_CONSULTED: usize = 256;

#[derive(Clone)]
pub(crate) struct SharedEntry {
    pub(crate) found: Found,
    pub(crate) truncated: bool,
    /// Minimum beam `cap` for which `found` is exact.
    pub(crate) cap_min: usize,
    /// Maximum beam `cap` (`None` when untruncated -> valid for all `cap >= cap_min`).
    pub(crate) cap_max: Option<usize>,
    /// `None` when no consulted rule has a native inverse (valid for both `native` modes).
    pub(crate) native: Option<bool>,
    /// 64-bit bloom filter of `consulted` rule IDs.
    pub(crate) bloom: u64,
    /// Exact sorted consulted rule IDs (when `<= MAX_CONSULTED`); `None` means overflowed.
    pub(crate) consulted: Option<Rc<[RuleId]>>,
}

impl SharedEntry {
    #[inline]
    pub(crate) fn fits_cap(&self, cap: usize) -> bool {
        cap >= self.cap_min && self.cap_max.is_none_or(|max| cap <= max)
    }

    #[inline]
    pub(crate) fn fits_native(&self, native: bool) -> bool {
        self.native.is_none_or(|n| n == native)
    }

    #[inline]
    pub(crate) fn disjoint_from(&self, key_bloom: u64, keyed_rules: &HashSet<RuleId>) -> bool {
        if keyed_rules.is_empty() {
            return true;
        }
        let Some(rules) = &self.consulted else {
            return false;
        };
        if (self.bloom & key_bloom) == 0 {
            return true;
        }
        rules.iter().all(|r| !keyed_rules.contains(r))
    }

    #[inline]
    pub(crate) fn disjoint_from_open(&self, open_bloom: u64, open_stack: &[RuleId]) -> bool {
        if open_stack.is_empty() {
            return true;
        }
        let Some(rules) = &self.consulted else {
            return false;
        };
        if (self.bloom & open_bloom) == 0 {
            return true;
        }
        open_stack.iter().all(|r| rules.binary_search(r).is_err())
    }
}

/// Per-face cache shared across repeated prints of one card face within a single
/// `print_trace::record` scope.
#[derive(Default)]
pub struct FaceMemo {
    pub(crate) values: Values,
    pub(crate) contexts: Vec<Vec<String>>,
    pub(crate) best: Vec<u32>,
    pub(crate) dyn_words: Vec<String>,
    pub(crate) dyn_ids: HashMap<String, u32>,
    /// Keyed by `(rule, value_id, context_id)`.
    pub(crate) unkeyed: HashMap<(RuleId, u32, usize), SharedEntry>,
    pub(crate) parse: crate::parse::ParseMemo,
    pub(crate) card_name: Option<String>,
    pub(crate) self_name: u8,
}

impl FaceMemo {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn align(&mut self, card_name: Option<&str>, self_name: u8) {
        if self.card_name.as_deref() != card_name || self.self_name != self_name {
            self.unkeyed.clear();
            if self.card_name.as_deref() != card_name {
                self.parse = crate::parse::ParseMemo::default();
            }
            self.card_name = card_name.map(str::to_string);
            self.self_name = self_name;
        }
    }
}

#[inline]
pub(crate) fn rule_bit(rule: RuleId) -> u64 {
    1u64 << (rule & 63)
}
