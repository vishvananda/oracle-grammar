//! What every value a pattern prints holds, as the nodes a trace keys: the pattern's
//! own nodes and, through its holes and splices, what the children bound to them
//! print. A symbol the printer looks through keeps what it matches.
use crate::grammar::RuleId;
use crate::pattern::{Item, Pattern, passes_through};
use crate::print_trace::{ELEMENT, FEATURE_DEPTH, feature_key, node_tag};
use rustc_hash::FxHashMap as HashMap;
use serde_json::Value;
use std::rc::Rc;

/// The most transparent symbols a required node may sit under; past them a symbol
/// matches anything.
const MOST_GUARDS: usize = 4;

/// The most nodes that may meet one required node instead; past them it is dropped.
const MOST_INSTEAD: usize = 40;

/// A node every value some rule prints holds, at a path from the value's root, unless
/// the value holds one of the `unless` nodes instead: a transparent symbol's own node,
/// or the same node where a conjunct may sit instead.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Feature {
    path: Vec<Rc<str>>,
    kind: u8,
    text: Rc<str>,
    unless: Vec<Node>,
}

/// A node as the trace keys it: where it is, its kind and its spelling.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Node {
    path: Vec<Rc<str>>,
    kind: u8,
    text: Rc<str>,
}

impl Node {
    fn key(&self) -> u32 {
        feature_key(self.path.iter().map(|step| &**step), self.kind, &self.text)
    }

    fn moved(&self, prefix: &[Rc<str>]) -> Self {
        let mut path = prefix.to_vec();
        path.extend(self.path.iter().cloned());
        Self {
            path,
            kind: self.kind,
            text: self.text.clone(),
        }
    }
}

impl Feature {
    fn node(&self) -> Node {
        Node {
            path: self.path.clone(),
            kind: self.kind,
            text: self.text.clone(),
        }
    }

    /// The feature's key and those of the nodes that would meet it instead.
    pub(super) fn keys(&self) -> (u32, Vec<u32>) {
        (
            self.node().key(),
            self.unless.iter().map(Node::key).collect(),
        )
    }

    /// One conjunct of an `@and`'s value, as its argument takes it: an object of one
    /// kind, so not an `AllOf` list or `Any`, which a symbol node cannot be either.
    fn one_conjunct(&self) -> bool {
        self.path.len() == 1
            && &*self.path[0] == "kind"
            && self.kind == b'S'
            && !matches!(&*self.text, "AllOf" | "Any")
            && self.unless.iter().all(|node| node.path.len() == 1)
    }
}

/// Sorted and without repeats.
pub(super) type Set = Vec<Feature>;

/// The nodes a value fitting a pattern holds, gathered from the printed value's root.
pub(super) struct Walk<'a> {
    bound: &'a HashMap<&'a str, RuleId>,
    found: &'a HashMap<RuleId, Option<Set>>,
    path: Vec<Rc<str>>,
    guards: Vec<Node>,
    pub(super) out: Set,
}

impl<'a> Walk<'a> {
    pub(super) fn new(
        bound: &'a HashMap<&'a str, RuleId>,
        found: &'a HashMap<RuleId, Option<Set>>,
    ) -> Self {
        Self {
            bound,
            found,
            path: Vec::new(),
            guards: Vec::new(),
            out: Set::new(),
        }
    }

    /// What the child bound to a hole requires now: `Some(None)` when it prints nothing
    /// yet, `None` when no child is bound or it lies past the closure.
    fn child(&self, name: &str) -> Option<&'a Option<Set>> {
        let found = self.found;
        self.bound.get(name).and_then(|child| found.get(child))
    }

    /// `false` when a child bound to a hole prints nothing yet.
    pub(super) fn pattern(&mut self, pattern: &Pattern) -> bool {
        if self.path.len() > FEATURE_DEPTH {
            return true;
        }
        match pattern {
            Pattern::Value(value) => self.value(value),
            Pattern::Symbol(name, args) => {
                // The trace keeps a symbol node's name one step below it.
                if passes_through(name)
                    && self.path.len() < FEATURE_DEPTH
                    && self.guards.len() < MOST_GUARDS
                    && let Some(inner) = args.first()
                {
                    let name = crate::bind::damage_relative::ordinary_name(name);
                    let mut path = self.path.clone();
                    path.push(Rc::from("$sym"));
                    self.guards.push(Node {
                        path,
                        kind: b'S',
                        text: Rc::from(name),
                    });
                    let derives = self.pattern(inner);
                    self.guards.pop();
                    return derives;
                }
                match (&**name, args.as_slice()) {
                    // Only the order of the list's items may differ.
                    ("unordered", [inner @ Pattern::List(_)]) => return self.pattern(inner),
                    ("and", _) => return self.conjuncts(args),
                    ("either_event", [left, right]) => return self.either(left, right),
                    ("entering_controller", [inner]) => return self.entering(inner),
                    _ => {}
                }
            }
            Pattern::Hole(name, _) => match self.child(name) {
                Some(None) => return false,
                Some(Some(set)) => self.shift(set, false),
                None => {}
            },
            Pattern::Object(fields) => {
                self.node(b'O', "");
                for (name, inner) in fields {
                    self.path.push(Rc::from(name.as_str()));
                    let derives = self.pattern(inner);
                    self.path.pop();
                    if !derives {
                        return false;
                    }
                }
            }
            Pattern::List(items) => {
                self.node(b'L', "");
                for item in items {
                    match item {
                        Item::One(inner) => {
                            self.path.push(Rc::from(ELEMENT));
                            let derives = self.pattern(inner);
                            self.path.pop();
                            if !derives {
                                return false;
                            }
                        }
                        // A splice's child prints the run of items it took, as a list.
                        Item::Splice(name, _) => match self.child(name) {
                            Some(None) => return false,
                            Some(Some(set)) => self.shift(set, true),
                            None => {}
                        },
                    }
                }
            }
        }
        true
    }

    fn node(&mut self, kind: u8, text: &str) {
        self.out.push(Feature {
            path: self.path.clone(),
            kind,
            text: Rc::from(text),
            unless: self.guards.clone(),
        });
    }

    /// Every node of a literal value, as the trace records the nodes of looked-up ones.
    fn value(&mut self, value: &Value) {
        if self.path.len() > FEATURE_DEPTH {
            return;
        }
        let (kind, text) = node_tag(value);
        self.node(kind, &text);
        match value {
            Value::Object(object) => {
                for (name, inner) in object {
                    self.path.push(Rc::from(name.as_str()));
                    self.value(inner);
                    self.path.pop();
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.path.push(Rc::from(ELEMENT));
                    self.value(item);
                    self.path.pop();
                }
            }
            _ => {}
        }
    }

    /// What a child requires of its value, placed here; for a splice, only what it
    /// requires of the items, which become items here. A feature whose symbol nodes
    /// would lie past what the trace keeps is dropped.
    fn shift(&mut self, set: &Set, items: bool) {
        let moved = |path: &[Rc<str>]| {
            let mut out = self.path.clone();
            out.extend(path.iter().cloned());
            out
        };
        for feature in set {
            if items && feature.path.first().map(|step| &**step) != Some(ELEMENT)
                || self.path.len() + feature.path.len() > FEATURE_DEPTH
            {
                continue;
            }
            let mut unless = self.guards.clone();
            unless.extend(feature.unless.iter().map(|node| node.moved(&self.path)));
            if unless.len() > MOST_INSTEAD
                || unless.iter().any(|node| node.path.len() > FEATURE_DEPTH)
            {
                continue;
            }
            unless.sort_unstable();
            self.out.push(Feature {
                path: moved(&feature.path),
                kind: feature.kind,
                text: feature.text.clone(),
                unless,
            });
        }
    }

    /// `@and` deals the value's conjuncts, the nodes under its nested `AllOf` lists, out
    /// among its arguments. An argument that takes one conjunct holds its nodes at that
    /// conjunct's depth; past the deepest depth the trace keeps them for, the value has
    /// an `AllOf` list there. Another argument may take several conjuncts or none.
    fn conjuncts(&mut self, args: &[Pattern]) -> bool {
        for arg in args {
            let mut inner = Walk::new(self.bound, self.found);
            if !inner.pattern(arg) {
                return false;
            }
            if !inner.out.iter().any(Feature::one_conjunct) {
                continue;
            }
            for feature in &inner.out {
                self.place(feature);
            }
        }
        true
    }

    /// `@either_event` matches `{"AnyOf": [left, right]}`, in either order.
    fn either(&mut self, left: &Pattern, right: &Pattern) -> bool {
        let depth = self.path.len();
        self.node(b'O', "");
        self.path.push(Rc::from("AnyOf"));
        if self.path.len() <= FEATURE_DEPTH {
            self.node(b'L', "");
        }
        self.path.push(Rc::from(ELEMENT));
        let derives = self.pattern(left) && self.pattern(right);
        self.path.truncate(depth);
        derives
    }

    /// `@entering_controller` matches a source-entry replacement as it is, or as it reads
    /// with each `ControllerOf(EventSubject)` under its body taken as `AbilityController`
    /// (`src/functions/entering.rs`), so such a node may be the original instead.
    fn entering(&mut self, inner: &Pattern) -> bool {
        let start = self.out.len();
        let derives = self.pattern(inner);
        let mut body = self.path.clone();
        body.extend([Rc::from("args"), Rc::from("body")]);
        let read = |node: &Node| {
            node.path.starts_with(&body)
                && node.path.last().is_some_and(|step| &**step == "kind")
                && node.kind == b'S'
                && &*node.text == "AbilityController"
        };
        for feature in &mut self.out[start..] {
            let mut original: Vec<Node> = std::iter::once(feature.node())
                .chain(feature.unless.iter().cloned())
                .filter(read)
                .map(|node| Node {
                    text: Rc::from("ControllerOf"),
                    ..node
                })
                .collect();
            feature.unless.append(&mut original);
        }
        derives
    }

    fn place(&mut self, feature: &Feature) {
        let deepest = feature
            .unless
            .iter()
            .map(|node| node.path.len())
            .chain([feature.path.len(), 1])
            .max()
            .unwrap_or(1);
        let Some(room) = FEATURE_DEPTH.checked_sub(self.path.len() + deepest) else {
            return;
        };
        let (mut at, mut nodes) = (self.path.clone(), Vec::new());
        for depth in 0..=room / 2 {
            if depth > 0 {
                at.extend([Rc::from("args"), Rc::from(ELEMENT)]);
            }
            nodes.push(feature.node().moved(&at));
            nodes.extend(feature.unless.iter().map(|node| node.moved(&at)));
        }
        at.push(Rc::from("kind"));
        nodes.push(Node {
            path: at,
            kind: b'S',
            text: Rc::from("AllOf"),
        });
        nodes.extend(self.guards.iter().cloned());
        if nodes.len() > MOST_INSTEAD + 1 {
            return;
        }
        let first = nodes.remove(0);
        self.out.push(Feature {
            path: first.path,
            kind: first.kind,
            text: first.text,
            unless: nodes,
        });
    }
}

pub(super) fn normal(mut set: Set) -> Set {
    for feature in &mut set {
        feature.unless.sort_unstable();
        feature.unless.dedup();
    }
    set.sort_unstable();
    set.dedup();
    set
}

pub(super) fn intersect(left: &Set, right: &Set) -> Set {
    let (mut a, mut b) = (left.iter().peekable(), right.iter().peekable());
    let mut out = Set::new();
    while let (Some(x), Some(y)) = (a.peek(), b.peek()) {
        match x.cmp(y) {
            std::cmp::Ordering::Less => {
                a.next();
            }
            std::cmp::Ordering::Greater => {
                b.next();
            }
            std::cmp::Ordering::Equal => {
                out.push((*x).clone());
                a.next();
                b.next();
            }
        }
    }
    out
}
