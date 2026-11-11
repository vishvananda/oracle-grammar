//! Which mined rules read the corpus correctly: for each rule, the cards where a reading
//! that uses it equals the IR and the cards where one does not.
use oracle_ir_grammar::corpus;
use oracle_ir_grammar::engine::{Engine, canonical_abilities};
use oracle_ir_grammar::parse::RuleSet;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// What one card's equal readings depend on.
struct Tail {
    equal: bool,
    needs_tail: bool,
    needs_literal: bool,
    /// Equal, but the tagged parse hit a limit, so no rule took blame or credit from it.
    over_limit: bool,
    /// Equal, and the tagged parse finished without that reading.
    unmatched: bool,
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [unified, oracle, slice, out, rest @ ..] = args.as_slice() else {
        return Err(
            "usage: rule-audit <unified.jsonl> <oracle.json> <slice.json> <out.json> [module=path]..."
                .into(),
        );
    };
    let mut modules = Vec::new();
    for pair in rest {
        let (name, path) = pair.split_once('=').ok_or("expected module=path")?;
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        modules.push((name.to_string(), text));
    }
    let engine = Engine::load_with(&modules)?;
    let literal = engine.literal_rules();
    let tail_rules = engine.rule_set(|rule| rule.starts_with("t_"));
    let literal_rules = engine.rule_set(|rule| literal.contains(rule));
    let oracle = corpus::face_texts(oracle)?;
    let slice: Value = serde_json::from_slice(&std::fs::read(slice).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let wanted: BTreeSet<&str> = slice["cards"]
        .as_array()
        .ok_or("slice has no cards")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let mut wanted_faces = Vec::new();
    for face in corpus::faces(unified)? {
        let face = face?;
        if face["name"].as_str().is_some_and(|n| wanted.contains(n)) {
            wanted_faces.push(face);
        }
    }
    let judged = corpus::parallel(&wanted_faces, |face| {
        let name = face["name"].as_str().unwrap_or_default();
        let text = engine.presentation_free(oracle.get(name).map_or("", String::as_str));
        let gold = canonical_abilities(&face["abilities"])?;
        let (mut right, mut wrong) = (BTreeSet::new(), BTreeSet::new());
        let reads = |off: &RuleSet| {
            engine
                .parse_without(name, &text, off)
                .is_ok_and(|parsed| parsed.readings.contains(&gold))
        };
        let equal = reads(&RuleSet::default());
        let needs_tail = equal && !reads(&tail_rules);
        let needs_literal = equal && !reads(&literal_rules);
        let audited = engine.audited(name, &text);
        let over_limit = equal && audited.is_err();
        let mut matched = false;
        for (reading, rules) in audited.unwrap_or_default() {
            matched |= reading == gold;
            // Blame the abilities that differ, not the card's other lines.
            let aligned = reading.as_array().map_or(0, Vec::len) == rules.len()
                && gold.as_array().map_or(0, Vec::len) == rules.len();
            for (index, names) in rules.into_iter().enumerate() {
                if aligned && reading[index] == gold[index] {
                    right.extend(names);
                } else if aligned {
                    wrong.extend(names);
                }
            }
        }
        // A rule in a right reading of this card is not to blame for a wrong one beside it.
        let tail = Tail {
            equal,
            needs_tail,
            needs_literal,
            over_limit,
            unmatched: equal && !over_limit && !matched && !text.trim().is_empty(),
        };
        Ok::<_, String>((right.clone(), &wrong - &right, tail))
    })?;
    let (mut equal, mut needs_tail, mut needs_literal) = (0u64, 0u64, 0u64);
    let (mut over_limit, mut unmatched) = (0u64, 0u64);
    let mut tally: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for card in judged {
        let (right, wrong, tail) = card?;
        equal += u64::from(tail.equal);
        needs_tail += u64::from(tail.needs_tail);
        needs_literal += u64::from(tail.needs_literal);
        over_limit += u64::from(tail.over_limit);
        unmatched += u64::from(tail.unmatched);
        for rule in right {
            tally.entry(rule).or_default().0 += 1;
        }
        for rule in wrong {
            tally.entry(rule).or_default().1 += 1;
        }
    }
    let table: BTreeMap<&String, Value> = tally
        .iter()
        .map(|(rule, (right, wrong))| (rule, json!({"right": right, "wrong": wrong})))
        .collect();
    println!(
        "faces {} equal {equal} depend on the tail module {needs_tail} depend on a literal rule {needs_literal} unblamed: over the limit {over_limit} unmatched {unmatched}",
        wanted_faces.len()
    );
    std::fs::write(out, json!(table).to_string()).map_err(|e| e.to_string())
}

fn main() -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(run)
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "rule audit panicked".to_string())?
}
