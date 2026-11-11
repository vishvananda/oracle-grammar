//! What the rule miners share: lining lines up with gold abilities, and reading a line
//! with one unknown phrase. A gap token stands for the phrase; a reading through it holds
//! the phrase family's sentinel, and the gold IR at that place is the phrase's meaning.
use crate::engine::{Engine, canonical_abilities};
use crate::tokens::Token;
use serde_json::{Value, json};
use std::collections::BTreeSet;

mod results;

/// Longest run of lines one ability can hold: a modal spell and its bullets, a saga.
const MOST_LINES: usize = 12;
/// Longest phrase a gap stands for.
const LONGEST_GAP: usize = 14;
/// Gaps tried before a second one, in order of length.
const SECOND_GAPS: usize = 8;
const GAP: char = '\u{25A1}';

struct Lines<'a> {
    lines: Vec<&'a str>,
    /// How many abilities a line holds when it reads alone.
    alone: Vec<Option<usize>>,
    abilities: usize,
    dead: BTreeSet<(usize, usize)>,
}

impl Lines<'_> {
    /// Assign lines to abilities from line `line` and ability `at`. A line that reads alone
    /// keeps its count; a line that does not holds one ability, alone or with those after it.
    fn assign(&mut self, line: usize, at: usize) -> Option<Vec<(usize, usize, usize)>> {
        if line == self.lines.len() {
            return (at == self.abilities).then(Vec::new);
        }
        if at > self.abilities || self.dead.contains(&(line, at)) {
            return None;
        }
        if let Some(count) = self.alone[line]
            && let Some(rest) = self.assign(line + 1, at + count)
        {
            return Some(rest);
        }
        if at < self.abilities {
            for span in 1..=MOST_LINES.min(self.lines.len() - line) {
                if let Some(mut rest) = self.assign(line + span, at + 1) {
                    rest.push((line, line + span, at));
                    return Some(rest);
                }
            }
        }
        self.dead.insert((line, at));
        None
    }
}

/// Unread lines, each with the gold ability it holds; adjacent lines join when the IR
/// holds them as one ability.
pub fn line_rows(engine: &Engine, name: &str, text: &str, gold: &[Value]) -> Vec<(String, Value)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let alone = lines
        .iter()
        .map(|line| {
            let parsed = engine.parse(name, line).ok()?;
            Some(parsed.readings[0].as_array().map_or(1, Vec::len))
        })
        .collect();
    let mut plan = Lines {
        lines,
        alone,
        abilities: gold.len(),
        dead: BTreeSet::new(),
    };
    let Some(mut groups) = plan.assign(0, 0) else {
        return Vec::new();
    };
    groups.reverse();
    groups
        .into_iter()
        .map(|(start, end, at)| (plan.lines[start..end].join("\n"), gold[at].clone()))
        .collect()
}

/// Rule levels, in the order `grammar/mining/gaps.ebnf` numbers them.
const LEVELS: [&str; 16] = [
    "Condition",
    "Event",
    "Change",
    "Filter",
    "Plural",
    "Value",
    "Cards",
    "Ref",
    "Set",
    "Player",
    "Players",
    "Each",
    "Recipients",
    "Operation",
    "CostPart",
    "Instruction",
];

/// The rule level a sentinel belongs to.
fn sentinel(value: &Value) -> Option<&'static str> {
    if value["kind"] != "Gap" {
        return None;
    }
    let index = usize::try_from(value["args"].as_u64()?).ok()?;
    LEVELS.get(index).copied()
}

fn set_rest(kind: &str, ours: &[Value], gold: &[Value], out: &mut Vec<(String, Value)>) -> bool {
    let left: Vec<&Value> = ours.iter().filter(|o| !gold.contains(o)).collect();
    let rest: Vec<&Value> = gold.iter().filter(|g| !ours.contains(g)).collect();
    let [only] = left.as_slice() else {
        return false;
    };
    let (Some(level), false) = (sentinel(only), rest.is_empty()) else {
        return false;
    };
    let meaning = match rest.as_slice() {
        [one] => (*one).clone(),
        many => json!({"kind": kind, "args": many}),
    };
    out.push((level.to_string(), meaning));
    true
}

/// A list holding an instruction gap: what stands around the gap must match, and the gap
/// means the gold's middle.
fn unify_run(
    ours: &[Value],
    gold: &[Value],
    out: &mut Vec<(String, Value)>,
    unread: &BTreeSet<u64>,
) -> Option<bool> {
    let at = ours
        .iter()
        .position(|item| sentinel(item) == Some("Instruction"))?;
    let after = ours.len() - at - 1;
    if gold.len() < at + after + 1 {
        return Some(false);
    }
    let middle = gold[at..gold.len() - after].to_vec();
    let before = ours[..at]
        .iter()
        .zip(gold)
        .all(|(x, y)| match_value(x, y, out, unread));
    let tail = ours[at + 1..]
        .iter()
        .zip(&gold[gold.len() - after..])
        .all(|(x, y)| match_value(x, y, out, unread));
    out.push(("Instruction".to_string(), Value::Array(middle)));
    Some(before && tail)
}

/// A phrase through a gap can announce a target of its own, so the reading may hold fewer.
fn fewer_targets(ours: &[Value], gold: &[Value]) -> bool {
    let plain = |spec: &Value| {
        let mut copy = spec.clone();
        if let Some(fields) = copy.as_object_mut() {
            fields.remove("slot");
        }
        copy
    };
    let gold: Vec<Value> = gold.iter().map(plain).collect();
    ours.len() < gold.len() && ours.iter().all(|spec| gold.contains(&plain(spec)))
}

/// Match a reading through gaps against the gold, collecting each sentinel's meaning.
fn unify(ours: &Value, gold: &Value, out: &mut Vec<(String, Value)>) -> bool {
    match_value(ours, gold, out, &results::unread(ours))
}

fn match_value(
    ours: &Value,
    gold: &Value,
    out: &mut Vec<(String, Value)>,
    unread: &BTreeSet<u64>,
) -> bool {
    if let Some(level) = sentinel(ours) {
        out.push((level.to_string(), gold.clone()));
        return true;
    }
    if ours == gold {
        return true;
    }
    match (ours, gold) {
        (Value::Object(a), Value::Object(b)) => {
            if a.get("kind") == Some(&json!("Target")) && a.get("kind") == b.get("kind") {
                return true;
            }
            if let (Some(kind @ ("AllOf" | "AnyOf")), Some(x), Some(y)) = (
                a.get("kind").and_then(Value::as_str),
                a.get("args").and_then(Value::as_array),
                b.get("args").and_then(Value::as_array),
            ) && a.get("kind") == b.get("kind")
                && set_rest(kind, x, y, out)
            {
                return true;
            }
            a.len() == b.len()
                && a.iter().all(|(key, item)| {
                    b.get(key).is_some_and(|other| match key.as_str() {
                        // What reads a result can sit in the gap.
                        "result" if item.as_u64().is_some_and(|id| unread.contains(&id)) => {
                            other.is_u64()
                        }
                        "targets" => match (item.as_array(), other.as_array()) {
                            (Some(x), Some(y)) if fewer_targets(x, y) => true,
                            _ => match_value(item, other, out, unread),
                        },
                        _ => match_value(item, other, out, unread),
                    })
                })
        }
        (Value::Array(a), Value::Array(b)) => {
            if let Some(matched) = unify_run(a, b, out, unread) {
                return matched;
            }
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| match_value(x, y, out, unread))
        }
        _ => false,
    }
}

/// One phrase found in a line: its rule level, token span and meaning.
pub struct Phrase {
    pub level: String,
    pub start: usize,
    pub end: usize,
    pub meaning: Value,
}

fn gap_allowed(tokens: &[Token]) -> bool {
    tokens.last() != Some(&Token::Mark('.'))
        && !tokens.iter().any(|t| {
            matches!(
                t,
                Token::Newline | Token::Mark(':' | '"' | '\u{2022}' | '\u{2014}')
            )
        })
}

struct Search<'a> {
    engine: &'a Engine,
    name: &'a str,
    gold: &'a Value,
    /// A reading that got through but differs from the gold, for the report.
    near: std::cell::RefCell<Option<Value>>,
}

impl Search<'_> {
    /// The gaps around the place a parse stuck at, shortest first.
    fn spans(tokens: &[Token], stuck: usize) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        for length in 1..=LONGEST_GAP {
            for start in stuck.saturating_sub(length - 1)..=stuck {
                let end = start + length;
                if end <= tokens.len() && gap_allowed(&tokens[start..end]) {
                    spans.push((start, end));
                }
            }
        }
        spans
    }

    fn with_gap(tokens: &[Token], start: usize, end: usize) -> Vec<Token> {
        let mut out = tokens[..start].to_vec();
        out.push(Token::Mark(GAP));
        out.extend_from_slice(&tokens[end..]);
        out
    }

    /// Fill gaps left to right: `room` more may open, each around where the parse sticks.
    fn fill(&self, tokens: &[Token], room: usize) -> Option<Vec<(Vec<Token>, String, Value)>> {
        let stuck = match self.engine.mining_tokens(self.name, tokens) {
            Ok(readings) => {
                for reading in &readings {
                    let mut found = Vec::new();
                    if unify(reading, self.gold, &mut found) {
                        return Some(self.attach(tokens, found));
                    }
                }
                if let Some(first) = readings.first() {
                    self.near.borrow_mut().get_or_insert_with(|| first.clone());
                }
                return None;
            }
            Err(stuck) => stuck,
        };
        if room == 0 {
            return None;
        }
        let mut seconds = 0;
        for (start, end) in Self::spans(tokens, stuck) {
            let filled = Self::with_gap(tokens, start, end);
            match self.engine.mining_tokens(self.name, &filled) {
                Ok(_) => {
                    if let Some(found) = self.fill(&filled, room - 1) {
                        return Some(found);
                    }
                }
                Err(next) if next > start + 1 && seconds < SECOND_GAPS && room > 1 => {
                    seconds += 1;
                    if let Some(found) = self.fill(&filled, room - 1) {
                        return Some(found);
                    }
                }
                Err(_) => {}
            }
        }
        None
    }

    fn attach(
        &self,
        tokens: &[Token],
        found: Vec<(String, Value)>,
    ) -> Vec<(Vec<Token>, String, Value)> {
        found
            .into_iter()
            .map(|(level, meaning)| (tokens.to_vec(), level, meaning))
            .collect()
    }
}

/// Which spans of `original` the gaps in `filled` cover, in order.
fn gap_spans(original: &[Token], filled: &[Token]) -> Vec<(usize, usize)> {
    let (mut spans, mut at) = (Vec::new(), 0);
    for (index, token) in filled.iter().enumerate() {
        if *token != Token::Mark(GAP) {
            at += 1;
            continue;
        }
        // The tokens up to the next gap come after this one in both.
        let after = &filled[index + 1..];
        let plain = after
            .iter()
            .position(|t| *t == Token::Mark(GAP))
            .unwrap_or(after.len());
        let mut end = at + 1;
        while end < original.len() && !original[end..].starts_with(&after[..plain]) {
            end += 1;
        }
        spans.push((at, end));
        at = end;
    }
    spans
}

/// The unknown phrases of a line, when at most two gaps make it read as the gold.
pub fn phrases(engine: &Engine, name: &str, line: &str, ability: &Value) -> Vec<Phrase> {
    let Ok(tokens) = engine.tokens_of(name, line) else {
        return Vec::new();
    };
    let Ok(gold) = canonical_abilities(&Value::Array(vec![ability.clone()])) else {
        return Vec::new();
    };
    let search = Search {
        engine,
        name,
        gold: &gold,
        near: std::cell::RefCell::new(None),
    };
    let Some(found) = search.fill(&tokens, 2) else {
        if std::env::var_os("GAP_REPORT").is_some() {
            let near = search.near.borrow();
            eprintln!(
                "{}",
                json!({"name": name, "line": line, "gold": gold, "near": *near})
            );
        }
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some((filled, _, _)) = found.first() {
        let spans = gap_spans(&tokens, filled);
        if spans.len() == found.len() {
            for ((start, end), (_, level, meaning)) in spans.into_iter().zip(found) {
                out.push(Phrase {
                    level,
                    start,
                    end,
                    meaning,
                });
            }
        }
    }
    out
}
