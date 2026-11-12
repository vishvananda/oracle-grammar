//! Which seeded (`m_` and `t_`) rules the corpus needs. A face that matches with every
//! seeded rule off needs none. For the others, a tagged parse in the narrowest grammar that
//! still matches names the seeded rules behind the matching readings, and the face must then
//! match with every other seeded rule off. A seeded rule that no face needs can go without
//! losing a match, which `score` then confirms.
//!
//! Faces are read as `score` reads them, a Station card with its printed layout. The tagged
//! parse has no layout, so a Station card that needs seeded rules stays unresolved.
//!
//! `--credit` also tallies, per rule, the faces where a reading that uses it equals the IR
//! and the faces where one does not; that needs a tagged parse of every face.
use oracle_ir_grammar::corpus;
use oracle_ir_grammar::engine::{Audited, Engine, canonical_abilities};
use oracle_ir_grammar::grammar::RuleId;
use oracle_ir_grammar::parse::RuleSet;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// The rules switched off to try a face in a narrower grammar.
struct Grammars {
    seeded: RuleSet,
    tail_and_literal: RuleSet,
    tail: RuleSet,
    literal: RuleSet,
    whole: RuleSet,
    /// The rules of each seeded declaration, its anonymous groups included.
    groups: BTreeMap<String, Vec<RuleId>>,
}

impl Grammars {
    fn ids<'a>(
        &'a self,
        rules: impl IntoIterator<Item = &'a String>,
    ) -> impl Iterator<Item = RuleId> {
        rules
            .into_iter()
            .flat_map(|rule| self.groups.get(rule).into_iter().flatten().copied())
    }

    /// Every seeded rule off except `kept`.
    fn only(&self, kept: &BTreeSet<String>) -> RuleSet {
        let mut off = self.seeded.clone();
        for id in self.ids(kept) {
            off.remove(&id);
        }
        off
    }
}

#[path = "score/source.rs"]
mod source;

/// One face as `score` reads it.
struct Case<'a> {
    name: &'a str,
    text: String,
    gold: Value,
    input: source::Input,
}

impl<'a> Case<'a> {
    fn of(engine: &Engine, sources: &source::Sources, face: &'a Value) -> Result<Self, String> {
        let input = source::input(engine, sources, face)?;
        Ok(Case {
            name: face["name"].as_str().unwrap_or_default(),
            text: engine.presentation_free(&input.text),
            gold: canonical_abilities(&face["abilities"])?,
            input,
        })
    }

    fn reads(&self, engine: &Engine, off: &RuleSet) -> bool {
        let (name, text, gold) = (self.name, self.text.as_str(), &self.gold);
        if self.input.error.is_some() {
            false
        } else if self.input.context.station_present {
            engine
                .parse_card_without_source(name, text, &self.input.context, off)
                .is_ok_and(|parsed| parsed.readings.iter().any(|r| r.abilities == *gold))
        } else {
            engine
                .parse_without(name, text, off)
                .is_ok_and(|parsed| parsed.readings.contains(gold))
        }
    }
}

/// What one face's matching readings depend on.
#[derive(Default)]
struct Face {
    name: String,
    equal: bool,
    compositional: bool,
    needs_tail: bool,
    needs_literal: bool,
    /// Seeded rules, by declared name, behind the matching readings of the narrowest
    /// grammar that matches.
    needed: BTreeSet<String>,
    /// Why the tagged parse named no rules for an equal face that needs some.
    unresolved: Option<String>,
    /// Unresolved, and bisection found the unused rules it needs.
    rescued: bool,
    right: BTreeSet<String>,
    wrong: BTreeSet<String>,
    millis: u128,
}

/// The declared rule an anonymous group (`name#n`) belongs to.
fn declared(rule: &str) -> &str {
    rule.split_once('#').map_or(rule, |(name, _)| name)
}

fn seeded(rule: &str) -> bool {
    rule.starts_with("m_") || rule.starts_with("t_")
}

fn judge(engine: &Engine, grammars: &Grammars, case: &Case) -> Face {
    let (name, text, gold) = (case.name, case.text.as_str(), &case.gold);
    let reads = |off: &RuleSet| case.reads(engine, off);
    let mut face = Face {
        name: name.to_string(),
        equal: reads(&grammars.whole),
        ..Face::default()
    };
    face.compositional = face.equal && reads(&grammars.seeded);
    if !face.equal || face.compositional {
        return face;
    }
    // Any narrower grammar that matches keeps a matching derivation; prefer losing tail.
    let narrowest = if reads(&grammars.tail_and_literal) {
        &grammars.tail_and_literal
    } else {
        face.needs_tail = !reads(&grammars.tail);
        face.needs_literal = !reads(&grammars.literal);
        match (face.needs_tail, face.needs_literal) {
            (false, _) => &grammars.tail,
            (true, false) => &grammars.literal,
            (true, true) => &grammars.whole,
        }
    };
    let derivations = match engine.audited(name, text, narrowest) {
        Ok(readings) => derivations(readings, gold),
        Err(why) => {
            face.unresolved = Some(why);
            return face;
        }
    };
    // Keep the smallest derivation, or else the union of all. A rule whose value holds no
    // tag escapes the audit, and the face then fails this proof.
    let union: BTreeSet<String> = derivations.iter().flatten().cloned().collect();
    for candidate in derivations.first().into_iter().chain([&union]) {
        if reads(&grammars.only(candidate)) {
            face.needed = candidate.clone();
            return face;
        }
    }
    face.needed = union;
    face.unresolved = Some(if derivations.is_empty() {
        "no tagged reading equals the IR".into()
    } else {
        "does not match with only its needed seeded rules".into()
    });
    face
}

/// The seeded rules behind each tagged reading equal to the IR, smallest first.
fn derivations(readings: Vec<Audited>, gold: &Value) -> Vec<BTreeSet<String>> {
    let mut found: Vec<BTreeSet<String>> = readings
        .into_iter()
        .filter(|(reading, _)| reading == gold)
        .map(|(_, rules)| {
            let rules = rules.iter().flatten();
            rules.map(|rule| declared(rule).to_string()).collect()
        })
        .collect();
    found.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    found.dedup();
    found
}

/// The rules among `unused` that an unresolved face cannot lose, by bisection: each round
/// finds the shortest prefix of `unused` that, kept on, lets the face match, and keeps its
/// last rule. `None` when the face does not match even with all of them on.
fn rescue(
    engine: &Engine,
    grammars: &Grammars,
    case: &Case,
    unused: &[String],
) -> Option<Vec<String>> {
    const ROUNDS: usize = 64;
    let off = |on: &[usize], from: usize| -> RuleSet {
        let rest = (from..unused.len()).filter(|index| !on.contains(index));
        grammars.ids(rest.map(|index| &unused[index])).collect()
    };
    let mut on = Vec::new();
    let mut high = unused.len();
    for _ in 0..ROUNDS {
        if case.reads(engine, &off(&on, 0)) {
            return Some(on.iter().map(|&index| unused[index].clone()).collect());
        }
        if !case.reads(engine, &off(&on, high)) {
            return None;
        }
        let mut low = 0;
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if case.reads(engine, &off(&on, middle)) {
                high = middle;
            } else {
                low = middle;
            }
        }
        on.push(low);
        high = low;
    }
    None
}

/// Blame the abilities that differ, not the card's other lines; a rule in a right reading
/// of this face is not to blame for a wrong one beside it.
fn credit(engine: &Engine, case: &Case, face: &mut Face) {
    let gold = &case.gold;
    let Ok(readings) = engine.audited(case.name, &case.text, &RuleSet::default()) else {
        return;
    };
    let width = gold.as_array().map_or(0, Vec::len);
    for (reading, rules) in readings {
        if reading.as_array().map_or(0, Vec::len) != width || rules.len() != width {
            continue;
        }
        for (index, names) in rules.into_iter().enumerate() {
            let names = names.iter().map(|rule| declared(rule).to_string());
            if reading[index] == gold[index] {
                face.right.extend(names);
            } else {
                face.wrong.extend(names);
            }
        }
    }
    face.wrong = &face.wrong - &face.right;
}

/// Each needed seeded rule with the faces that need it.
fn needed(faces: &[Face]) -> BTreeMap<&str, Vec<&str>> {
    let mut needed: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for face in faces {
        for rule in &face.needed {
            needed.entry(rule).or_default().push(&face.name);
        }
    }
    needed
}

fn seeded_rules(engine: &Engine) -> BTreeSet<String> {
    let names = engine.grammar.rules.iter().map(|rule| declared(&rule.name));
    names
        .filter(|rule| seeded(rule))
        .map(str::to_string)
        .collect()
}

fn report(engine: &Engine, faces: &[Face], seconds: f64, with_credit: bool) -> Value {
    let count = |test: fn(&Face) -> bool| faces.iter().filter(|face| test(face)).count();
    let needed = needed(faces);
    let rules = seeded_rules(engine);
    let unused: Vec<&String> = rules
        .iter()
        .filter(|rule| !needed.contains_key(rule.as_str()))
        .collect();
    let unresolved: Vec<Value> = faces
        .iter()
        .filter_map(|face| {
            let why = face.unresolved.as_ref()?;
            Some(json!({"name": face.name, "why": why, "rescued": face.rescued}))
        })
        .collect();
    let mut slowest: Vec<&Face> = faces.iter().collect();
    slowest.sort_by_key(|face| std::cmp::Reverse(face.millis));
    let slowest: Vec<Value> = slowest
        .iter()
        .take(25)
        .map(|face| json!({"name": face.name, "millis": face.millis}))
        .collect();
    let mut out = json!({
        "summary": {
            "faces": faces.len(),
            "equal": count(|face| face.equal),
            "compositional": count(|face| face.compositional),
            "needs_seeded": count(|face| face.equal && !face.compositional),
            "needs_tail": count(|face| face.needs_tail),
            "needs_literal": count(|face| face.needs_literal),
            "unresolved": unresolved.len(),
            "at_risk": count(|face| face.unresolved.is_some() && !face.rescued),
            "seeded_rules": rules.len(),
            "needed_rules": needed.len(),
            "unused_rules": unused.len(),
            "seconds": (seconds * 10.0).round() / 10.0,
        },
        "needed": needed,
        "unused": unused,
        "unresolved": unresolved,
        "slowest": slowest,
    });
    if with_credit {
        let mut tally: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
        for face in faces {
            for rule in &face.right {
                tally.entry(rule).or_default().0 += 1;
            }
            for rule in &face.wrong {
                tally.entry(rule).or_default().1 += 1;
            }
        }
        let table: BTreeMap<&str, Value> = tally
            .into_iter()
            .map(|(rule, (right, wrong))| (rule, json!({"right": right, "wrong": wrong})))
            .collect();
        out["credit"] = json!(table);
    }
    out
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let with_credit = args.iter().any(|arg| arg == "--credit");
    let plain: Vec<&String> = args.iter().filter(|arg| !arg.starts_with("--")).collect();
    let [unified, oracle, slice, out] = plain.as_slice() else {
        return Err(
            "usage: rule-audit <unified.jsonl> <oracle.json> <slice.json> <out.json> [--credit] [--module=name=path]..."
                .into(),
        );
    };
    let started = Instant::now();
    let engine = Engine::load_from_args()?;
    let literal = engine.literal_rules();
    let grammars = Grammars {
        seeded: engine.rule_set(seeded),
        tail_and_literal: engine.rule_set(|rule| rule.starts_with("t_") || literal.contains(rule)),
        tail: engine.rule_set(|rule| rule.starts_with("t_")),
        literal: engine.rule_set(|rule| literal.contains(rule)),
        whole: RuleSet::default(),
        groups: (0..)
            .zip(&engine.grammar.rules)
            .filter(|(_, rule)| seeded(&rule.name))
            .fold(BTreeMap::new(), |mut groups, (id, rule)| {
                let declared = declared(&rule.name).to_string();
                groups.entry(declared).or_insert_with(Vec::new).push(id);
                groups
            }),
    };
    let sources = source::load(oracle)?;
    let slice: Value = serde_json::from_slice(&std::fs::read(slice).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let wanted: BTreeSet<String> = slice["cards"]
        .as_array()
        .ok_or("slice has no cards")?
        .iter()
        .filter_map(|card| card.as_str().map(str::to_string))
        .collect();
    let wanted_faces = source::faces(unified, &wanted)?;
    let done = AtomicUsize::new(0);
    let judged = corpus::parallel(&wanted_faces, |face| {
        let began = Instant::now();
        let case = Case::of(&engine, &sources, face)?;
        let mut judged = judge(&engine, &grammars, &case);
        if with_credit {
            credit(&engine, &case, &mut judged);
        }
        judged.millis = began.elapsed().as_millis();
        let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
        if finished.is_multiple_of(1000) {
            let seconds = started.elapsed().as_secs();
            eprintln!("{finished}/{} faces in {seconds}s", wanted_faces.len());
        }
        Ok::<_, String>(judged)
    })?;
    let mut faces = judged.into_iter().collect::<Result<Vec<_>, _>>()?;
    let needed = needed(&faces);
    let unused: Vec<String> = seeded_rules(&engine)
        .into_iter()
        .filter(|rule| !needed.contains_key(rule.as_str()))
        .collect();
    let stuck: Vec<usize> = (0..faces.len())
        .filter(|&index| faces[index].unresolved.is_some())
        .collect();
    let rescued = corpus::parallel(&stuck, |&index| {
        let case = Case::of(&engine, &sources, &wanted_faces[index])?;
        Ok::<_, String>(rescue(&engine, &grammars, &case, &unused))
    })?;
    for (index, found) in stuck.into_iter().zip(rescued) {
        if let Some(rules) = found? {
            faces[index].needed.extend(rules);
            faces[index].rescued = true;
        }
    }
    let report = report(
        &engine,
        &faces,
        started.elapsed().as_secs_f64(),
        with_credit,
    );
    println!("{}", report["summary"]);
    std::fs::write(out, report.to_string()).map_err(|e| e.to_string())
}

fn main() -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(run)
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "rule audit panicked".to_string())?
}
