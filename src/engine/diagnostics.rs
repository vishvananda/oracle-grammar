//! Optional evidence from the original parse; never a second recognition pass.
use super::{Engine, Parsed, Refusal, bind_surface, canonical_abilities, validate_abilities};
use crate::parse::{Counters, Parser, RuleSet};
use crate::tokens::{self, Names, Token};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Lexical,
    Syntax,
    Binding,
    BoundValidation,
    Canonicalization,
    CanonicalValidation,
    ResourceLimit,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub stage: Stage,
    pub candidate: Option<usize>,
    pub message: String,
    pub message_truncated: bool,
    /// Original syntactic proposal, before binding; never a valid reading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_ir: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_path: Option<String>,
    pub candidate_ir_truncated: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct Evidence {
    pub tokens: usize,
    pub furthest: usize,
    pub frontier: String,
    pub expected: Vec<String>,
    pub expected_truncated: bool,
    pub counters: Counters,
    pub unbound_candidates: usize,
    pub failures: Vec<Diagnostic>,
    pub failures_truncated: bool,
}

impl Evidence {
    fn record(&mut self, stage: Stage, candidate: Option<usize>, message: &str) {
        if self.failures.len() == 64 {
            self.failures_truncated = true;
            return;
        }
        let end = message
            .char_indices()
            .map(|(at, _)| at)
            .find(|at| *at >= 4096);
        self.failures.push(Diagnostic {
            stage,
            candidate,
            message: message[..end.unwrap_or(message.len())].into(),
            message_truncated: end.is_some(),
            candidate_ir: None,
            candidate_path: None,
            candidate_ir_truncated: false,
        });
    }
}

#[derive(Debug)]
pub struct Attempt {
    pub result: Result<Parsed, Refusal>,
    pub evidence: Evidence,
}

impl Engine {
    pub fn inspect(&self, name: &str, text: &str) -> Attempt {
        self.inspect_without(name, text, &RuleSet::default())
    }

    /// Capture bounded original proposals from the same recognition pass.
    /// The path names the proposal root, not a precise validation-error location.
    pub fn inspect_with_candidate_ir(&self, name: &str, text: &str) -> Attempt {
        let mut evidence = Evidence::default();
        let result = run(
            self,
            name,
            text,
            &RuleSet::default(),
            true,
            true,
            &mut evidence,
        );
        Attempt { result, evidence }
    }

    pub fn inspect_without(&self, name: &str, text: &str, off: &RuleSet) -> Attempt {
        parse(self, name, text, off, true)
    }

    /// Format a previously returned NoParse position, without invoking recognition.
    pub fn frontier_at(&self, name: &str, text: &str, at: usize) -> String {
        let names = Names::of(name);
        tokens::tokenize(text, &names, &self.grammar.vocabulary)
            .map(|tokens| frontier(&tokens, &names, at))
            .unwrap_or_default()
    }
}

pub(super) fn frontier(tokens: &[Token], names: &Names, at: usize) -> String {
    let at = at.min(tokens.len());
    let before = tokens::detokenize(&tokens[at.saturating_sub(3)..at], names);
    let after = tokens::detokenize(&tokens[at..(at + 2).min(tokens.len())], names);
    format!("{before} ^ {after}").replace('\n', " / ")
}

pub(super) fn bind(value: &Value, name: &str) -> Result<Value, (Stage, String)> {
    let bound = bind_surface(value, name).map_err(|why| (Stage::Binding, why))?;
    validate_abilities(&bound)
        .map_err(|why| (Stage::BoundValidation, format!("bound IR: {why}")))?;
    let canonical = canonical_abilities(&bound).map_err(|why| (Stage::Canonicalization, why))?;
    validate_abilities(&canonical)
        .map_err(|why| (Stage::CanonicalValidation, format!("canonical IR: {why}")))?;
    Ok(canonical)
}

pub(super) fn parse(
    engine: &Engine,
    name: &str,
    text: &str,
    off: &RuleSet,
    capture: bool,
) -> Attempt {
    let mut evidence = Evidence::default();
    let result = run(engine, name, text, off, capture, false, &mut evidence);
    Attempt { result, evidence }
}

fn run(
    engine: &Engine,
    name: &str,
    text: &str,
    off: &RuleSet,
    capture: bool,
    candidate_ir: bool,
    evidence: &mut Evidence,
) -> Result<Parsed, Refusal> {
    let names = Names::of(name);
    let tokens = tokens::tokenize(text, &names, &engine.grammar.vocabulary).map_err(|why| {
        if capture {
            evidence.record(Stage::Lexical, None, &why);
        }
        Refusal::Lexical(why)
    })?;
    let mut parser = Parser::new(&engine.grammar, &tokens, engine.limits).without(off);
    if capture {
        parser = parser.with_diagnostics();
    }
    let values = parser.whole(engine.root);
    if capture {
        evidence.tokens = tokens.len();
        evidence.furthest = parser.furthest;
        evidence.frontier = frontier(&tokens, &names, parser.furthest);
        evidence.expected = parser.expected.clone();
        evidence.expected_truncated = parser.expected_truncated;
        evidence.counters = parser.counters.clone();
    }
    let values = values.map_err(|why| {
        if capture {
            evidence.record(Stage::ResourceLimit, None, &why);
        }
        Refusal::Limit(why)
    })?;
    if capture {
        evidence.unbound_candidates = values.len();
    }
    if values.is_empty() {
        if capture {
            evidence.record(Stage::Syntax, None, "no whole-input reading");
        }
        return Err(Refusal::NoParse {
            furthest: parser.furthest,
            tokens: tokens.len(),
        });
    }
    let contextual_station = values
        .iter()
        .any(|value| super::card::station::requires_context(value));
    let explicit_station = if contextual_station {
        engine
            .parse_card_without(
                name,
                text,
                &super::card::station::empty_characteristics(),
                off,
            )
            .ok()
            .map(|card| {
                card.readings
                    .into_iter()
                    .map(|reading| reading.abilities)
                    .collect::<Vec<_>>()
            })
    } else {
        None
    };
    let mut readings = Vec::new();
    let mut candidate_bytes_left = 64 * 1024;
    let mut failure = None;
    for (index, value) in values.iter().enumerate() {
        let bound = bind(value, name).and_then(|bound| {
            if contextual_station
                && super::card::station::requires_context(&bound)
                && !explicit_station
                    .as_ref()
                    .is_some_and(|readings| readings.contains(&bound))
            {
                Err((
                    Stage::Binding,
                    "omitted Station P/T requires printed source context".into(),
                ))
            } else {
                Ok(bound)
            }
        });
        match bound {
            Ok(bound) if !readings.contains(&bound) => {
                if readings.len() >= engine.reading_limit {
                    if capture {
                        evidence.record(Stage::ResourceLimit, Some(index), "bound reading limit");
                    }
                    return Err(Refusal::Limit("bound reading limit".into()));
                }
                readings.push(bound);
            }
            Ok(_) => {}
            Err((stage, why)) => {
                if capture {
                    let before = evidence.failures.len();
                    evidence.record(stage, Some(index), &why);
                    if candidate_ir && let Some(diagnostic) = evidence.failures.get_mut(before) {
                        diagnostic.candidate_ir =
                            bounded_ir(value, candidate_bytes_left.min(16 * 1024));
                        if let Some(ir) = &diagnostic.candidate_ir {
                            candidate_bytes_left -=
                                serde_json::to_vec(ir).expect("JSON value").len();
                        }
                        diagnostic.candidate_ir_truncated = diagnostic.candidate_ir.is_none();
                        diagnostic.candidate_path = Some("$".into());
                    }
                }
                failure = Some(why);
            }
        }
    }
    if readings.is_empty() {
        return Err(Refusal::Binding(failure.unwrap_or_default()));
    }
    Ok(Parsed {
        readings,
        counters: parser.counters.clone(),
    })
}

/// Copy JSON only when its compact encoding fits the byte bound. This does not
/// alter the value, prune operands, or substitute a partial reading.
pub fn bounded_ir(value: &Value, max_bytes: usize) -> Option<Value> {
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other("diagnostic IR byte limit"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded {
        bytes: Vec::new(),
        limit: max_bytes,
    };
    serde_json::to_writer(&mut output, value).ok()?;
    serde_json::from_slice(&output.bytes).ok()
}
