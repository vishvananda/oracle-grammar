//! Zero-allocation token-slice abstraction and style-rule trigger bitmasks.
use crate::rtoken::{RToken, WordTable};
use crate::tokens::Token;

pub(crate) const F_DRAW: u64 = 1 << 0;
pub(crate) const F_DAMAGE: u64 = 1 << 1;
pub(crate) const F_SACRIFICE: u64 = 1 << 2;
pub(crate) const F_COUNTED_NOUN: u64 = 1 << 3;
pub(crate) const F_VERB_AGREE: u64 = 1 << 4;
pub(crate) const F_MAY: u64 = 1 << 5;
pub(crate) const F_THIS: u64 = 1 << 6;
pub(crate) const F_ATTACH: u64 = 1 << 7;
pub(crate) const F_SLASH: u64 = 1 << 8;
pub(crate) const F_UNTIL: u64 = 1 << 9;
pub(crate) const F_HAS_HAVE: u64 = 1 << 10;
pub(crate) const F_SELF_NAME: u64 = 1 << 11;
pub(crate) const F_AND: u64 = 1 << 12;
pub(crate) const F_JOIN_VERB: u64 = 1 << 13;
pub(crate) const F_PLAIN: u64 = 1 << 14;
pub(crate) const F_GAINS: u64 = 1 << 15;
pub(crate) const F_SUBJECT_VERB: u64 = 1 << 16;
pub(crate) const F_WITH: u64 = 1 << 17;
pub(crate) const F_EQUAL: u64 = 1 << 18;
pub(crate) const F_THOUGH: u64 = 1 << 19;
pub(crate) const F_LPAREN: u64 = 1 << 20;
pub(crate) const F_KICKED: u64 = 1 << 21;
pub(crate) const F_ADD: u64 = 1 << 22;
pub(crate) const F_LBRACE: u64 = 1 << 23;
pub(crate) const F_SPEND: u64 = 1 << 24;
pub(crate) const F_CAST: u64 = 1 << 25;
pub(crate) const F_ACTIVATE: u64 = 1 << 26;
pub(crate) const F_OWNER: u64 = 1 << 27;
pub(crate) const F_WHERE: u64 = 1 << 28;
pub(crate) const F_QUOTE: u64 = 1 << 29;
pub(crate) const F_ARTICLE: u64 = 1 << 30;
pub(crate) const F_SPELL: u64 = 1 << 31;
pub(crate) const F_PERMANENT: u64 = 1 << 32;
pub(crate) const F_COMBAT: u64 = 1 << 33;
pub(crate) const F_SPELLS: u64 = 1 << 34;

pub(crate) const ALL_FLAGS: u64 = u64::MAX;

pub(crate) fn word_flags(word: &str) -> u64 {
    let mut f = 0u64;
    match word {
        "draw" => f |= F_DRAW,
        "damage" => f |= F_DAMAGE,
        "sacrifice" => f |= F_SACRIFICE,
        "counter" | "counters" | "token" | "tokens" => f |= F_COUNTED_NOUN,
        "get" => f |= F_VERB_AGREE | F_JOIN_VERB,
        "gets" => f |= F_VERB_AGREE | F_JOIN_VERB | F_SUBJECT_VERB,
        "gain" => f |= F_VERB_AGREE | F_JOIN_VERB | F_PLAIN,
        "gains" => f |= F_VERB_AGREE | F_JOIN_VERB | F_PLAIN | F_GAINS | F_SUBJECT_VERB,
        "have" => f |= F_VERB_AGREE | F_HAS_HAVE | F_JOIN_VERB | F_PLAIN,
        "has" => f |= F_VERB_AGREE | F_HAS_HAVE | F_JOIN_VERB | F_SUBJECT_VERB,
        "lose" => f |= F_VERB_AGREE | F_JOIN_VERB | F_PLAIN,
        "loses" => f |= F_VERB_AGREE | F_JOIN_VERB | F_SUBJECT_VERB,
        "become" | "becomes" => f |= F_VERB_AGREE,
        "deals" => f |= F_SUBJECT_VERB,
        "may" => f |= F_MAY,
        "this" => f |= F_THIS,
        "enchanted" | "equipped" => f |= F_ATTACH,
        "until" => f |= F_UNTIL,
        "and" => f |= F_AND,
        "life" | "target" | "whenever" | "sacrifices" => f |= F_PLAIN,
        "with" => f |= F_WITH,
        "equal" => f |= F_EQUAL,
        "though" => f |= F_THOUGH,
        "kicked" => f |= F_KICKED,
        "add" | "adds" => f |= F_ADD,
        "spend" => f |= F_SPEND,
        "cast" => f |= F_CAST,
        "activate" => f |= F_ACTIVATE,
        "owner's" | "owners'" => f |= F_OWNER,
        "where" => f |= F_WHERE,
        "a" | "an" => f |= F_ARTICLE,
        "spell" => f |= F_SPELL,
        "permanent" => f |= F_PERMANENT,
        "attacking" | "blocking" => f |= F_COMBAT,
        "spell's" => f |= F_SPELLS,
        _ => {}
    }
    f
}

#[inline]
pub(crate) fn mark_flags(mark: char) -> u64 {
    match mark {
        '/' => F_SLASH,
        '(' => F_LPAREN,
        '{' => F_LBRACE,
        '"' => F_QUOTE,
        _ => 0,
    }
}

pub(crate) fn slice_flags(tokens: &[Token]) -> u64 {
    let mut flags = 0u64;
    for tok in tokens {
        match tok {
            Token::Word(w) => flags |= word_flags(w),
            Token::Mark(c) => flags |= mark_flags(*c),
            Token::SelfName(_) => flags |= F_SELF_NAME,
            Token::Number(_) | Token::Grouped(_) | Token::Newline => {}
        }
    }
    flags
}

/// Uniform read-only view over `&[Token]` or `&[RToken]`, monomorphized with zero overhead.
pub(crate) trait TokenSlice {
    fn len(&self) -> usize;
    fn word(&self, at: usize) -> Option<&str>;
    fn mark(&self, at: usize) -> Option<char>;
    fn number(&self, at: usize) -> Option<u32>;
    fn is_self_name(&self, at: usize) -> bool;
    fn is_newline(&self, at: usize) -> bool;

    #[inline]
    fn is_word(&self, at: usize, text: &str) -> bool {
        self.word(at) == Some(text)
    }

    #[inline]
    fn is_mark(&self, at: usize, ch: char) -> bool {
        self.mark(at) == Some(ch)
    }

    #[inline]
    fn token_flags(&self, _at: usize) -> u64 {
        ALL_FLAGS
    }
}

impl TokenSlice for [Token] {
    #[inline]
    fn len(&self) -> usize {
        self.len()
    }

    #[inline]
    fn word(&self, at: usize) -> Option<&str> {
        match self.get(at) {
            Some(Token::Word(w)) => Some(w.as_str()),
            _ => None,
        }
    }

    #[inline]
    fn mark(&self, at: usize) -> Option<char> {
        match self.get(at) {
            Some(Token::Mark(c)) => Some(*c),
            _ => None,
        }
    }

    #[inline]
    fn number(&self, at: usize) -> Option<u32> {
        match self.get(at) {
            Some(Token::Number(n)) => Some(*n),
            _ => None,
        }
    }

    #[inline]
    fn is_self_name(&self, at: usize) -> bool {
        matches!(self.get(at), Some(Token::SelfName(_)))
    }

    #[inline]
    fn is_newline(&self, at: usize) -> bool {
        matches!(self.get(at), Some(Token::Newline))
    }
}

pub(crate) struct RTokenSlice<'a> {
    pub(crate) tokens: &'a [RToken],
    pub(crate) table: &'a WordTable,
    pub(crate) dyn_words: &'a [String],
}

impl TokenSlice for RTokenSlice<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.tokens.len()
    }

    #[inline]
    fn word(&self, at: usize) -> Option<&str> {
        self.table.word_str(*self.tokens.get(at)?, self.dyn_words)
    }

    #[inline]
    fn mark(&self, at: usize) -> Option<char> {
        match self.tokens.get(at) {
            Some(RToken::Mark(c)) => Some(*c),
            _ => None,
        }
    }

    #[inline]
    fn number(&self, at: usize) -> Option<u32> {
        match self.tokens.get(at) {
            Some(RToken::Number(n) | RToken::Grouped(n)) => Some(*n),
            _ => None,
        }
    }

    #[inline]
    fn is_self_name(&self, at: usize) -> bool {
        matches!(self.tokens.get(at), Some(RToken::SelfName(_)))
    }

    #[inline]
    fn is_newline(&self, at: usize) -> bool {
        matches!(self.tokens.get(at), Some(RToken::Newline))
    }

    #[inline]
    fn token_flags(&self, at: usize) -> u64 {
        match self.tokens.get(at) {
            Some(RToken::Word(id)) => self.table.word_flags[*id as usize],
            Some(RToken::Dynamic(id)) => word_flags(&self.dyn_words[*id as usize]),
            Some(RToken::Mark(c)) => mark_flags(*c),
            Some(RToken::SelfName(_)) => F_SELF_NAME,
            _ => 0,
        }
    }
}

pub(crate) trait BreachSink {
    fn push(&mut self, name: &'static str, cost: f32, at: usize);
}

impl BreachSink for Vec<(&'static str, f32, usize)> {
    #[inline]
    fn push(&mut self, name: &'static str, cost: f32, at: usize) {
        Vec::push(self, (name, cost, at));
    }
}

#[derive(Default)]
pub(crate) struct CostSum(pub(crate) f32);

impl BreachSink for CostSum {
    #[inline]
    fn push(&mut self, _name: &'static str, cost: f32, _at: usize) {
        self.0 += cost;
    }
}
