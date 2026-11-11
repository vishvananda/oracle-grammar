//! Which rules can print a value, found from the value. Every `@sem` pattern sits in one
//! discrimination tree, the term index theorem provers use, where a hole matches any
//! subtree. A lookup returns a superset of the patterns that unify, and walking up from
//! those through the pass-through rules gives the alternatives worth trying. Work then
//! follows the matches and not the size of the grammar. An alternative left out could
//! only have failed, so leaving it out changes no derivation and no order.
use crate::grammar::{Rule, RuleId, Sym};
use crate::pattern::{Item, Pattern};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use serde_json::Value;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// A pattern's path stops here, and whatever is left is for `unify` to decide. Sharing a
/// long prefix is rare enough that this bounds the tree without widening lookups.
const MOST_STEPS: usize = 192;
/// Tags in the low bits of a step.
const TAG_BITS: u32 = 3;
const STRING: u32 = 0;
const KEY: u32 = 1;
const OBJECT: u32 = 2;
const LIST: u32 = 3;
const NUMBER: u32 = 4;
const OTHER: u32 = 5;
/// A step no pattern takes.
const UNSEEN: u32 = u32::MAX;
const NONE: u32 = u32::MAX;

/// What walking up from some rules reaches.
#[derive(Debug, Default)]
pub struct Reach {
    /// The rules whose own pattern can fit, before walking up.
    pub fits: Vec<RuleId>,
    rules: HashSet<RuleId>,
    /// The alternatives of each pass-through rule reached, in grammar order.
    alternatives: HashMap<RuleId, Vec<u32>>,
}

impl Reach {
    fn visit(&mut self, start: Vec<RuleId>, parents: &[Vec<(RuleId, u32)>]) -> Vec<RuleId> {
        let mut stack = start;
        let mut touched = Vec::new();
        while let Some(rule) = stack.pop() {
            if !self.rules.insert(rule) {
                continue;
            }
            for (parent, alternative) in &parents[rule as usize] {
                self.alternatives
                    .entry(*parent)
                    .or_default()
                    .push(*alternative);
                touched.push(*parent);
                stack.push(*parent);
            }
        }
        touched
    }
}

/// The discrimination tree, and the edges to walk up.
#[derive(Debug, Default)]
pub struct Net {
    strings: HashMap<String, u32>,
    numbers: HashMap<String, u32>,
    edges: HashMap<(u32, u32), u32>,
    /// For each node, where a hole leads.
    holes: Vec<u32>,
    /// The rules whose path ends at a node.
    ends: HashMap<u32, Vec<RuleId>>,
    /// For each rule, the pass-through alternatives that start with it.
    parents: Vec<Vec<(RuleId, u32)>>,
    /// What every value reaches: rules that take any shape, and their parents.
    always: Reach,
}

/// One step of a path through a value.
enum Step {
    Exact(u32),
    Hole,
}

fn tagged(tag: u32, payload: u32) -> u32 {
    payload << TAG_BITS | tag
}

fn other(value: &Value) -> u32 {
    tagged(
        OTHER,
        match value {
            Value::Null => 0,
            Value::Bool(false) => 1,
            _ => 2,
        },
    )
}

fn count(items: usize) -> u32 {
    u32::try_from(items).unwrap_or(u32::MAX >> TAG_BITS)
}

/// A value in preorder, with where each subtree ends so a hole can skip it.
#[derive(Debug, Default)]
struct Flat {
    steps: Vec<u32>,
    after: Vec<u32>,
}

impl Net {
    fn intern(table: &mut HashMap<String, u32>, text: &str) -> u32 {
        if let Some(id) = table.get(text) {
            return *id;
        }
        let id = count(table.len());
        table.insert(text.to_string(), id);
        id
    }

    fn value_path(&mut self, value: &Value, out: &mut Vec<Step>) {
        match value {
            Value::String(text) => {
                let id = Self::intern(&mut self.strings, text);
                out.push(Step::Exact(tagged(STRING, id)));
            }
            Value::Number(number) => {
                let id = Self::intern(&mut self.numbers, &number.to_string());
                out.push(Step::Exact(tagged(NUMBER, id)));
            }
            Value::Array(items) => {
                out.push(Step::Exact(tagged(LIST, count(items.len()))));
                items.iter().for_each(|item| self.value_path(item, out));
            }
            Value::Object(object) => {
                out.push(Step::Exact(tagged(OBJECT, count(object.len()))));
                let mut fields: Vec<(&String, &Value)> = object.iter().collect();
                fields.sort_unstable_by_key(|(name, _)| *name);
                for (name, item) in fields {
                    let id = Self::intern(&mut self.strings, name);
                    out.push(Step::Exact(tagged(KEY, id)));
                    self.value_path(item, out);
                }
            }
            scalar => out.push(Step::Exact(other(scalar))),
        }
    }

    fn pattern_path(&mut self, pattern: &Pattern, out: &mut Vec<Step>) {
        match pattern {
            Pattern::Value(value) => self.value_path(value, out),
            Pattern::Hole(..) | Pattern::Symbol(..) => out.push(Step::Hole),
            Pattern::Object(fields) => {
                out.push(Step::Exact(tagged(OBJECT, count(fields.len()))));
                let mut fields: Vec<&(String, Pattern)> = fields.iter().collect();
                fields.sort_unstable_by_key(|(name, _)| name);
                for (name, inner) in fields {
                    let id = Self::intern(&mut self.strings, name);
                    out.push(Step::Exact(tagged(KEY, id)));
                    self.pattern_path(inner, out);
                }
            }
            Pattern::List(pieces) => {
                let exact = pieces.iter().all(|piece| matches!(piece, Item::One(_)));
                if !exact {
                    out.push(Step::Hole);
                    return;
                }
                out.push(Step::Exact(tagged(LIST, count(pieces.len()))));
                for piece in pieces {
                    if let Item::One(inner) = piece {
                        self.pattern_path(inner, out);
                    }
                }
            }
        }
    }

    fn insert(&mut self, rule: RuleId, path: &[Step]) {
        let mut node = 0;
        for step in path.iter().take(MOST_STEPS) {
            node = match step {
                Step::Hole => {
                    if self.holes[node as usize] == NONE {
                        self.holes[node as usize] = count(self.holes.len());
                        self.holes.push(NONE);
                    }
                    self.holes[node as usize]
                }
                Step::Exact(exact) => match self.edges.get(&(node, *exact)) {
                    Some(next) => *next,
                    None => {
                        let next = count(self.holes.len());
                        self.holes.push(NONE);
                        self.edges.insert((node, *exact), next);
                        next
                    }
                },
            };
        }
        self.ends.entry(node).or_default().push(rule);
    }

    pub fn build(rules: &[Rule]) -> Self {
        let mut net = Self {
            holes: vec![NONE],
            parents: vec![Vec::new(); rules.len()],
            ..Self::default()
        };
        let mut seeds = Vec::new();
        for (id, rule) in (0u32..).zip(rules) {
            match &rule.sem {
                Some(pattern) => {
                    let mut path = Vec::new();
                    net.pattern_path(pattern, &mut path);
                    if matches!(path.first(), Some(Step::Hole)) {
                        seeds.push(id);
                    } else {
                        net.insert(id, &path);
                    }
                }
                None => {
                    for (index, alternative) in (0u32..).zip(&rule.alternatives) {
                        match alternative.syms.first() {
                            Some(Sym::Rule(child)) => {
                                net.parents[*child as usize].push((id, index));
                            }
                            Some(_) => {
                                net.always.alternatives.entry(id).or_default().push(index);
                                seeds.push(id);
                            }
                            None => {}
                        }
                    }
                }
            }
        }
        let parents = std::mem::take(&mut net.parents);
        net.always.visit(seeds, &parents);
        net.parents = parents;
        for list in net.always.alternatives.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        net
    }

    fn flatten(&self, value: &Value, flat: &mut Flat) {
        let at = flat.steps.len();
        flat.steps.push(UNSEEN);
        flat.after.push(0);
        match value {
            Value::String(text) => {
                if let Some(id) = self.strings.get(text.as_str()) {
                    flat.steps[at] = tagged(STRING, *id);
                }
            }
            Value::Number(number) => {
                if let Some(id) = self.numbers.get(&number.to_string()) {
                    flat.steps[at] = tagged(NUMBER, *id);
                }
            }
            Value::Array(items) => {
                flat.steps[at] = tagged(LIST, count(items.len()));
                items.iter().for_each(|item| self.flatten(item, flat));
            }
            Value::Object(object) => {
                flat.steps[at] = tagged(OBJECT, count(object.len()));
                let mut fields: Vec<(&String, &Value)> = object.iter().collect();
                fields.sort_unstable_by_key(|(name, _)| *name);
                for (name, item) in fields {
                    let key = self.strings.get(name.as_str());
                    let step = key.map_or(UNSEEN, |id| tagged(KEY, *id));
                    flat.steps.push(step);
                    flat.after.push(count(flat.steps.len()));
                    self.flatten(item, flat);
                }
            }
            scalar => flat.steps[at] = other(scalar),
        }
        flat.after[at] = count(flat.steps.len());
    }

    fn walk(&self, node: u32, at: usize, flat: &Flat, found: &mut Vec<RuleId>) {
        if let Some(rules) = self.ends.get(&node) {
            found.extend_from_slice(rules);
        }
        let Some(step) = flat.steps.get(at) else {
            return;
        };
        let hole = self.holes[node as usize];
        if hole != NONE {
            self.walk(hole, flat.after[at] as usize, flat, found);
        }
        if let Some(next) = self.edges.get(&(node, *step)) {
            self.walk(*next, at + 1, flat, found);
        }
    }

    /// The rules that can print `value`, and the alternatives that lead to them.
    fn reach(&self, value: &Value) -> Reach {
        let mut flat = Flat::default();
        self.flatten(value, &mut flat);
        let mut found = Vec::new();
        self.walk(0, 0, &flat, &mut found);
        let fits = found.clone();
        found.retain(|rule| !self.always.rules.contains(rule));
        let mut reach = Reach {
            fits,
            ..Reach::default()
        };
        let touched = reach.visit(found, &self.parents);
        for rule in touched {
            let fixed = self.always.alternatives.get(&rule);
            if let Some(list) = reach.alternatives.get_mut(&rule) {
                list.extend(fixed.into_iter().flatten().copied());
                list.sort_unstable();
                list.dedup();
            }
        }
        reach
    }

    /// Whether `rule` can print a value with this reach.
    pub fn reaches(&self, reach: &Reach, rule: RuleId) -> bool {
        reach.rules.contains(&rule) || self.always.rules.contains(&rule)
    }

    /// The alternatives of a pass-through rule worth trying, in grammar order.
    pub fn alternatives<'a>(&'a self, reach: &'a Reach, rule: RuleId) -> &'a [u32] {
        reach
            .alternatives
            .get(&rule)
            .or_else(|| self.always.alternatives.get(&rule))
            .map_or(&[], Vec::as_slice)
    }
}

/// A digest of a value, wide enough that two values sharing one can be ignored: a text
/// printed from a mistaken match would still have to read back as the value it was for.
#[derive(Debug, Clone, Copy)]
struct Digest(u64, u64);

impl Default for Digest {
    fn default() -> Self {
        Self(0x243f_6a88_85a3_08d3, 0x1319_8a2e_0370_7344)
    }
}

impl Digest {
    fn word(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
        self.1 = (self.1 ^ word)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .rotate_left(29);
    }

    fn key(self) -> u128 {
        u128::from(self.0) << 64 | u128::from(self.1)
    }
}

impl Hasher for Digest {
    fn write(&mut self, bytes: &[u8]) {
        let (chunks, left) = bytes.as_chunks::<8>();
        for chunk in chunks {
            self.word(u64::from_le_bytes(*chunk));
        }
        let mut last = [0; 8];
        last[..left.len()].copy_from_slice(left);
        self.word(u64::from_le_bytes(last) ^ (bytes.len() as u64) << 56);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// Distinct values under small numbers, each with what it reaches, so a memo key costs
/// one digest per value and not a serialization per call. Equal values written in another
/// field order may get two numbers, which costs one more lookup.
#[derive(Debug, Default)]
pub struct Values {
    ids: HashMap<u128, u32>,
    reaches: Vec<Rc<Reach>>,
}

impl Values {
    /// The number of a value, and what it reaches.
    pub fn id(&mut self, net: &Net, value: &Value) -> (u32, Rc<Reach>) {
        let mut state = Digest::default();
        digest(value, &mut state);
        let id = *self.ids.entry(state.key()).or_insert_with(|| {
            self.reaches.push(Rc::new(net.reach(value)));
            count(self.reaches.len() - 1)
        });
        (id, Rc::clone(&self.reaches[id as usize]))
    }
}

fn digest(value: &Value, state: &mut Digest) {
    match value {
        Value::Null => state.word(1),
        Value::Bool(flag) => state.word(2 + u64::from(*flag)),
        Value::Number(number) => number.hash(state),
        Value::String(text) => state.write(text.as_bytes()),
        Value::Array(items) => {
            state.word(4 + ((items.len() as u64) << 8));
            items.iter().for_each(|item| digest(item, state));
        }
        Value::Object(object) => {
            state.word(5 + ((object.len() as u64) << 8));
            for (name, item) in object {
                state.write(name.as_bytes());
                digest(item, state);
            }
        }
    }
}
