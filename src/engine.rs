//! The whole pipeline for one card, in both directions.
use crate::bind;
use crate::grammar::{self, Grammar, Sym};
use crate::model::{canonical, ir::Ability, validate};
use crate::parse::{Counters, Limits, Parser, RuleSet};
use crate::render::Renderer;
use crate::style::Model;
use crate::tokens::{self, Names};
use serde_json::{Value, json};

mod card;
#[cfg(test)]
mod spell_neighbors_tests;
pub use card::{
    CardInputContext, CardNote, CardReading, ParsedCard, PrintedPowerToughness, StationAnimationBox,
};
/// Empty native characteristics for source-context discovery without inventing stats.
pub fn empty_card_characteristics() -> crate::model::ir::Characteristics {
    card::station::empty_characteristics()
}
mod presentation;
mod printing;
pub use printing::Enumerated;

pub mod diagnostics;

/// A reading, with the mined rules behind each of its abilities.
pub type Audited = (Value, Vec<Vec<String>>);

const LEXICAL: &str = include_str!("../grammar/lexical.ebnf");
/// How often each rule spells the corpus, from `print-order`. Stale names do nothing.
const PRINT_MODEL: &str = include_str!("../grammar/print-model.txt");
const PRINT_WEIGHTS: &str = include_str!("../grammar/print-weights.txt");

// Syntax is input data: editing EBNF must not invalidate compiled Rust.
const SYNTAX_MODULES: &[(&str, &str)] = &[
    ("card", "syntax/card.ebnf"),
    ("card_notes", "syntax/card_notes.ebnf"),
    ("actions", "syntax/actions.ebnf"),
    ("action_reminders", "syntax/action_reminders.ebnf"),
    ("conditions", "syntax/conditions.ebnf"),
    ("continuous", "syntax/continuous.ebnf"),
    ("animation_types", "syntax/animation_types.ebnf"),
    ("costs", "syntax/costs.ebnf"),
    ("cost_reminders", "syntax/cost_reminders.ebnf"),
    ("copy_next", "syntax/copy_next.ebnf"),
    ("copy_entry", "syntax/copy_entry.ebnf"),
    ("defender", "syntax/defender.ebnf"),
    ("library", "syntax/library.ebnf"),
    ("land_mana", "syntax/land_mana.ebnf"),
    ("mechanics", "syntax/mechanics.ebnf"),
    ("replacements", "syntax/replacements.ebnf"),
    ("counters_named", "syntax/counters_named.ebnf"),
    ("effects", "syntax/effects.ebnf"),
    ("energy", "syntax/energy.ebnf"),
    ("events", "syntax/events.ebnf"),
    ("keywords", "syntax/keywords.ebnf"),
    ("keyword_actions", "syntax/keyword_actions.ebnf"),
    ("cost_branches", "syntax/cost_branches.ebnf"),
    ("cost_continuations", "syntax/cost_continuations.ebnf"),
    ("face_cost_reminders", "syntax/face_cost_reminders.ebnf"),
    ("labels", "syntax/labels.ebnf"),
    ("reminder_groups", "syntax/reminder_groups.ebnf"),
    ("reminders", "syntax/reminders.ebnf"),
    ("players", "syntax/players.ebnf"),
    ("mined", "syntax/mined.ebnf"),
    ("tail", "tail/mined.ebnf"),
    ("nouns", "syntax/nouns.ebnf"),
    ("numbers", "syntax/numbers.ebnf"),
    ("sagas", "syntax/sagas.ebnf"),
    ("subtypes", "syntax/subtypes.ebnf"),
    ("tokens", "syntax/tokens.ebnf"),
    ("quoted", "syntax/quoted.ebnf"),
    ("comparisons", "syntax/comparisons.ebnf"),
];

fn read_module(path: &str) -> Result<String, String> {
    let root = std::env::var_os("ORACLE_GRAMMAR_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("grammar"));
    let path = root.join(path);
    std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))
}

/// Read one declared syntax module from the current grammar input directory.
pub fn module_source(name: &str) -> Result<String, String> {
    let (_, path) = SYNTAX_MODULES
        .iter()
        .find(|(module, _)| *module == name)
        .ok_or_else(|| format!("unknown syntax module {name}"))?;
    read_module(path)
}

fn syntax(replaced: &[(String, String)]) -> Result<Vec<(String, String)>, String> {
    let mut modules = Vec::new();
    for (name, path) in SYNTAX_MODULES {
        let text = match replaced.iter().rev().find(|(module, _)| module == name) {
            Some((_, text)) => text.clone(),
            None => read_module(path)?,
        };
        modules.push((name.to_string(), text));
    }
    for (name, text) in replaced {
        if !SYNTAX_MODULES.iter().any(|(module, _)| module == name) {
            match modules.iter_mut().find(|(module, _)| module == name) {
                Some(module) => module.1 = text.clone(),
                None => modules.push((name.clone(), text.clone())),
            }
        }
    }
    Ok(modules)
}

pub struct Engine {
    pub grammar: Grammar,
    style: Model,
    root: u32,
    pub limits: Limits,
    /// Distinct validated meanings; independent of the parser's unbound values.
    pub reading_limit: usize,
}

/// Why a text has no reading.
#[derive(Debug, Clone)]
pub enum Refusal {
    Lexical(String),
    Limit(String),
    NoParse { furthest: usize, tokens: usize },
    Binding(String),
}

#[derive(Debug)]
pub struct Parsed {
    /// Distinct canonical readings. More than one is reported, never guessed.
    pub readings: Vec<Value>,
    pub counters: Counters,
}

impl Engine {
    pub fn load() -> Result<Self, String> {
        Self::load_with(&[])
    }

    /// The grammar with some modules read from elsewhere, to try rules without a rebuild.
    /// Load with any `--module=name=path` arguments, which replace those modules.
    pub fn load_from_args() -> Result<Self, String> {
        let mut modules = Vec::new();
        for arg in std::env::args().skip(1) {
            if let Some(pair) = arg.strip_prefix("--module=") {
                let (name, path) = pair.split_once('=').ok_or("expected --module=name=path")?;
                let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                modules.push((name.to_string(), text));
            }
        }
        Self::load_with(&modules)
    }

    pub fn load_with(replaced: &[(String, String)]) -> Result<Self, String> {
        let schema = card::schema()?;
        let modules = syntax(replaced)?;
        let syntax: Vec<(&str, &str)> = modules
            .iter()
            .map(|(n, t)| (n.as_str(), t.as_str()))
            .collect();
        let grammar = grammar::load(LEXICAL, &syntax, &schema)?;
        let root = *grammar.names.get("card").ok_or("no card rule")?;
        Ok(Self {
            grammar,
            style: print_style(),
            root,
            limits: Limits::default(),
            reading_limit: 256,
        })
    }

    pub fn parse(&self, name: &str, text: &str) -> Result<Parsed, Refusal> {
        self.parse_without(name, text, &RuleSet::default())
    }

    /// How many productions the plain reader took, each checked against `tabnas-ebnf`.
    pub fn plain_reader_agrees() -> Result<usize, String> {
        let modules = syntax(&[])?;
        let syntax: Vec<(&str, &str)> = modules
            .iter()
            .map(|(n, t)| (n.as_str(), t.as_str()))
            .collect();
        grammar::plain_reader_agrees(LEXICAL, &syntax)
    }

    /// Print by these learned weights instead of the built-in table.
    pub fn relearn(&mut self, table: &str) {
        self.style.learn(table);
    }

    pub fn rule_count(&self) -> usize {
        self.grammar.rules.len()
    }

    /// The rules whose names `wanted` accepts.
    pub fn rule_set(&self, wanted: impl Fn(&str) -> bool) -> RuleSet {
        (0u32..)
            .zip(&self.grammar.rules)
            .filter(|(_, rule)| wanted(&rule.name))
            .map(|(id, _)| id)
            .collect()
    }

    /// Parse as if the rules in `off` were not in the grammar.
    pub fn parse_without(&self, name: &str, text: &str, off: &RuleSet) -> Result<Parsed, Refusal> {
        diagnostics::parse(self, name, text, off, false).result
    }

    /// Every reading of a text, with the mined rules behind each of its abilities.
    /// Readings of tokens already made, or how far a parse got.
    pub fn read_tokens(&self, name: &str, tokens: &[tokens::Token]) -> Result<Vec<Value>, usize> {
        let mut parser = Parser::new(&self.grammar, tokens, self.limits);
        let values = parser.whole(self.root).unwrap_or_default();
        if values.is_empty() {
            return Err(parser.furthest.min(tokens.len().saturating_sub(1)));
        }
        let mut readings: Vec<Value> = Vec::new();
        for value in values {
            if let Ok(bound) = bind_card(&value, name)
                && !readings.contains(&bound)
            {
                if readings.len() >= self.reading_limit {
                    return Err(parser.furthest.min(tokens.len().saturating_sub(1)));
                }
                readings.push(bound);
            }
        }
        Ok(readings)
    }

    /// Incomplete proposals for the phrase miner, never accepted card readings.
    /// Gap sentinels deliberately have no corresponding complete IR variant.
    pub(crate) fn mining_tokens(
        &self,
        name: &str,
        tokens: &[tokens::Token],
    ) -> Result<Vec<Value>, usize> {
        let mut parser = Parser::new(&self.grammar, tokens, self.limits);
        let values = parser.whole(self.root).unwrap_or_default();
        if values.is_empty() {
            return Err(parser.furthest.min(tokens.len().saturating_sub(1)));
        }
        let mut proposals = Vec::new();
        for value in values {
            if let Ok(bound) =
                bind_surface(&value, name).and_then(|bound| canonical_abilities(&bound))
                && !proposals.contains(&bound)
            {
                proposals.push(bound);
            }
        }
        Ok(proposals)
    }

    /// Why a face prints or does not: the parser's own values for the same abilities,
    /// which the inverse of the grammar should always reach, against what the printer
    /// starts from, ability by ability.
    pub fn print_gap(&self, name: &str, text: &str, abilities: &Value) -> Result<Value, String> {
        let names = Names::of(name);
        let tokens = tokens::tokenize(text, &names, &self.grammar.vocabulary)?;
        let mut parser = Parser::new(&self.grammar, &tokens, self.limits);
        let wanted = canonical_abilities(abilities)?;
        let raw: Vec<Value> = parser
            .whole(self.root)?
            .iter()
            .map(|value| (**value).clone())
            .filter(|value| bind_card(value, name).is_ok_and(|bound| bound == wanted))
            .collect();
        let prints = |value: &Value| {
            let mut renderer = Renderer::new(&self.grammar, &self.style, 8, Vec::new());
            !renderer.render(self.root, value).is_empty()
        };
        let singly = |value: &Value| -> Vec<Value> {
            value
                .as_array()
                .into_iter()
                .flatten()
                .map(|item| {
                    json!({
                        "kind": item["kind"],
                        "prints": prints(&json!([item])),
                        "value": item,
                    })
                })
                .collect()
        };
        let surfaces = Self::surfaces(abilities)?;
        let reaches = |value: &Value| {
            crate::render::Matcher::new(&self.grammar, &tokens).reaches(self.root, value)
        };
        Ok(json!({
            "raw_prints": raw.iter().any(prints),
            "surface_prints": surfaces.iter().any(prints),
            "raw_reaches": raw.iter().any(reaches),
            "surface_reaches": surfaces.iter().any(reaches),
            "raw": raw.first().map(&singly),
            "surface": surfaces.first().map(&singly),
        }))
    }

    /// Mined rules with no nonterminal: sentences, not phrases.
    pub fn literal_rules(&self) -> std::collections::HashSet<String> {
        self.grammar
            .rules
            .iter()
            .filter(|rule| rule.name.starts_with("m_") || rule.name.starts_with("t_"))
            .filter(|rule| {
                rule.alternatives
                    .iter()
                    .all(|a| !a.syms.iter().any(|sym| matches!(sym, Sym::Rule(_))))
            })
            .map(|rule| rule.name.clone())
            .collect()
    }

    /// Readings with the mined rules behind each ability, as if the rules in `off` were
    /// not in the grammar. Tags keep readings of one value apart, so a card can hit the
    /// reading limit here and not in [`Engine::parse`].
    pub fn audited(&self, name: &str, text: &str, off: &RuleSet) -> Result<Vec<Audited>, String> {
        let names = Names::of(name);
        let Ok(tokens) = tokens::tokenize(text, &names, &self.grammar.vocabulary) else {
            return Ok(Vec::new());
        };
        let mut parser = Parser::new(&self.grammar, &tokens, self.limits)
            .without(off)
            .with_audit();
        let values = parser.whole(self.root)?;
        let mut out = Vec::new();
        for value in values {
            let mut plain = (*value).clone();
            let mut rules = Vec::new();
            if let Value::Array(items) = &mut plain {
                for item in items {
                    let mut names = Vec::new();
                    crate::parse::take_tags(item, &mut names);
                    rules.push(names);
                }
            }
            if let Ok(bound) = bind_card(&plain, name) {
                out.push((bound, rules));
            }
        }
        Ok(out)
    }
}

/// Ordinary spell paragraphs share a resolution. A modal announcement owns
/// its selected programs; neighboring unconditional instructions remain separate.
fn merge_spells(items: Vec<Value>) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::with_capacity(items.len());
    for item in items {
        if item["kind"] == "Spell"
            && item["args"]["announcement"]["modes"].is_null()
            && let Some(previous) = merged.last_mut()
            && previous["kind"] == "Spell"
            && previous["args"]["announcement"]["modes"].is_null()
            && let (Some(list), Some(more)) = (
                previous["args"]["body"]["instructions"].as_array_mut(),
                item["args"]["body"]["instructions"].as_array(),
            )
        {
            list.extend(more.iter().cloned());
            continue;
        }
        merged.push(item);
    }
    merged
}

fn bind_card(value: &Value, name: &str) -> Result<Value, String> {
    diagnostics::bind(value, name).map_err(|(_, why)| why)
}

fn bind_surface(value: &Value, name: &str) -> Result<Value, String> {
    let mut bound = match value {
        Value::Array(items) => Value::Array(merge_spells(items.clone())),
        other => other.clone(),
    };
    let linked_card_sites = bind::linked_card_actions::prepare(&mut bound)?;
    let attachment = bind::attachment(&bound);
    if let Value::Array(items) = &mut bound {
        for item in items {
            bind::bind_held_ability(item, name, attachment)?;
        }
    }
    bind::linked_card_actions::lower(&mut bound, &linked_card_sites)?;
    bind::chosen_kinds::check(&bound)?;
    Ok(bound)
}

/// Validate a complete ability list in its own casting-cost namespace, without
/// normalizing it or inventing printed card characteristics.
pub fn validate_abilities(value: &Value) -> Result<(), String> {
    let abilities: Vec<Ability> =
        serde_json::from_value(value.clone()).map_err(|e| format!("typed abilities: {e}"))?;
    validate::abilities(&abilities)
}

fn print_style() -> Model {
    let mut style = Model::parse(PRINT_MODEL);
    style.learn(PRINT_WEIGHTS);
    style
}

pub fn canonical_abilities(value: &Value) -> Result<Value, String> {
    let mut copy = value.clone();
    canonical::abilities(&mut copy)?;
    Ok(copy)
}

/// Meaning comparison erases presentation explicitly. Canonicalization and
/// print/reparse checks keep it, so metadata loss cannot pass an exact gate.
pub fn semantic_abilities(value: &Value) -> Result<Value, String> {
    let mut copy = value.clone();
    canonical::semantic_abilities(&mut copy)?;
    Ok(copy)
}

/// The grammar as one standard EBNF document, semantics left out.
pub fn export() -> String {
    let mut document = String::from(LEXICAL);
    let modules = syntax(&[]).expect("read grammar modules for export");
    for (_, module) in &modules {
        document.push('\n');
        let mut rest = module.as_str();
        while let Some(start) = rest.find("/* @sem") {
            document.push_str(&rest[..start]);
            let body = &rest[start..];
            let Some(end) = body.find("*/") else {
                rest = "";
                break;
            };
            rest = body[end + 2..].trim_start_matches('\n');
        }
        document.push_str(rest);
    }
    document
}

/// Remove only declared presentation labels using the built-in grammar.
/// Engine-bound tools use `Engine::presentation_free` so module overrides apply.
pub fn presentation_free(text: &str) -> String {
    presentation::builtin(text)
}

impl Engine {
    /// The token stream a card's text scans to.
    pub fn tokens_of(&self, name: &str, text: &str) -> Result<Vec<tokens::Token>, String> {
        tokens::tokenize(text, &Names::of(name), &self.grammar.vocabulary)
    }

    /// Whether the grammar derives these tokens, before any binding.
    pub fn recognizes(&self, tokens: &[tokens::Token]) -> Result<bool, String> {
        let mut parser = Parser::new(&self.grammar, tokens, self.limits);
        Ok(!parser.whole(self.root)?.is_empty())
    }
}

/// Where a phrase family matches inside a text the grammar may not parse whole.
pub struct Span {
    pub family: String,
    pub start: usize,
    pub end: usize,
    pub meaning: Value,
}

impl Engine {
    /// The phrases this grammar already knows inside a line, as evidence for a
    /// rule it does not have yet.
    pub fn known_spans(
        &self,
        tokens: &[tokens::Token],
        families: &[&str],
    ) -> Result<Vec<Span>, String> {
        let mut parser = Parser::new(&self.grammar, tokens, self.limits);
        let mut out = Vec::new();
        for family in families {
            let Some(rule) = self.grammar.names.get(*family).copied() else {
                continue;
            };
            for start in 0..tokens.len() {
                for reading in parser.readings_at(rule, start)? {
                    out.push(Span {
                        family: (*family).to_string(),
                        start,
                        end: reading.end,
                        meaning: without_positions(&reading.value),
                    });
                }
            }
        }
        Ok(out)
    }

    /// The first surface form of an ability: its references back to symbols.
    pub fn surface(ability: &Value) -> Result<Value, String> {
        let mut reduced = Value::Array(vec![ability.clone()]);
        canonical::reduced(&mut reduced)?;
        let form = bind::unbind_ability(&reduced[0]);
        form.into_iter()
            .next()
            .ok_or_else(|| "no surface form".to_string())
    }
}

/// A symbol's token position says where it was read, not what it means.
fn without_positions(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .filter(|(key, _)| !(key.as_str() == "at" && object.contains_key("$sym")))
                .map(|(key, item)| (key.clone(), without_positions(item)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_positions).collect()),
        other => other.clone(),
    }
}

/// One phrase of a card: the family, its words and what they can mean.
pub struct Phrase {
    pub family: String,
    pub words: String,
    pub meaning: Value,
}

impl Engine {
    /// Every reading of these rules in a card, whether or not the whole parse
    /// used it, so a phrase's shapes are the grammar's and not one derivation's.
    pub fn phrases(&self, name: &str, text: &str, families: &[&str]) -> Vec<Phrase> {
        let names = Names::of(name);
        let Ok(tokens) = tokens::tokenize(text, &names, &self.grammar.vocabulary) else {
            return Vec::new();
        };
        if tokens.len() > self.limits.tokens {
            return Vec::new();
        }
        let mut parser = Parser::new(&self.grammar, &tokens, self.limits);
        // Phrase evidence includes prefixes, even when they cannot finish a card.
        let _ = parser.readings_at(self.root, 0);
        let mut out = Vec::new();
        for family in families {
            let Some(rule) = self.grammar.names.get(*family).copied() else {
                continue;
            };
            for (start, end, value) in parser.spans(rule) {
                out.push(Phrase {
                    family: (*family).to_string(),
                    words: tokens::detokenize(&tokens[start..end], &names),
                    meaning: without_positions(&value),
                });
            }
        }
        out
    }

    /// The tokens around where recognition stopped, for triage.
    pub fn stuck_at(&self, name: &str, text: &str) -> String {
        self.stuck_without(name, text, &RuleSet::default())
    }

    /// Where a parse stops with the rules in `off` out of the grammar.
    pub fn stuck_without(&self, name: &str, text: &str, off: &RuleSet) -> String {
        self.stuck_root(name, text, off, self.root)
    }

    fn stuck_root(&self, name: &str, text: &str, off: &RuleSet, root: u32) -> String {
        let names = Names::of(name);
        let Ok(tokens) = tokens::tokenize(text, &names, &self.grammar.vocabulary) else {
            return String::new();
        };
        let mut parser = Parser::new(&self.grammar, &tokens, self.limits).without(off);
        let _ = parser.whole(root);
        let at = parser.furthest.min(tokens.len());
        let start = at.saturating_sub(3);
        let end = (at + 2).min(tokens.len());
        let before = tokens::detokenize(&tokens[start..at], &names);
        let after = tokens::detokenize(&tokens[at..end], &names);
        format!("{before} ^ {after}").replace('\n', " / ")
    }
}
