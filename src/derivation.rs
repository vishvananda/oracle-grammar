//! One way to print a value, held as what was built and not as text. Printing joins far
//! more often than it reads, so a join here is constant time, and the tree can say what
//! the text means by filling the same patterns the parser fills, with no search.
use crate::grammar::{Grammar, RuleId};
use crate::pattern::Bindings;
use crate::rtoken::RToken;
use serde_json::Value;
use std::borrow::Cow;
use std::rc::Rc;
use std::sync::Arc;

/// A sequence joined in constant time.
#[derive(Debug, Clone)]
pub struct Rope<T>(Option<Rc<Knot<T>>>);

#[derive(Debug)]
enum Knot<T> {
    Leaf(T),
    Chunk(Arc<[T]>),
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

    pub(crate) fn chunk(items: Arc<[T]>) -> Self {
        if items.is_empty() {
            Self(None)
        } else {
            Self(Some(Rc::new(Knot::Chunk(items))))
        }
    }

    pub fn join(&self, other: &Self) -> Self {
        match (&self.0, &other.0) {
            (None, _) => other.clone(),
            (_, None) => self.clone(),
            _ => Self(Some(Rc::new(Knot::Join(self.clone(), other.clone())))),
        }
    }

    /// Append the items in order into `out` without allocating a new output vector.
    pub fn items_into(&self, out: &mut Vec<T>) {
        out.clear();
        let mut stack = smallvec::SmallVec::<[&Self; 32]>::new();
        stack.push(self);
        while let Some(Self(knot)) = stack.pop() {
            match knot.as_deref() {
                None => {}
                Some(Knot::Leaf(item)) => out.push(item.clone()),
                Some(Knot::Chunk(items)) => out.extend_from_slice(items),
                Some(Knot::Join(left, right)) => {
                    stack.push(right);
                    stack.push(left);
                }
            }
        }
    }

    /// The items in order.
    pub fn items(&self) -> Vec<T> {
        let mut out = Vec::new();
        self.items_into(&mut out);
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
    pub(crate) tokens: Rope<RToken>,
    /// Each rule used, with the alternative taken.
    pub productions: Rope<(RuleId, usize)>,
    /// How many `@prefer`red alternatives it took.
    pub preferred: u32,
    /// How many of its references to the card pass over the card's best word for itself.
    pub missed: u32,
    /// How well the best word for the card it says names the card.
    pub word: u32,
    /// How many references to the card it holds whose word has been weighed.
    pub references: u32,
    /// How many tokens it holds.
    pub length: usize,
    /// Bitmask of style-rule trigger tokens present in `tokens`.
    pub(crate) style_flags: u64,
    /// Bitset of matched key steps when the card's key has at most 128 entries.
    pub(crate) key_bits: [u64; 2],
    /// The children so far of a production being walked.
    pub kids: Rope<Kid>,
    /// What this finished derivation built, when it built anything.
    pub node: Option<Rc<Node>>,
}

impl Derivation {
    pub(crate) fn token(token: RToken, flags: u64, value: Option<Value>) -> Self {
        Self {
            tokens: Rope::leaf(token),
            length: 1,
            style_flags: flags,
            node: value.map(|value| Rc::new(Node::Value(value))),
            ..Self::default()
        }
    }

    /// Append a static literal token run in place (never carries a semantic `Node`).
    #[inline]
    pub(crate) fn append_literal(&mut self, rope: &Rope<RToken>, len: usize, flags: u64) {
        self.tokens = self.tokens.join(rope);
        self.length += len;
        self.style_flags |= flags;
        self.node = None;
    }

    /// Append one token (optionally with a semantic `Node`) in place.
    #[inline]
    pub(crate) fn append_token(
        &mut self,
        token: RToken,
        flags: u64,
        sym: usize,
        node: Option<Rc<Node>>,
    ) {
        if let Some(node) = node {
            self.kids = self.kids.join(&Rope::leaf(Kid {
                sym,
                offset: self.length,
                node,
            }));
        }
        self.tokens = self.tokens.join(&Rope::leaf(token));
        self.length += 1;
        self.style_flags |= flags;
        self.node = None;
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
            missed: self.missed + piece.missed,
            word: self.word.max(piece.word),
            references: self.references + piece.references,
            length: self.length + piece.length,
            style_flags: self.style_flags | piece.style_flags,
            key_bits: [
                self.key_bits[0] | piece.key_bits[0],
                self.key_bits[1] | piece.key_bits[1],
            ],
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
