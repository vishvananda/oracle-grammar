//! Compact `Copy` tokens and pre-interned word tables for zero-allocation printing.
//!
//! Public APIs and parsing continue to use [`Token`]. Inside [`crate::render::Renderer`],
//! grammar literals are pre-interned into `u32` word IDs so ranking, trigram scoring,
//! and style checks never clone `String`s or re-hash words.
use crate::grammar::{Class, Rule, RuleId, Sym};
use crate::style::{self, Model};
use crate::tokens::Token;
use rustc_hash::FxHashMap as HashMap;
use std::sync::Arc;

/// An 8-byte `Copy` token used during inverse rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RToken {
    /// Index into [`WordTable::words`].
    Word(u32),
    /// Index into a renderer's dynamic-word arena (only for `Class::Word`/`Capitalized`
    /// strings not already present in [`WordTable::words`]).
    Dynamic(u32),
    Number(u32),
    Mark(char),
    SelfName(u8),
    Newline,
}

/// One pre-compiled step of a grammar alternative for the renderer.
#[derive(Debug, Clone)]
pub(crate) enum CompiledStep {
    /// One or more consecutive static literal tokens (`Word`, `Mark`, `Newline`),
    /// stored as a shared slice with combined style trigger flags.
    Literal { tokens: Arc<[RToken]>, flags: u64 },
    /// `Class::SelfName` at original symbol index `sym`.
    SelfName { sym: usize },
    /// A dynamic token class (`Number`, `Word`, `Capitalized`) at `sym`.
    Class {
        sym: usize,
        class: Class,
        hole: Option<String>,
    },
    /// A child nonterminal rule at `sym`.
    Rule {
        sym: usize,
        rule: RuleId,
        hole: Option<String>,
    },
}

/// Pre-parsed `@prefer` annotation for allocation-free preference checks.
#[derive(Debug, Clone)]
pub(crate) enum CompiledPrefer {
    Always,
    Once,
    Recurs,
    Value(String),
    Symbol(String),
}

/// Interned grammar word vocabulary, style trigger bitmasks, and pre-resolved
/// trigram model token IDs.
#[derive(Debug)]
pub(crate) struct WordTable {
    pub(crate) words: Vec<String>,
    pub(crate) ids: HashMap<String, u32>,
    pub(crate) word_flags: Vec<u64>,
    pub(crate) style_ids: Vec<Option<u32>>,
    pub(crate) ascii_mark_style: [Option<u32>; 128],
    pub(crate) number_style_id: Option<u32>,
    pub(crate) self_style_id: Option<u32>,
    pub(crate) line_style_id: Option<u32>,
    pub(crate) style_bound: bool,
    pub(crate) compiled_alts: Vec<Vec<Vec<CompiledStep>>>,
    pub(crate) compiled_prefer: Vec<Vec<CompiledPrefer>>,
}

impl Default for WordTable {
    fn default() -> Self {
        Self {
            words: Vec::new(),
            ids: HashMap::default(),
            word_flags: Vec::new(),
            style_ids: Vec::new(),
            ascii_mark_style: [None; 128],
            number_style_id: None,
            self_style_id: None,
            line_style_id: None,
            style_bound: false,
            compiled_alts: Vec::new(),
            compiled_prefer: Vec::new(),
        }
    }
}

impl WordTable {
    pub(crate) fn build(rules: &[Rule]) -> Self {
        let mut table = Self::default();
        let mut compiled_alts = Vec::with_capacity(rules.len());
        let mut compiled_prefer = Vec::with_capacity(rules.len());
        for rule in rules {
            let mut alts = Vec::with_capacity(rule.alternatives.len());
            for alt in &rule.alternatives {
                alts.push(table.compile_alternative(&alt.syms, &alt.holes));
            }
            compiled_alts.push(alts);
            let prefs = rule.prefer.iter().filter_map(|p| parse_prefer(p)).collect();
            compiled_prefer.push(prefs);
        }
        table.compiled_alts = compiled_alts;
        table.compiled_prefer = compiled_prefer;
        table
    }

    pub(crate) fn bind_style(&mut self, model: &Model) {
        self.style_ids = self
            .words
            .iter()
            .map(|word| model.id_of_key(word))
            .collect();
        let mut buf = [0u8; 4];
        for b in 0..128u8 {
            let key = (b as char).encode_utf8(&mut buf);
            self.ascii_mark_style[b as usize] = model.id_of_key(key);
        }
        self.number_style_id = model.id_of_key("#");
        self.self_style_id = model.id_of_key("@");
        self.line_style_id = model.id_of_key(style::LINE);
        self.style_bound = true;
    }

    fn intern(&mut self, word: &str) -> u32 {
        if let Some(&id) = self.ids.get(word) {
            return id;
        }
        let id = u32::try_from(self.words.len()).unwrap_or(u32::MAX);
        self.words.push(word.to_string());
        self.word_flags.push(style::word_flags(word));
        self.ids.insert(word.to_string(), id);
        id
    }

    fn rtoken_of_word(&mut self, word: &str) -> RToken {
        word.parse()
            .map_or_else(|_| RToken::Word(self.intern(word)), RToken::Number)
    }

    fn compile_alternative(&mut self, syms: &[Sym], holes: &[Option<String>]) -> Vec<CompiledStep> {
        let mut steps = Vec::with_capacity(syms.len());
        let mut lit_buf: Vec<RToken> = Vec::new();
        let mut lit_flags: u64 = 0;

        let flush = |steps: &mut Vec<CompiledStep>, buf: &mut Vec<RToken>, flags: &mut u64| {
            if buf.is_empty() {
                return;
            }
            steps.push(CompiledStep::Literal {
                tokens: Arc::from(buf.as_slice()),
                flags: *flags,
            });
            buf.clear();
            *flags = 0;
        };

        for (sym_idx, (sym, hole)) in syms.iter().zip(holes).enumerate() {
            match sym {
                Sym::Word(word) => {
                    let tok = self.rtoken_of_word(word);
                    lit_flags |= self.token_flags(tok, &[]);
                    lit_buf.push(tok);
                }
                Sym::Mark(mark) => {
                    let tok = RToken::Mark(*mark);
                    lit_flags |= style::mark_flags(*mark);
                    lit_buf.push(tok);
                }
                Sym::Class(Class::Newline) => {
                    lit_buf.push(RToken::Newline);
                }
                Sym::Class(Class::SelfName) => {
                    flush(&mut steps, &mut lit_buf, &mut lit_flags);
                    steps.push(CompiledStep::SelfName { sym: sym_idx });
                }
                Sym::Class(class) => {
                    flush(&mut steps, &mut lit_buf, &mut lit_flags);
                    steps.push(CompiledStep::Class {
                        sym: sym_idx,
                        class: *class,
                        hole: hole.clone(),
                    });
                }
                Sym::Rule(rule) => {
                    flush(&mut steps, &mut lit_buf, &mut lit_flags);
                    steps.push(CompiledStep::Rule {
                        sym: sym_idx,
                        rule: *rule,
                        hole: hole.clone(),
                    });
                }
            }
        }
        flush(&mut steps, &mut lit_buf, &mut lit_flags);
        steps
    }

    #[inline]
    pub(crate) fn word_str<'a>(&'a self, tok: RToken, dyn_words: &'a [String]) -> Option<&'a str> {
        match tok {
            RToken::Word(id) => self.words.get(id as usize).map(String::as_str),
            RToken::Dynamic(id) => dyn_words.get(id as usize).map(String::as_str),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn token_flags(&self, tok: RToken, dyn_words: &[String]) -> u64 {
        match tok {
            RToken::Word(id) => self.word_flags.get(id as usize).copied().unwrap_or(0),
            RToken::Dynamic(id) => dyn_words
                .get(id as usize)
                .map_or(0, |w| style::word_flags(w)),
            RToken::Mark(c) => style::mark_flags(c),
            RToken::SelfName(_) => style::F_SELF_NAME,
            RToken::Number(_) | RToken::Newline => 0,
        }
    }

    #[inline]
    pub(crate) fn style_id(&self, tok: RToken, dyn_words: &[String], model: &Model) -> Option<u32> {
        if self.style_bound {
            match tok {
                RToken::Word(id) => self.style_ids.get(id as usize).copied().flatten(),
                RToken::Dynamic(id) => dyn_words.get(id as usize).and_then(|w| model.id_of_key(w)),
                RToken::Number(_) => self.number_style_id,
                RToken::Mark(c) if (c as u32) < 128 => self.ascii_mark_style[c as usize],
                RToken::Mark(c) => {
                    let mut buf = [0u8; 4];
                    model.id_of_key(c.encode_utf8(&mut buf))
                }
                RToken::SelfName(_) => self.self_style_id,
                RToken::Newline => self.line_style_id,
            }
        } else {
            let mut buf = [0u8; 4];
            let key = match tok {
                RToken::Word(id) => self.words.get(id as usize)?.as_str(),
                RToken::Dynamic(id) => dyn_words.get(id as usize)?.as_str(),
                RToken::Number(_) => "#",
                RToken::Mark(c) => c.encode_utf8(&mut buf),
                RToken::SelfName(_) => "@",
                RToken::Newline => style::LINE,
            };
            model.id_of_key(key)
        }
    }

    pub(crate) fn to_token(&self, tok: RToken, dyn_words: &[String]) -> Token {
        match tok {
            RToken::Word(id) => Token::Word(self.words[id as usize].clone()),
            RToken::Dynamic(id) => Token::Word(dyn_words[id as usize].clone()),
            RToken::Number(n) => Token::Number(n),
            RToken::Mark(c) => Token::Mark(c),
            RToken::SelfName(i) => Token::SelfName(i),
            RToken::Newline => Token::Newline,
        }
    }
}

fn parse_prefer(text: &str) -> Option<CompiledPrefer> {
    if text == "always" {
        Some(CompiledPrefer::Always)
    } else if text == "once" {
        Some(CompiledPrefer::Once)
    } else if text == "recurs" {
        Some(CompiledPrefer::Recurs)
    } else if let Some(kind) = text
        .strip_prefix("value(")
        .and_then(|s| s.strip_suffix(')'))
    {
        Some(CompiledPrefer::Value(kind.to_string()))
    } else {
        text.strip_prefix("symbol(")
            .and_then(|s| s.strip_suffix(')'))
            .map(|sym| CompiledPrefer::Symbol(sym.to_string()))
    }
}
