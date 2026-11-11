//! One way to print a value, held as what was built and not as text. Printing joins far
//! more often than it reads, so a join here is constant time, and the tree can say what
//! the text means by filling the same patterns the parser fills, with no search.
use crate::grammar::{Grammar, RuleId};
use crate::pattern::Bindings;
use crate::tokens::Token;
use serde_json::Value;
use std::borrow::Cow;
use std::rc::Rc;

/// A sequence joined in constant time.
#[derive(Debug, Clone)]
pub struct Rope<T>(Option<Rc<Knot<T>>>);

#[derive(Debug)]
enum Knot<T> {
    Leaf(T),
    Join(Rope<T>, Rope<T>),
}

impl<T> Default for Rope<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: Clone> Rope<T> {
    pub fn leaf(item: T) -> Self {
        Self(Some(Rc::new(Knot::Leaf(item))))
    }

    pub fn join(&self, other: &Self) -> Self {
        match (&self.0, &other.0) {
            (None, _) => other.clone(),
            (_, None) => self.clone(),
            _ => Self(Some(Rc::new(Knot::Join(self.clone(), other.clone())))),
        }
    }

    /// The items in order.
    pub fn items(&self) -> Vec<T> {
        let mut out = Vec::new();
        let mut stack = vec![self];
        while let Some(Self(knot)) = stack.pop() {
            match knot.as_deref() {
                None => {}
                Some(Knot::Leaf(item)) => out.push(item.clone()),
                Some(Knot::Join(left, right)) => {
                    stack.push(right);
                    stack.push(left);
                }
            }
        }
        out
    }
}

/// A child of a production: the symbol it fills, and the tokens before it.
#[derive(Debug, Clone)]
pub struct Kid {
    pub sym: usize,
    pub offset: usize,
    pub node: Rc<Node>,
}

/// What a derivation built.
#[derive(Debug)]
pub enum Node {
    /// A token class carrying a value.
    Value(Value),
    /// A rule with a pattern, and the children that fill it.
    Rule {
        rule: RuleId,
        alternative: usize,
        kids: Vec<Kid>,
    },
}

impl Node {
    /// The value the parser builds for this tree, when it starts at token `at`.
    pub fn fill(&self, grammar: &Grammar, at: usize) -> Result<Rc<Value>, String> {
        let Self::Rule {
            rule,
            alternative,
            kids,
        } = self
        else {
            let Self::Value(value) = self else {
                return Err("not a value".into());
            };
            return Ok(Rc::new(value.clone()));
        };
        let definition = &grammar.rules[*rule as usize];
        let holes = &definition.alternatives[*alternative].holes;
        let mut filled = Vec::with_capacity(kids.len());
        for kid in kids {
            filled.push((kid.sym, kid.node.fill(grammar, at + kid.offset)?));
        }
        let Some(pattern) = &definition.sem else {
            return Err("a pass-through rule built a node".into());
        };
        let mut bindings = Bindings::new();
        for (sym, value) in &filled {
            if let Some(hole) = &holes[*sym] {
                bindings.insert(hole, Cow::Borrowed(&**value));
            }
        }
        Ok(Rc::new(pattern.fill(&bindings, at)?))
    }
}

/// One text for a value, the named productions it used, and what it built.
#[derive(Debug, Clone, Default)]
pub struct Derivation {
    pub tokens: Rope<Token>,
    /// Each rule used, with the alternative taken.
    pub productions: Rope<(RuleId, usize)>,
    /// How many `@prefer`red alternatives it took.
    pub preferred: u32,
    /// How many tokens it holds.
    pub length: usize,
    /// The children so far of a production being walked.
    pub kids: Rope<Kid>,
    /// What this finished derivation built, when it built anything.
    pub node: Option<Rc<Node>>,
}

impl Derivation {
    pub fn token(token: Token, value: Option<Value>) -> Self {
        Self {
            tokens: Rope::leaf(token),
            length: 1,
            node: value.map(|value| Rc::new(Node::Value(value))),
            ..Self::default()
        }
    }

    /// Add `piece`, as the symbol at index `sym`, to this production so far.
    pub fn then(&self, piece: &Self, sym: usize) -> Self {
        let kids = match &piece.node {
            Some(node) => self.kids.join(&Rope::leaf(Kid {
                sym,
                offset: self.length,
                node: Rc::clone(node),
            })),
            None => self.kids.clone(),
        };
        Self {
            tokens: self.tokens.join(&piece.tokens),
            productions: self.productions.join(&piece.productions),
            preferred: self.preferred + piece.preferred,
            length: self.length + piece.length,
            kids,
            node: None,
        }
    }

    /// Close a production with a pattern over the children gathered.
    pub fn close(&mut self, rule: RuleId, alternative: usize) {
        self.node = Some(Rc::new(Node::Rule {
            rule,
            alternative,
            kids: self.kids.items(),
        }));
        self.kids = Rope::default();
    }
}
