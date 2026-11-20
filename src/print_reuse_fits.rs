//! Which looked-up values a changed child of a pass-through rule could print now and
//! could not before.
//!
//! A child's seeds are the rules with a meaning it reaches through pass-through
//! alternatives. Only a seed that is new, changed, or newly reached through the same
//! alternative can fit a value it did not fit before. Such a seed fits a value only when
//! the value's leading steps follow one of its patterns, and when the value holds every
//! node that every value the seed can print holds: the pattern's own nodes and, through
//! its holes and splices, its children's. The trace keeps the nodes of every looked-up
//! value near its root, so a card none of whose values could fit is not printed again.
//! A transparent symbol matches what its argument matches or a symbol node of its own
//! name, so a node required under one is also met by that symbol node.
use super::{Children, Pair, Saved, Side, first_child, has};
use crate::grammar::{Grammar, RuleId, Sym};
use crate::pattern::Pattern;
use crate::print_trace::pattern_prefix;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::rc::Rc;
use walk::{Feature, Set, Walk, intersect, normal};

#[path = "print_reuse_walk.rs"]
mod walk;

/// The most pass-through descendants searched for the seeds a child could reach, and
/// the most rules whose features one zone computes; more than any grammar has today.
const MOST_DESCENDANTS: usize = 1 << 15;

/// The most rounds the feature fixed point takes before it gives every rule no features.
const MOST_ROUNDS: usize = 256;

/// What a child of a changed pass-through rule could reach under the new grammar that
/// it could not under the old one.
#[derive(Debug, Clone)]
pub(super) enum Reaching {
    /// Every value, or too many seeds to list.
    Anything,
    /// The seeds no longer reached, new ids of the new or changed ones, and what a
    /// value needs to fit each of those.
    Seeds {
        lost: Lost,
        new: Vec<RuleId>,
        fits: Vec<Fit>,
    },
}

impl Reaching {
    /// Whether what the child reaches anew, or no longer reaches, could change a value
    /// this card looked up.
    pub(super) fn may_fit(&self, saved: &Saved) -> bool {
        match self {
            Self::Anything => true,
            Self::Seeds { lost, fits, .. } => {
                fits.iter().any(|fit| fit.may_fit(saved)) || lost.may_matter(saved, false)
            }
        }
    }
}

/// Seeds the old grammar reached that the new one does not, by old id, with what a
/// value needs to fit each under the old grammar; `None` when a search passed the bound.
#[derive(Debug, Clone, Default)]
pub(super) struct Lost {
    seeds: Option<Vec<RuleId>>,
    fits: Vec<Fit>,
}

impl Lost {
    /// A removed candidate changes a print only when the print consulted it, or the
    /// lost alternative's child, and it could fit one of the card's looked-up values.
    pub(super) fn may_matter(&self, saved: &Saved, consulted_child: bool) -> bool {
        let Some(seeds) = &self.seeds else {
            return true;
        };
        (consulted_child || seeds.iter().any(|seed| has(&saved.rules, *seed)))
            && self.fits.iter().any(|fit| fit.may_fit(saved))
    }
}

/// What a looked-up value needs to fit one seed: to fit one of its patterns.
#[derive(Debug, Clone, Default)]
pub(super) struct Fit(Vec<Shape>);

/// What a value fitting one pattern holds.
#[derive(Debug, Clone)]
struct Shape {
    /// The hash of the pattern's leading steps; `None` when it starts with a hole.
    prefix: Option<u32>,
    /// Nodes every value it prints holds, as the trace spells them, each with the symbol
    /// nodes that would meet it instead.
    features: Vec<(u32, Vec<u32>)>,
}

impl Fit {
    pub(super) fn may_fit(&self, saved: &Saved) -> bool {
        let holds = |key: &u32| has(&saved.features, *key);
        self.0.iter().any(|shape| {
            shape
                .prefix
                .is_none_or(|prefix| has(&saved.prefixes, prefix))
                && shape
                    .features
                    .iter()
                    .all(|(feature, unless)| holds(feature) || unless.iter().any(holds))
        })
    }
}

/// The seeds a rule reaches through pass-through alternatives, by key, with their ids.
pub(super) type Seeds = HashMap<String, RuleId>;

impl Pair<'_, '_> {
    /// The old seeds under the children of alternatives the new grammar lost.
    pub(super) fn lost_under(&mut self, children: &[RuleId]) -> Lost {
        let mut seeds = Vec::new();
        for child in children {
            let Some(found) = self.seeds(true, *child) else {
                return Lost::default();
            };
            seeds.extend(found.values());
        }
        seeds.sort_unstable();
        seeds.dedup();
        Lost {
            seeds: Some(seeds),
            fits: Vec::new(),
        }
    }

    /// The seeds a rule reaches in one grammar, or `None` past `MOST_DESCENDANTS`.
    fn seeds(&mut self, old: bool, rule: RuleId) -> Option<Rc<Seeds>> {
        if let Some(found) = self.reaching.get(&(old, rule)) {
            return found.clone();
        }
        let found = search(if old { self.old } else { self.new }, rule).map(Rc::new);
        self.reaching.insert((old, rule), found.clone());
        found
    }

    /// What a zone rule's child reaches now and did not through the same alternative
    /// before (`before` is `None` for a new alternative). A seed reached both times with
    /// its meaning unchanged fits the same values as before, so it cannot change a print.
    pub(super) fn reaching(
        &mut self,
        child: RuleId,
        before: Option<RuleId>,
        changed: &HashSet<String>,
    ) -> Reaching {
        let Some(now) = self.seeds(false, child) else {
            return Reaching::Anything;
        };
        let was = match before.map(|before| self.seeds(true, before)) {
            Some(None) => return Reaching::Anything,
            Some(Some(was)) => was,
            None => Rc::default(),
        };
        let mut new: Vec<RuleId> = now
            .iter()
            .filter(|(key, _)| !was.contains_key(*key) || changed.contains(*key))
            .map(|(_, seed)| *seed)
            .collect();
        let mut lost: Vec<RuleId> = was
            .iter()
            .filter(|(key, _)| !now.contains_key(*key))
            .map(|(_, seed)| *seed)
            .collect();
        new.sort_unstable();
        lost.sort_unstable();
        Reaching::Seeds {
            lost: Lost {
                seeds: Some(lost),
                fits: Vec::new(),
            },
            new,
            fits: Vec::new(),
        }
    }
}

/// Every seed under `start` through pass-through alternatives, by key.
fn search(side: &Side<'_>, start: RuleId) -> Option<Seeds> {
    let grammar = side.grammar;
    let (mut stack, mut seen) = (vec![start], HashSet::default());
    let mut seeds = Seeds::default();
    while let Some(rule) = stack.pop() {
        if !seen.insert(rule) {
            continue;
        }
        if seen.len() > MOST_DESCENDANTS {
            return None;
        }
        let definition = &grammar.rules[rule as usize];
        if definition.sem.is_some() {
            seeds.insert(side.key(rule).to_string(), rule);
        } else {
            stack.extend(definition.alternatives.iter().filter_map(first_child));
        }
    }
    Some(seeds)
}

/// Give every zone rule and open child what a value needs to fit each seed it reaches
/// anew, under the new grammar, and each seed it no longer reaches, under the old one.
pub(super) fn fill(zone: &mut HashMap<RuleId, Children>, old: &Grammar, new: &Grammar) {
    let (mut fresh, mut gone) = (Vec::new(), Vec::new());
    for children in zone.values() {
        gone.extend(children.removed.seeds.iter().flatten());
        for open in &children.open {
            if let Reaching::Seeds { lost, new, .. } = &open.reach {
                fresh.extend(new);
                gone.extend(lost.seeds.iter().flatten());
            }
        }
    }
    let fresh = fits(new, fresh);
    let gone = fits(old, gone);
    let fill_lost = |lost: &mut Lost| {
        if let Some(seeds) = &lost.seeds {
            lost.fits = seeds.iter().map(|seed| gone[seed].clone()).collect();
        }
    };
    for children in zone.values_mut() {
        fill_lost(&mut children.removed);
        for open in &mut children.open {
            if let Reaching::Seeds { lost, new, fits } = &mut open.reach {
                *fits = new.iter().map(|seed| fresh[seed].clone()).collect();
                fill_lost(lost);
            }
        }
    }
}

/// What a value needs for a print to render each changed or removed rule, in either
/// grammar, by old id: to fit the rule, or, for a rule without a telling meaning, one
/// of its owners. A rule left out reprints every print that consulted it.
pub(super) fn changed(pair: &Pair<'_, '_>, strict: &HashSet<RuleId>) -> HashMap<RuleId, Vec<Fit>> {
    let printing = |side: &Side<'_>, uses: &super::reads::Uses, rule: RuleId| {
        if super::reads::telling(&side.grammar.rules[rule as usize]) {
            Some(vec![rule])
        } else {
            uses.owners(side.grammar, rule)
        }
    };
    let wanted: Vec<(RuleId, Vec<RuleId>, Vec<RuleId>)> = strict
        .iter()
        .filter_map(|&before| {
            let old = printing(pair.old, &pair.uses_old, before)?;
            let new = match pair.new.id(pair.old.key(before)) {
                Some(after) => printing(pair.new, &pair.uses_new, after)?,
                None => Vec::new(),
            };
            Some((before, old, new))
        })
        .collect();
    let old = fits(
        pair.old.grammar,
        wanted
            .iter()
            .flat_map(|(_, old, _)| old.iter().copied())
            .collect(),
    );
    let new = fits(
        pair.new.grammar,
        wanted
            .iter()
            .flat_map(|(_, _, new)| new.iter().copied())
            .collect(),
    );
    wanted
        .into_iter()
        .map(|(before, olds, news)| {
            let fits = olds.iter().map(|rule| old[rule].clone());
            (
                before,
                fits.chain(news.iter().map(|rule| new[rule].clone()))
                    .collect(),
            )
        })
        .collect()
}

fn fits(grammar: &Grammar, mut seeds: Vec<RuleId>) -> HashMap<RuleId, Fit> {
    seeds.sort_unstable();
    seeds.dedup();
    let found = required(grammar, &seeds);
    seeds
        .iter()
        .map(|seed| (*seed, fit(grammar, *seed, &found)))
        .collect()
}

/// The patterns a seed prints from: its meaning, filled by any alternative, and the
/// inverses it also prints. A pattern no alternative can fill, such as the meaning of a
/// disabled `never` rule, which has none, prints nothing.
fn fit(grammar: &Grammar, seed: RuleId, found: &HashMap<RuleId, Option<Set>>) -> Fit {
    let Some(pattern) = &grammar.rules[seed as usize].sem else {
        return Fit::default();
    };
    let shape = |pattern: &Pattern, set: Set| Shape {
        prefix: pattern_prefix(pattern),
        features: set.iter().map(Feature::keys).collect(),
    };
    let mut shapes: Vec<Shape> = meaning_features(grammar, seed, pattern, found)
        .map(|set| shape(pattern, set))
        .into_iter()
        .collect();
    for (inverse, holes) in inverses(grammar, seed) {
        shapes.extend(
            inverse_set(grammar, seed, inverse, holes, found).map(|set| shape(inverse, set)),
        );
    }
    Fit(shapes)
}

/// The inverses a rule also prints from, and whether the alternatives fill their holes:
/// a conjunction inverse keeps the meaning's holes, a literal one has none.
fn inverses(grammar: &Grammar, rule: RuleId) -> impl Iterator<Item = (&Pattern, bool)> {
    [
        grammar
            .literal_inverses
            .get(&rule)
            .map(|inverse| (inverse, false)),
        grammar
            .conjunction_inverses
            .get(&rule)
            .map(|inverse| (inverse, true)),
    ]
    .into_iter()
    .flatten()
}

/// What every value an inverse prints holds; `None` while it prints nothing yet.
fn inverse_set(
    grammar: &Grammar,
    rule: RuleId,
    inverse: &Pattern,
    holes: bool,
    found: &HashMap<RuleId, Option<Set>>,
) -> Option<Set> {
    if holes {
        meaning_features(grammar, rule, inverse, found)
    } else {
        Some(inverse_features(inverse, found))
    }
}

/// What every value a rule's meaning prints holds, whichever alternative fills it;
/// `None` while no alternative prints anything yet.
fn meaning_features(
    grammar: &Grammar,
    rule: RuleId,
    pattern: &Pattern,
    found: &HashMap<RuleId, Option<Set>>,
) -> Option<Set> {
    let mut met: Option<Set> = None;
    for alternative in &grammar.rules[rule as usize].alternatives {
        let bound: HashMap<&str, RuleId> = alternative
            .holes
            .iter()
            .zip(&alternative.syms)
            .filter_map(|(hole, sym)| Some((hole.as_deref()?, rule_sym(sym)?)))
            .collect();
        let mut walk = Walk::new(&bound, found);
        if walk.pattern(pattern) {
            met = Some(meet(met, normal(walk.out)));
        }
    }
    met
}

/// An inverse's holes take any value.
fn inverse_features(pattern: &Pattern, found: &HashMap<RuleId, Option<Set>>) -> Set {
    let bound = HashMap::default();
    let mut walk = Walk::new(&bound, found);
    walk.pattern(pattern);
    normal(walk.out)
}

fn meet(met: Option<Set>, set: Set) -> Set {
    match met {
        None => set,
        Some(before) => intersect(&before, &set),
    }
}

/// What every value each rule under `start` prints holds: the greatest fixed point,
/// `None` while a rule prints nothing yet. It is sound by induction on a print's height:
/// the value holds the nodes of the pattern it fit, and each hole's value holds what
/// the child that printed it requires. A rule past `MOST_DESCENDANTS` requires nothing.
fn required(grammar: &Grammar, start: &[RuleId]) -> HashMap<RuleId, Option<Set>> {
    let (mut stack, mut closure) = (start.to_vec(), Vec::new());
    let mut found: HashMap<RuleId, Option<Set>> = HashMap::default();
    while let Some(rule) = stack.pop() {
        if found.len() >= MOST_DESCENDANTS || found.contains_key(&rule) {
            continue;
        }
        found.insert(rule, None);
        closure.push(rule);
        for alternative in &grammar.rules[rule as usize].alternatives {
            stack.extend(alternative.syms.iter().filter_map(rule_sym));
        }
    }
    for _ in 0..MOST_ROUNDS {
        let mut changed = false;
        for &rule in &closure {
            let next = rule_features(grammar, rule, &found);
            if found[&rule] != next {
                found.insert(rule, next);
                changed = true;
            }
        }
        if !changed {
            return found;
        }
    }
    closure
        .iter()
        .map(|rule| (*rule, Some(Set::new())))
        .collect()
}

fn rule_sym(sym: &Sym) -> Option<RuleId> {
    match sym {
        Sym::Rule(child) => Some(*child),
        _ => None,
    }
}

/// What every value `rule` prints holds, from what its children hold now.
fn rule_features(
    grammar: &Grammar,
    rule: RuleId,
    found: &HashMap<RuleId, Option<Set>>,
) -> Option<Set> {
    let definition = &grammar.rules[rule as usize];
    let Some(pattern) = &definition.sem else {
        // A pass-through alternative prints what its one child prints.
        let mut met = None;
        for alternative in &definition.alternatives {
            let children: Vec<RuleId> = alternative.syms.iter().filter_map(rule_sym).collect();
            let set = match children.as_slice() {
                [child] => match found.get(child) {
                    Some(Some(set)) => set.clone(),
                    Some(None) => continue,
                    None => Set::new(),
                },
                _ => Set::new(),
            };
            met = Some(meet(met, set));
        }
        return met;
    };
    let mut met = meaning_features(grammar, rule, pattern, found)?;
    for (inverse, holes) in inverses(grammar, rule) {
        if let Some(set) = inverse_set(grammar, rule, inverse, holes, found) {
            met = intersect(&met, &set);
        }
    }
    Some(met)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::print_trace::{lookup, record};
    use serde_json::{Value, json};

    fn looked_up(value: &Value) -> Saved {
        let ((), trace) = record(|| lookup(value, std::iter::empty()));
        Saved::from_trace(&trace)
    }

    fn fit_of(pattern: &Pattern) -> Fit {
        let set = inverse_features(pattern, &HashMap::default());
        Fit(vec![Shape {
            prefix: None,
            features: set.iter().map(Feature::keys).collect(),
        }])
    }

    /// The fit admits every value the pattern unifies with, and here no other.
    fn assert_fits_as_it_unifies(pattern: &Pattern, values: &[Value]) {
        let fit = fit_of(pattern);
        for value in values {
            let unifies = !pattern
                .unify(value, crate::pattern::Bindings::new())
                .is_empty();
            assert_eq!(fit.may_fit(&looked_up(value)), unifies, "{value}");
        }
    }

    #[test]
    fn a_conjunction_fits_its_conjuncts_at_any_depth() {
        let (a, b) = (
            json!({"kind": "A", "args": 1}),
            json!({"kind": "B", "args": 2}),
        );
        let pattern = Pattern::Symbol(
            "and".into(),
            vec![Pattern::Value(a.clone()), Pattern::Value(b.clone())],
        );
        let mut deep = a.clone();
        for _ in 0..7 {
            deep = json!({"kind": "AllOf", "args": [deep, {"kind": "Any"}]});
        }
        assert_fits_as_it_unifies(
            &pattern,
            &[
                json!({"kind": "AllOf", "args": [b, a]}),
                json!({"kind": "AllOf", "args": [{"kind": "AllOf", "args": [a]}, b]}),
                json!({"kind": "AllOf", "args": [deep, b]}),
                json!({"kind": "AllOf", "args": [a, {"kind": "C", "args": 2}]}),
                a.clone(),
            ],
        );
    }

    #[test]
    fn either_event_fits_both_orders() {
        let (a, b) = (json!({"kind": "A"}), json!({"kind": "B"}));
        let pattern = Pattern::Symbol(
            "either_event".into(),
            vec![Pattern::Value(a.clone()), Pattern::Value(b.clone())],
        );
        assert_fits_as_it_unifies(
            &pattern,
            &[
                json!({"AnyOf": [b, a]}),
                json!({"AnyOf": [a, b]}),
                json!({"AnyOf": [a, {"kind": "C"}]}),
            ],
        );
    }

    #[test]
    fn an_entering_controller_is_also_met_by_the_controller_it_reads() {
        let replacement = |players: Value| json!({"kind": "Replacement", "args": {"active": "SourceEntering", "body": {"players": players}}});
        let pattern = Pattern::Symbol(
            "entering_controller".into(),
            vec![Pattern::Value(replacement(
                json!({"kind": "AbilityController"}),
            ))],
        );
        let fit = fit_of(&pattern);
        let incoming = json!({"kind": "ControllerOf", "args": {"kind": "EventSubject"}});
        assert!(fit.may_fit(&looked_up(&replacement(incoming))));
        assert!(!fit.may_fit(&looked_up(&replacement(json!({"kind": "Opponent"})))));
    }

    #[test]
    fn a_node_under_a_transparent_symbol_is_also_met_by_the_symbol_node() {
        let pattern = Pattern::Symbol(
            "sacrificed_result".into(),
            vec![Pattern::Value(json!({"kind": "Sacrifice", "args": 1}))],
        );
        let set = inverse_features(&pattern, &HashMap::default());
        let fit = Fit(vec![Shape {
            prefix: None,
            features: set.iter().map(Feature::keys).collect(),
        }]);
        assert!(fit.may_fit(&looked_up(&json!({"kind": "Sacrifice", "args": 1}))));
        assert!(!fit.may_fit(&looked_up(&json!({"kind": "Sacrifice", "args": 2}))));
        let symbol = json!({"$sym": "sacrificed_result", "args": [{"kind": "Exile"}]});
        assert!(fit.may_fit(&looked_up(&symbol)));
    }
}
