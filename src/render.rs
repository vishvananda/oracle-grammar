//! The inverse direction: every derivation of a value, read from the same
//! declarations the parser instantiates. Recursion is bounded by the value and
//! each node has finitely many productions, so enumeration always terminates.
use crate::derivation::{Derivation, Rope};
use crate::grammar::{Alternative, Class, Grammar, RuleId, Sym};
use crate::heads::{Reach, Values};
use crate::pattern::Bindings;
use crate::style::Model;
use crate::tokens::Token;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use serde_json::Value;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

mod branches;
mod context;
mod prefer;
pub(crate) mod words;

type Found = Rc<Vec<Derivation>>;

/// How many derivations a join may hold before the style model cuts them to the cap.
const WIDEN: usize = 4;
/// What each distinct alternative of a card's key is worth against everything else.
const KEY_WEIGHT: f32 = 1000.0;
/// What a `@prefer` is worth, taken or passed over: more than any style score.
const PREFER_WEIGHT: f32 = 100.0;

/// What printing did, to compare a change by more than the clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct Work {
    /// Distinct rule and value pairs tried.
    pub nodes: u64,
    /// Patterns unified, and how many matched.
    pub unified: u64,
    pub matched: u64,
    /// Alternatives walked, and derivations built by joining their symbols.
    pub walked: u64,
    pub joined: u64,
    /// Derivations ranked, and the tokens read to rank them.
    pub ranked: u64,
    pub read: u64,
    /// Rankings that dropped something.
    pub cuts: u64,
    /// Derivation-tree nodes visited to retain distinct ability productions.
    pub branch_nodes: u64,
}

impl Work {
    pub fn add(&mut self, other: &Self) {
        self.nodes += other.nodes;
        self.unified += other.unified;
        self.matched += other.matched;
        self.walked += other.walked;
        self.joined += other.joined;
        self.ranked += other.ranked;
        self.read += other.read;
        self.cuts += other.cuts;
        self.branch_nodes += other.branch_nodes;
    }
}

pub struct Renderer<'a> {
    pub work: Work,
    grammar: &'a Grammar,
    style: &'a Model,
    /// Alternatives a card's key names. A rule it names takes no others.
    key: HashSet<(RuleId, usize)>,
    keyed_rules: HashSet<RuleId>,
    /// The most derivations kept for any one node.
    cap: usize,
    values: Values,
    memo: HashMap<(RuleId, u32, usize), Found>,
    nothing: Found,
    /// Set when a cap dropped a derivation, so a count is a lower bound.
    pub truncated: bool,
    /// The spelling of the card's name to print.
    self_name: u8,
    /// Card types, for `@prefer type(...)`.
    contexts: Vec<Vec<String>>,
    /// The card's best word for itself in each context.
    best: Vec<u32>,
    context: usize,
    /// The words of the card's name, when it is known: a seeded rule that calls the card
    /// by a name prints only for the card it names.
    name: Option<HashSet<String>>,
}

impl<'a> Renderer<'a> {
    pub fn new(grammar: &'a Grammar, style: &'a Model, cap: usize, context: Vec<String>) -> Self {
        let context = words::card_context(context);
        Self {
            work: Work::default(),
            grammar,
            style,
            key: HashSet::default(),
            keyed_rules: HashSet::default(),
            cap,
            values: Values::default(),
            memo: HashMap::default(),
            nothing: Rc::new(Vec::new()),
            truncated: false,
            self_name: 0,
            best: vec![grammar.words.best(&context)],
            contexts: vec![context],
            context: 0,
            name: None,
        }
    }

    /// Print the card's name as this entry of its [`crate::tokens::Names`].
    pub fn with_self_name(mut self, index: u8) -> Self {
        self.self_name = index;
        self
    }

    /// Print for the card of this name.
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(words::name_words(name));
        self
    }

    /// Hold every rule the key names to the alternatives it names.
    pub fn with_key(mut self, key: HashSet<(RuleId, usize)>) -> Self {
        self.keyed_rules = key.iter().map(|(rule, _)| *rule).collect();
        self.key = key;
        self
    }

    /// What the key, `@prefer` and the canonical style rules add to a text's rank.
    pub fn weight(&self, derivation: &Derivation, tokens: &[Token]) -> f32 {
        let named = if self.key.is_empty() {
            0
        } else {
            let steps = derivation.productions.items();
            let named: HashSet<&(RuleId, usize)> = steps
                .iter()
                .filter(|step| self.key.contains(*step))
                .collect();
            named.len()
        };
        KEY_WEIGHT * named as f32
            + PREFER_WEIGHT * (derivation.preferred as f32 - derivation.missed as f32)
            - crate::style::broken_rules(tokens, &self.contexts[self.context])
    }

    fn rank(&mut self, derivation: &Derivation) -> f32 {
        let tokens = derivation.tokens.items();
        self.work.ranked += 1;
        self.work.read += tokens.len() as u64;
        self.style.score(&tokens, false) + self.weight(derivation, &tokens)
    }

    /// `found` best first.
    /// `found` best first, with the alternative each came from, keeping one tree of each
    /// text: a beam full of one text in many derivations has no room for another text.
    fn scored(&mut self, found: Vec<Derivation>, opens: &[usize]) -> Vec<(usize, Derivation)> {
        let mut scored: Vec<(f32, u64, usize, Derivation)> = found
            .into_iter()
            .enumerate()
            .map(|(at, derivation)| {
                let tokens = derivation.tokens.items();
                let mut hasher = rustc_hash::FxHasher::default();
                tokens.hash(&mut hasher);
                let alternative = opens.partition_point(|open| *open <= at);
                (
                    self.rank(&derivation),
                    hasher.finish(),
                    alternative,
                    derivation,
                )
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut seen = HashSet::default();
        scored
            .into_iter()
            .filter(|(_, text, ..)| seen.insert(*text))
            .map(|(_, _, alternative, derivation)| (alternative, derivation))
            .collect()
    }

    fn ranked(&mut self, found: Vec<Derivation>) -> Vec<Derivation> {
        self.scored(found, &[])
            .into_iter()
            .map(|(_, derivation)| derivation)
            .collect()
    }

    /// `found` ranked, with the best text that passes over a better word for the card
    /// moved up behind the best, so that a cut product keeps it, as
    /// [`Self::best_of_each`] does.
    fn ranked_heads(&mut self, found: Vec<Derivation>) -> Vec<Derivation> {
        let mut ranked = self.ranked(found);
        let fewest = ranked.iter().map(|derivation| derivation.missed).min();
        if let Some(at) = ranked.iter().position(|d| Some(d.missed) > fewest)
            && at > 1
        {
            let passing = ranked.remove(at);
            ranked.insert(1, passing);
        }
        ranked
    }

    /// Derivations of `value` from `rule`, likeliest first. Empty means this
    /// grammar has no text for the value.
    pub fn render(&mut self, rule: RuleId, value: &Value) -> Found {
        let found = self.derive(rule, value);
        Rc::new(self.ranked(found.to_vec()))
    }

    /// Whether any rule's own pattern prints this value, whatever rule asked for it.
    pub fn printable(&mut self, value: &Value) -> bool {
        let (id, reach) = self.values.id(&self.grammar.net, value);
        let fits = reach.fits.clone();
        fits.iter()
            .any(|rule| !self.render_id(*rule, value, (id, &reach)).is_empty())
    }

    /// The rules whose own pattern prints this value.
    pub fn printers(&mut self, value: &Value) -> Vec<RuleId> {
        let (id, reach) = self.values.id(&self.grammar.net, value);
        let fits = reach.fits.clone();
        fits.into_iter()
            .filter(|rule| !self.render_id(*rule, value, (id, &reach)).is_empty())
            .collect()
    }

    /// Derivations in no order, which only a cut or the finished text needs.
    fn derive(&mut self, rule: RuleId, value: &Value) -> Found {
        let (id, reach) = self.values.id(&self.grammar.net, value);
        self.render_id(rule, value, (id, &reach))
    }

    fn render_id(&mut self, rule: RuleId, value: &Value, (id, reach): (u32, &Reach)) -> Found {
        let outer = self.context;
        if let Some(context) = context::own(value) {
            self.context = self.intern_context(context);
        }
        let found = self.render_contextual(rule, value, (id, reach));
        self.context = outer;
        found
    }

    fn intern_context(&mut self, context: Vec<String>) -> usize {
        self.contexts
            .iter()
            .position(|existing| existing == &context)
            .unwrap_or_else(|| {
                self.best.push(self.grammar.words.best(&context));
                self.contexts.push(context);
                self.contexts.len() - 1
            })
    }

    fn render_contextual(
        &mut self,
        rule: RuleId,
        value: &Value,
        (id, reach): (u32, &Reach),
    ) -> Found {
        if !self.grammar.net.reaches(reach, rule) {
            return Rc::clone(&self.nothing);
        }
        let key = (rule, id, self.context);
        if let Some(found) = self.memo.get(&key) {
            return Rc::clone(found);
        }
        // Cut a cycle through a pass-through rule: it adds no tokens, so it can
        // never produce a text the direct route does not.
        self.memo.insert(key, Rc::clone(&self.nothing));
        self.work.nodes += 1;
        let grammar = self.grammar;
        let definition = &grammar.rules[rule as usize];
        let mut all = Vec::new();
        let mut opens = Vec::new();
        match &definition.sem {
            Some(pattern) => {
                let solutions = pattern.unify(value, Bindings::new());
                self.work.unified += 1;
                self.work.matched += u64::from(!solutions.is_empty());
                if !solutions.is_empty() {
                    let contexts: Vec<_> = solutions
                        .iter()
                        .map(|bindings| {
                            context::holes(pattern, value, bindings, &self.contexts[self.context])
                        })
                        .collect();
                    for (index, alternative) in definition.alternatives.iter().enumerate() {
                        if self.allowed(rule, index) {
                            let found = solutions
                                .iter()
                                .zip(&contexts)
                                .flat_map(|(bindings, contexts)| {
                                    self.alternative(alternative, bindings, contexts)
                                })
                                .collect();
                            opens.push(all.len());
                            self.take((rule, index), alternative, value, found, &mut all);
                        }
                    }
                }
            }
            None => {
                for index in grammar.net.alternatives(reach, rule) {
                    let index = *index as usize;
                    if self.allowed(rule, index) {
                        let alternative = &definition.alternatives[index];
                        let found = self.passthrough(alternative, value, (id, reach));
                        opens.push(all.len());
                        self.take((rule, index), alternative, value, found, &mut all);
                    }
                }
            }
        }
        let found = Rc::new(self.best_of_each(rule, all, &opens));
        self.memo.insert(key, Rc::clone(&found));
        found
    }

    /// At most the beam's width, reserving representatives of ability productions
    /// through aliases and card-list wrappers, or alternatives for other values.
    /// A rule is ranked with no text around it, and what reads best alone may read worst
    /// in a sentence: every `this <noun>` beats `it` until there is something to refer to,
    /// so the best text that passes over a better word for the card keeps a place too.
    /// `opens` says where each alternative starts in `found`.
    fn best_of_each(
        &mut self,
        rule: RuleId,
        found: Vec<Derivation>,
        opens: &[usize],
    ) -> Vec<Derivation> {
        if found.len() <= self.cap {
            return found;
        }
        self.work.cuts += 1;
        self.truncated = true;
        let mut seen = HashSet::default();
        let mut reserved = self.cap / 2;
        let mut first = Vec::new();
        let mut rest = Vec::new();
        let fewest = found.iter().map(|derivation| derivation.missed).min();
        let mut passing = true;
        for (alternative, derivation) in self.scored(found, opens) {
            let distinct = if reserved > 0 {
                let (branch, nodes) = branches::of(self.grammar, rule, alternative, &derivation);
                self.work.branch_nodes += nodes;
                seen.insert(branch)
            } else {
                false
            };
            let passes = passing && Some(derivation.missed) > fewest;
            if distinct || passes {
                reserved = reserved.saturating_sub(usize::from(distinct));
                passing &= !passes;
                first.push(derivation);
            } else {
                rest.push(derivation);
            }
        }
        rest.truncate(self.cap - first.len());
        first.append(&mut rest);
        first
    }

    fn allowed(&self, rule: RuleId, index: usize) -> bool {
        if self.keyed_rules.contains(&rule) {
            return self.key.contains(&(rule, index));
        }
        let name = self.name.as_ref();
        name.is_none_or(|name| self.grammar.words.fit((rule, index), name))
    }

    /// Add what one alternative derived to its rule's derivations. Where a value is the
    /// card itself, the innermost such production weighs the word its text says for the
    /// card, once: `it` or `this creature` for a legendary card passes over the card's
    /// best word. A text weighs the words it says, not the ones it avoids: `another
    /// creature` passes over nothing against `a creature other than this creature`.
    fn take(
        &self,
        step: (RuleId, usize),
        alternative: &Alternative,
        value: &Value,
        found: Vec<Derivation>,
        all: &mut Vec<Derivation>,
    ) {
        if found.is_empty() {
            return;
        }
        let preferred = prefer::preferred(self.grammar, alternative, value);
        // The card's best word for itself, where its context says what it is. A word for
        // something it is not, as `this Saga` is for a creature, misnames it as well as
        // passing over that word.
        // A seeded pronoun for the card is a word for it too: `on her` passes over
        // `this land`.
        let best = self.best[self.context];
        let word = self.grammar.words.of(step.0, &self.contexts[self.context]);
        let misnames = u32::from(word == Some(0) && best > 0);
        let word = word.unwrap_or(0);
        let closes = self.grammar.rules[step.0 as usize].sem.is_some();
        let pronoun = self.grammar.words.by_pronoun(step);
        let best = (closes && (words::is_the_card(value) || pronoun)).then_some(best);
        let leaf = Rope::leaf(step);
        for mut derivation in found {
            derivation.productions = derivation.productions.join(&leaf);
            derivation.preferred += preferred;
            derivation.missed += misnames;
            derivation.word = derivation.word.max(word);
            if let Some(best) = best
                && derivation.references == 0
            {
                derivation.missed += u32::from(derivation.word < best);
                derivation.references = 1;
            }
            if closes {
                derivation.close(step.0, step.1);
            }
            all.push(derivation);
        }
    }

    fn passthrough(
        &mut self,
        alternative: &Alternative,
        value: &Value,
        at: (u32, &Reach),
    ) -> Vec<Derivation> {
        match alternative.syms.first() {
            Some(Sym::Rule(rule)) => self.render_id(*rule, value, at).to_vec(),
            Some(sym @ Sym::Class(_)) => class_token(sym, value)
                .map(|token| {
                    let value = token_value(&token);
                    Derivation::token(token, value)
                })
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }

    fn alternative(
        &mut self,
        alternative: &Alternative,
        bindings: &Bindings<'_, '_>,
        contexts: &[(&str, Vec<String>)],
    ) -> Vec<Derivation> {
        self.work.walked += 1;
        let limit = self.cap * WIDEN;
        let mut partial = vec![Derivation::default()];
        for (index, (sym, hole)) in alternative.syms.iter().zip(&alternative.holes).enumerate() {
            let value = hole.as_ref().and_then(|name| bindings.get(name));
            let outer = self.context;
            if let Some((_, context)) = contexts
                .iter()
                .find(|(name, _)| hole.as_deref() == Some(name))
            {
                self.context = self.intern_context(context.clone());
            }
            let mut pieces = self.symbol(sym, value);
            self.context = outer;
            // The product has to fit, so when it does not, keep the best of each side.
            if partial.len() * pieces.len() > limit {
                self.truncated = true;
                partial = self.ranked_heads(partial);
                pieces = Rc::new(self.ranked(pieces.to_vec()));
            }
            let mut next = Vec::with_capacity(limit.min(partial.len() * pieces.len()));
            'heads: for head in &partial {
                for piece in pieces.iter() {
                    if next.len() >= limit {
                        break 'heads;
                    }
                    self.work.joined += 1;
                    next.push(head.then(piece, index));
                }
            }
            partial = next;
            if partial.is_empty() {
                break;
            }
        }
        partial
    }

    fn symbol(&mut self, sym: &Sym, value: Option<&Value>) -> Found {
        match sym {
            Sym::Word(word) => one(word_token(word), None),
            Sym::Mark(mark) => one(Token::Mark(*mark), None),
            Sym::Class(Class::Newline) => one(Token::Newline, None),
            Sym::Class(Class::SelfName) => {
                let token = Token::SelfName(self.self_name);
                let value = token_value(&token);
                one(token, value)
            }
            Sym::Class(_) => match value.and_then(|v| class_token(sym, v)) {
                Some(token) => {
                    let value = token_value(&token);
                    one(token, value)
                }
                None => Rc::new(Vec::new()),
            },
            Sym::Rule(rule) => self.derive(*rule, value.unwrap_or(&Value::Null)),
        }
    }
}

/// Whether a value can render as exactly these tokens. This proves the inverse
/// reaches a text without enumerating every other text on the way.
pub struct Matcher<'a> {
    grammar: &'a Grammar,
    target: &'a [Token],
    values: Values,
    memo: HashMap<(RuleId, usize, u32), Rc<Vec<usize>>>,
    nothing: Rc<Vec<usize>>,
}

type Open = HashSet<(RuleId, usize, usize, u32)>;

impl<'a> Matcher<'a> {
    pub fn new(grammar: &'a Grammar, target: &'a [Token]) -> Self {
        Self {
            grammar,
            target,
            values: Values::default(),
            memo: HashMap::default(),
            nothing: Rc::new(Vec::new()),
        }
    }

    pub fn reaches(&mut self, rule: RuleId, value: &Value) -> bool {
        self.ends(rule, value, 0).contains(&self.target.len())
    }

    /// The alternatives of one derivation of the whole target.
    pub fn derivation(&mut self, rule: RuleId, value: &Value) -> Vec<(RuleId, usize)> {
        let mut steps = Vec::new();
        let mut open = HashSet::default();
        self.pick(rule, value, (0, self.target.len()), &mut steps, &mut open);
        steps
    }

    fn pick(
        &mut self,
        rule: RuleId,
        value: &Value,
        span: (usize, usize),
        steps: &mut Vec<(RuleId, usize)>,
        open: &mut Open,
    ) -> bool {
        let (id, reach) = self.values.id(&self.grammar.net, value);
        self.pick_id(rule, (value, id, &reach), span, steps, open)
    }

    fn pick_id(
        &mut self,
        rule: RuleId,
        (value, id, reach): (&Value, u32, &Reach),
        span: (usize, usize),
        steps: &mut Vec<(RuleId, usize)>,
        open: &mut Open,
    ) -> bool {
        let key = (rule, span.0, span.1, id);
        let ends = self.ends_id(rule, (value, id, reach), span.0);
        if !ends.contains(&span.1) || !open.insert(key) {
            return false;
        }
        let grammar = self.grammar;
        let definition = &grammar.rules[rule as usize];
        let held = steps.len();
        let mut found = false;
        match &definition.sem {
            Some(pattern) => {
                let solutions = pattern.unify(value, Bindings::new());
                'alternatives: for (index, alternative) in
                    definition.alternatives.iter().enumerate()
                {
                    for bindings in &solutions {
                        if self.pick_from(alternative, bindings, (0, span), steps, open) {
                            steps.push((rule, index));
                            found = true;
                            break 'alternatives;
                        }
                        steps.truncate(held);
                    }
                }
            }
            None => {
                for index in grammar.net.alternatives(reach, rule) {
                    found = match definition.alternatives[*index as usize].syms.first() {
                        Some(Sym::Rule(child)) => {
                            self.pick_id(*child, (value, id, reach), span, steps, open)
                        }
                        Some(sym) => self.terminal(sym, Some(value), span.0) == Some(span.1),
                        None => false,
                    };
                    if found {
                        steps.push((rule, *index as usize));
                        break;
                    }
                    steps.truncate(held);
                }
            }
        }
        open.remove(&key);
        found
    }

    /// Walk an alternative's symbols from `from.0` across the span `from.1`.
    fn pick_from(
        &mut self,
        alternative: &Alternative,
        bindings: &Bindings<'_, '_>,
        from: (usize, (usize, usize)),
        steps: &mut Vec<(RuleId, usize)>,
        open: &mut Open,
    ) -> bool {
        let (index, (at, end)) = from;
        let Some(sym) = alternative.syms.get(index) else {
            return at == end;
        };
        let hole = alternative.holes.get(index).and_then(Option::as_ref);
        let value = hole.and_then(|name| bindings.get(name));
        let held = steps.len();
        for to in self.step(sym, value, at) {
            let walked = match sym {
                Sym::Rule(child) => {
                    let inner = value.unwrap_or(&Value::Null);
                    self.pick(*child, inner, (at, to), steps, open)
                }
                _ => true,
            };
            if walked && self.pick_from(alternative, bindings, (index + 1, (to, end)), steps, open)
            {
                return true;
            }
            steps.truncate(held);
        }
        false
    }

    fn step(&mut self, sym: &Sym, value: Option<&Value>, at: usize) -> Vec<usize> {
        match sym {
            Sym::Rule(rule) => self.ends(*rule, value.unwrap_or(&Value::Null), at).to_vec(),
            other => self.terminal(other, value, at).into_iter().collect(),
        }
    }

    fn ends(&mut self, rule: RuleId, value: &Value, at: usize) -> Rc<Vec<usize>> {
        let (id, reach) = self.values.id(&self.grammar.net, value);
        self.ends_id(rule, (value, id, &reach), at)
    }

    fn ends_id(
        &mut self,
        rule: RuleId,
        (value, id, reach): (&Value, u32, &Reach),
        at: usize,
    ) -> Rc<Vec<usize>> {
        if !self.grammar.net.reaches(reach, rule) {
            return Rc::clone(&self.nothing);
        }
        if let Some(found) = self.memo.get(&(rule, at, id)) {
            return Rc::clone(found);
        }
        self.memo.insert((rule, at, id), Rc::clone(&self.nothing));
        let grammar = self.grammar;
        let definition = &grammar.rules[rule as usize];
        let mut out = Vec::new();
        match &definition.sem {
            Some(pattern) => {
                let solutions = pattern.unify(value, Bindings::new());
                for alternative in &definition.alternatives {
                    for bindings in &solutions {
                        out.extend(self.sequence(alternative, bindings, at));
                    }
                }
            }
            None => {
                for index in grammar.net.alternatives(reach, rule) {
                    match definition.alternatives[*index as usize].syms.first() {
                        Some(Sym::Rule(child)) => {
                            let ends = self.ends_id(*child, (value, id, reach), at);
                            out.extend(ends.iter().copied());
                        }
                        Some(sym) => out.extend(self.terminal(sym, Some(value), at)),
                        None => {}
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        let found = Rc::new(out);
        self.memo.insert((rule, at, id), Rc::clone(&found));
        found
    }

    fn sequence(
        &mut self,
        alternative: &Alternative,
        bindings: &Bindings<'_, '_>,
        at: usize,
    ) -> Vec<usize> {
        let mut positions = vec![at];
        for (sym, hole) in alternative.syms.iter().zip(&alternative.holes) {
            let value = hole.as_ref().and_then(|name| bindings.get(name));
            let mut next = Vec::new();
            for position in positions {
                match sym {
                    Sym::Rule(rule) => next.extend(
                        self.ends(*rule, value.unwrap_or(&Value::Null), position)
                            .iter()
                            .copied(),
                    ),
                    other => next.extend(self.terminal(other, value, position)),
                }
            }
            next.sort_unstable();
            next.dedup();
            positions = next;
            if positions.is_empty() {
                break;
            }
        }
        positions
    }

    fn terminal(&self, sym: &Sym, value: Option<&Value>, at: usize) -> Option<usize> {
        let token = self.target.get(at)?;
        let wanted = match sym {
            Sym::Word(word) => word_token(word),
            Sym::Mark(mark) => Token::Mark(*mark),
            Sym::Class(Class::Newline) => Token::Newline,
            Sym::Class(Class::SelfName) => {
                return matches!(token, Token::SelfName(_)).then_some(at + 1);
            }
            Sym::Class(_) => class_token(sym, value?)?,
            Sym::Rule(_) => return None,
        };
        (wanted == *token).then_some(at + 1)
    }
}

pub(crate) fn word_token(word: &str) -> Token {
    word.parse()
        .map_or_else(|_| Token::Word(word.to_string()), Token::Number)
}

fn one(token: Token, value: Option<Value>) -> Found {
    Rc::new(vec![Derivation::token(token, value)])
}

/// The value a token gives its production, as the parser reads it.
fn token_value(token: &Token) -> Option<Value> {
    match token {
        Token::Number(number) | Token::Grouped(number) => Some(Value::from(*number)),
        Token::Word(word) => Some(Value::from(word.clone())),
        Token::SelfName(_) => Some(Value::from("$self")),
        Token::Mark(_) | Token::Newline => None,
    }
}

pub(crate) fn class_token(sym: &Sym, value: &Value) -> Option<Token> {
    match (sym, value) {
        (Sym::Class(Class::Number), Value::Number(number)) => {
            Some(Token::Number(u32::try_from(number.as_u64()?).ok()?))
        }
        (Sym::Class(Class::Word | Class::Capitalized), Value::String(word)) => {
            Some(Token::Word(word.clone()))
        }
        (Sym::Class(Class::SelfName), _) => Some(Token::SelfName(0)),
        _ => None,
    }
}
