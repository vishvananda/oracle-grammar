//! The scoreboard: every gate reported separately, per card, on a named slice.
use oracle_ir_grammar::corpus;
use oracle_ir_grammar::engine::{
    CardNote, CardReading, Engine, Parsed, Refusal, canonical_abilities, semantic_abilities,
};
use oracle_ir_grammar::parse::RuleSet;
use serde_json::{Value, json};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

#[path = "score/card.rs"]
mod card;
#[path = "score/reuse.rs"]
mod reuse;
#[path = "score/source.rs"]
mod source;

struct Options {
    unified: String,
    oracle: String,
    slice: String,
    out: String,
    render: bool,
    /// Read Oracle verbatim and compare meaning separately from presentation.
    raw: bool,
    /// Retain the validated source layout needed to recheck an edited Station card.
    source_context: bool,
    /// Reparse every enumerated text of each IR equal card, not only the ones kept.
    round_trip: bool,
    /// Totals only: a sealed slice must not print or store its cards.
    aggregate: bool,
    /// Also write the totals as JSON, for the gate check.
    totals: Option<String>,
    /// Rule-name prefixes to switch off, e.g. `m_,t_` for compositional coverage.
    without: Vec<String>,
    /// The rules those prefixes name, once the grammar is loaded.
    off: RuleSet,
    /// Traces to write, and a previous run whose rows may carry over.
    reuse: reuse::Flags,
}

fn options() -> Result<Options, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let render = args.iter().any(|a| a == "--render");
    let raw = args.iter().any(|a| a == "--raw");
    let source_context = args.iter().any(|a| a == "--source-context");
    let round_trip = args.iter().any(|a| a == "--round-trip");
    let aggregate = args.iter().any(|a| a == "--aggregate");
    let totals = args
        .iter()
        .find_map(|a| a.strip_prefix("--totals=").map(str::to_string));
    let without: Vec<String> = args
        .iter()
        .filter_map(|a| a.strip_prefix("--without="))
        .flat_map(|list| list.split(','))
        .filter(|prefix| !prefix.is_empty())
        .map(str::to_string)
        .collect();
    let reuse = reuse::flags(&args)?;
    let plain: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let [unified, oracle, slice, out] = plain.as_slice() else {
        return Err("usage: score <unified.jsonl> <oracle.json> <slice.json> <out.jsonl> [--raw] [--source-context] [--render] [--round-trip] [--aggregate] [--totals=FILE] [--without=PREFIX,...]".into());
    };
    if !without.is_empty() && (raw || render || round_trip) {
        return Err(
            "--without scores parsing only; it excludes --raw, --render and --round-trip".into(),
        );
    }
    Ok(Options {
        unified: (*unified).clone(),
        oracle: (*oracle).clone(),
        slice: (*slice).clone(),
        out: (*out).clone(),
        render,
        raw,
        source_context,
        round_trip,
        aggregate,
        totals,
        without,
        off: RuleSet::default(),
        reuse,
    })
}

#[derive(Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Score {
    cards: u64,
    recognized: u64,
    unique: u64,
    ir_equal: u64,
    exact_key: u64,
    ordered_keys: u64,
    exact_no_key: u64,
    /// Cards with no exact print whose text the grammar can still reach.
    reachable: u64,
    /// IR equal cards with a printed text that reads back to the same IR.
    round_trip: u64,
    /// Candidate texts at the first beam width, and those that read back. Printing is
    /// generate-and-test, so a candidate that fails is dropped, not a failed gate.
    candidates: u64,
    candidates_kept: u64,
    lexical: u64,
    limit: u64,
    no_parse: u64,
    binding: u64,
}

/// One card's row, what it adds to the totals, and where it stuck if nothing read it.
struct Judged {
    row: Value,
    /// A row carried over from a previous run, written as it was read.
    raw: Option<String>,
    score: Score,
    stuck: Option<String>,
}

fn render_row(
    engine: &Engine,
    face: &Value,
    text: &str,
    row: &mut Value,
    score: &mut Score,
) -> Result<(), String> {
    let name = face["name"].as_str().unwrap_or_default();
    let types = oracle_ir_grammar::corpus::print_context(face);
    let mut memo = oracle_ir_grammar::render::FaceMemo::new();
    let printed = engine.print_keyed_memo(&mut memo, name, &face["abilities"], &types, &[])?;
    render_row_with_default(engine, &mut memo, face, (text, &types), row, score, printed)
}

fn render_row_with_default(
    engine: &Engine,
    memo: &mut oracle_ir_grammar::render::FaceMemo,
    face: &Value,
    (text, types): (&str, &[String]),
    row: &mut Value,
    score: &mut Score,
    printed: Option<String>,
) -> Result<(), String> {
    let name = face["name"].as_str().unwrap_or_default();
    let abilities = &face["abilities"];
    score.round_trip += u64::from(printed.is_some());
    let first = printed.as_deref() == Some(text);
    let mut any = first;
    if !first {
        row["default"] = json!(printed);
        let default = engine.productions_of_memo(memo, name, abilities, types)?;
        if let Some(full) = engine.derivation_of_types(name, types, abilities, text)? {
            score.reachable += 1;
            let trimmed = Engine::trim_key(&full, &default);
            // A key is a set, so a short one can make every candidate wrong the same way
            // and fail only after the whole fallback search. Try it narrowly, then the full
            // key, and give the short key its whole search only if the full key fails too.
            let narrow = engine.print_keyed_narrow_memo(memo, name, abilities, types, &trimmed)?;
            let mut found = (narrow.as_deref() == Some(text)).then_some(&trimmed);
            let rest = if trimmed == full {
                vec![&full]
            } else {
                vec![&full, &trimmed]
            };
            for key in rest {
                if found.is_none()
                    && engine
                        .print_keyed_memo(memo, name, abilities, types, key)?
                        .as_deref()
                        == Some(text)
                {
                    found = Some(key);
                }
            }
            any = found.is_some();
            row["key"] = json!(found.unwrap_or(&full));
            // A set cannot tell two uses of one rule apart, and an ordered key can.
            if !any && let Some(trail) = engine.trail_of_types(name, types, abilities, text)? {
                let replayed = engine.print_trail_types(name, types, abilities, &trail)?;
                any = replayed.as_deref() == Some(text);
                if any {
                    score.ordered_keys += 1;
                    row["key"] = json!(trail);
                    row["ordered"] = json!(true);
                }
            }
        }
    }
    score.exact_no_key += u64::from(first);
    score.exact_key += u64::from(any);
    row["exact"] = json!(any);
    if !any {
        row["rendered"] = json!(printed.iter().collect::<Vec<_>>());
        row["print_frontier"] = json!(engine.print_frontier(name, abilities, text)?);
    }
    if !first {
        row["first"] = json!(printed);
    }
    Ok(())
}

fn round_trip_row(
    engine: &Engine,
    face: &Value,
    row: &mut Value,
    score: &mut Score,
) -> Result<(), String> {
    let name = face["name"].as_str().unwrap_or_default();
    let types = oracle_ir_grammar::corpus::print_context(face);
    let result = engine.round_trip(name, &face["abilities"], &types, 8)?;
    record_round_trip(row, score, result);
    Ok(())
}

fn record_round_trip(row: &mut Value, score: &mut Score, (texts, lost): (u64, Vec<String>)) {
    let same = texts - lost.len() as u64;
    score.candidates += texts;
    score.candidates_kept += same;
    row["round_trip"] = json!([texts, same]);
    if !lost.is_empty() {
        row["lost"] = json!(lost);
    }
}

/// Source gold has no presentation. In raw mode every reading with matching
/// meaning supplies its own presentation IR; none is silently selected. Card
/// printing gates require all such readings, while candidate counts sum the
/// reading/text pairs searched. Legacy gold-based scoring stays unchanged.
fn render_raw_readings(
    engine: &Engine,
    face: &Value,
    matching: &[(usize, &Value)],
    notes: &[Vec<CardNote>],
    row: &mut Value,
    score: &mut Score,
    options: &Options,
) -> Result<(), String> {
    let mut reports = Vec::new();
    let mut flags = Vec::new();
    for (index, reading) in matching {
        let mut presented = face.clone();
        presented["abilities"] = (*reading).clone();
        let mut detail = json!({"reading":index});
        let mut one = Score::default();
        if options.render {
            card::render(
                engine,
                &presented,
                &CardReading {
                    abilities: (*reading).clone(),
                    notes: notes[*index].clone(),
                },
                row["text"].as_str().unwrap_or_default(),
                &mut detail,
                &mut one,
            )?;
        }
        if options.round_trip {
            card::round_trip(
                engine,
                &presented,
                &CardReading {
                    abilities: (*reading).clone(),
                    notes: notes[*index].clone(),
                },
                &mut detail,
                &mut one,
            )?;
        }
        reports.push(detail);
        flags.push(one);
    }
    if options.render {
        score.exact_key += u64::from(flags.iter().all(|f| f.exact_key == 1));
        score.exact_no_key += u64::from(flags.iter().all(|f| f.exact_no_key == 1));
        score.round_trip += u64::from(flags.iter().all(|f| f.round_trip == 1));
        score.ordered_keys += u64::from(
            flags.iter().all(|f| f.exact_key == 1) && flags.iter().any(|f| f.ordered_keys == 1),
        );
        score.reachable += u64::from(flags.iter().any(|f| f.reachable == 1));
        row["exact"] = json!(flags.iter().all(|f| f.exact_key == 1));
    }
    score.candidates += flags.iter().map(|f| f.candidates).sum::<u64>();
    score.candidates_kept += flags.iter().map(|f| f.candidates_kept).sum::<u64>();
    row["printing"] = json!(reports);
    Ok(())
}

fn judge(
    engine: &Engine,
    face: &Value,
    oracle: &source::Sources,
    options: &Options,
) -> Result<Judged, String> {
    let name = face["name"].as_str().unwrap_or_default();
    let source::Input {
        text: source,
        context: source_context,
        error: source_error,
    } = source::input(engine, oracle, face)?;
    let source = source.as_str();
    let contextual_face = if source_context.station_present {
        let mut contextual_face = face.clone();
        contextual_face["_source_context"] =
            serde_json::to_value(&source_context).map_err(|e| e.to_string())?;
        Some(contextual_face)
    } else {
        None
    };
    let face = contextual_face.as_ref().unwrap_or(face);
    let text = if options.raw {
        source.to_string()
    } else {
        engine.presentation_free(source)
    };
    let gold = if options.raw {
        semantic_abilities(&face["abilities"])?
    } else {
        canonical_abilities(&face["abilities"])?
    };
    let mut score = Score {
        cards: 1,
        ..Score::default()
    };
    let mut stuck = None;
    let mut row = json!({"name": name, "text": text});
    if options.source_context && source_context.station_present && source_error.is_none() {
        row["source_context"] = face["_source_context"].clone();
    }
    if options.raw {
        row["input_mode"] = json!("raw");
    }
    let mut notes = Vec::new();
    let parsed = if let Some(why) = source_error {
        Err(Refusal::Binding(why))
    } else if source_context.station_present {
        engine
            .parse_card_without_source(name, &text, &source_context, &options.off)
            .map(|parsed| {
                notes = parsed
                    .readings
                    .iter()
                    .map(|reading| reading.notes.clone())
                    .collect();
                Parsed {
                    readings: parsed
                        .readings
                        .into_iter()
                        .map(|reading| reading.abilities)
                        .collect(),
                    counters: parsed.counters,
                }
            })
    } else if options.raw {
        let context =
            serde_json::from_value(face["characteristics"].clone()).map_err(|e| e.to_string())?;
        engine.parse_card(name, &text, &context).map(|parsed| {
            notes = parsed
                .readings
                .iter()
                .map(|reading| reading.notes.clone())
                .collect();
            Parsed {
                readings: parsed.readings.into_iter().map(|r| r.abilities).collect(),
                counters: parsed.counters,
            }
        })
    } else {
        let types = oracle_ir_grammar::corpus::print_context(face);
        engine.parse_without_types(name, &text, &types, &options.off)
    };
    match parsed {
        Err(refusal) => {
            row["status"] = json!("refused");
            row["why"] = json!(format!("{refusal:?}"));
            match refusal {
                Refusal::Lexical(_) => score.lexical += 1,
                Refusal::Limit(_) => score.limit += 1,
                Refusal::Binding(_) => score.binding += 1,
                Refusal::NoParse { furthest, tokens } => {
                    score.no_parse += 1;
                    let frontier = engine.frontier_at(name, &text, furthest);
                    row["syntax"] =
                        json!({"furthest":furthest,"tokens":tokens,"frontier":frontier});
                    stuck = Some(frontier);
                }
            }
        }
        Ok(parsed) => {
            score.recognized += 1;
            let mut matching = Vec::new();
            let mut meanings = Vec::new();
            for (index, reading) in parsed.readings.iter().enumerate() {
                let same = if options.raw {
                    let meaning = semantic_abilities(reading)?;
                    let same = meaning == gold;
                    meanings.push(meaning);
                    same
                } else {
                    *reading == gold
                };
                if same {
                    matching.push((index, reading));
                }
            }
            let equal = !matching.is_empty();
            if options.raw || source_context.station_present {
                row["notes_all"] = json!(notes);
                row["semantic_all"] = json!(meanings);
                row["matching_readings"] =
                    json!(matching.iter().map(|(index, _)| index).collect::<Vec<_>>());
            }
            score.unique += u64::from(parsed.readings.len() == 1);
            score.ir_equal += u64::from(equal);
            row["status"] = json!(if equal { "equal" } else { "different" });
            row["readings"] = json!(parsed.readings.len());
            if options.raw || parsed.readings.len() > 1 {
                row["all"] = json!(parsed.readings);
            }
            if !equal {
                row["gold"] = gold;
                row["ours"] = parsed.readings[0].clone();
            }
            if (options.raw || source_context.station_present)
                && equal
                && (options.render || options.round_trip)
            {
                render_raw_readings(
                    engine, face, &matching, &notes, &mut row, &mut score, options,
                )?;
            } else if equal {
                match (options.render, options.round_trip) {
                    (true, true) => {
                        let types = oracle_ir_grammar::corpus::print_context(face);
                        let mut memo = oracle_ir_grammar::render::FaceMemo::new();
                        let (printed, round_trip) = engine.audit_default_print_memo(
                            &mut memo,
                            name,
                            &face["abilities"],
                            &types,
                            8,
                        )?;
                        render_row_with_default(
                            engine,
                            &mut memo,
                            face,
                            (&text, &types),
                            &mut row,
                            &mut score,
                            printed,
                        )?;
                        record_round_trip(&mut row, &mut score, round_trip);
                    }
                    (true, false) => render_row(engine, face, &text, &mut row, &mut score)?,
                    (false, true) => round_trip_row(engine, face, &mut row, &mut score)?,
                    (false, false) => {}
                }
            }
        }
    }
    Ok(Judged {
        row,
        raw: None,
        score,
        stuck,
    })
}

impl Score {
    fn add(&mut self, other: &Score) {
        self.cards += other.cards;
        self.recognized += other.recognized;
        self.unique += other.unique;
        self.ir_equal += other.ir_equal;
        self.exact_key += other.exact_key;
        self.ordered_keys += other.ordered_keys;
        self.exact_no_key += other.exact_no_key;
        self.reachable += other.reachable;
        self.round_trip += other.round_trip;
        self.candidates += other.candidates;
        self.candidates_kept += other.candidates_kept;
        self.lexical += other.lexical;
        self.limit += other.limit;
        self.no_parse += other.no_parse;
        self.binding += other.binding;
    }
}

/// A judged card, what its print consulted, and its identity for the trace file.
type Traced = (Judged, reuse::Packed, String);

fn judge_traced(
    engine: &Engine,
    face: &Value,
    oracle: &source::Sources,
    options: &Options,
    previous: Option<&reuse::Previous<'_>>,
) -> Result<Traced, String> {
    let print = || judge(engine, face, oracle, options);
    if previous.is_none() && options.reuse.trace_out.is_none() {
        return Ok((print()?, reuse::Packed::default(), String::new()));
    }
    let name = face["name"].as_str().unwrap_or_default();
    let input = source::input(engine, oracle, face)?;
    let context = serde_json::to_string(&input.context).map_err(|e| e.to_string())?;
    let key = reuse::face_key(face, &input.text, &format!("{context}{:?}", input.error));
    let text = if options.raw {
        input.text.clone()
    } else {
        engine.presentation_free(&input.text)
    };
    let (judged, saved) = match previous {
        Some(previous) => previous.judge(&key, name, &text, print)?,
        None => reuse::traced(print)?,
    };
    Ok((judged, reuse::Packed::new(&saved), key))
}

fn judge_faces(
    engine: &Engine,
    faces: &[Value],
    oracle: &source::Sources,
    options: &Options,
    previous: Option<&reuse::Previous<'_>>,
) -> Result<Vec<Result<Traced, String>>, String> {
    let work = |face: &Value| judge_traced(engine, face, oracle, options, previous);
    if options.raw || !options.render || !options.round_trip {
        return corpus::parallel(faces, work);
    }
    // Starting longer faces first gives their expensive print searches time to
    // finish while other threads still have work. Emit results in input order.
    let mut schedule: Vec<_> = faces.iter().enumerate().collect();
    schedule.sort_by_cached_key(|(index, face)| {
        let size = source::original(oracle, face)
            .map_or(0, |original| engine.presentation_free(&original.text).len());
        (Reverse(size), *index)
    });
    let mut judged = corpus::parallel(&schedule, |(index, face)| (*index, work(face)))?;
    judged.sort_unstable_by_key(|(index, _)| *index);
    Ok(judged.into_iter().map(|(_, card)| card).collect())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut options = options()?;
    let engine = Engine::load_from_args()?;
    options.off =
        engine.rule_set(|rule| options.without.iter().any(|p| rule.starts_with(p.as_str())));
    let slice: Value = serde_json::from_slice(&std::fs::read(&options.slice)?)?;
    let wanted: BTreeSet<String> = slice["cards"]
        .as_array()
        .ok_or("slice has no cards")?
        .iter()
        .filter_map(|c| c.as_str().map(str::to_string))
        .collect();
    let oracle = source::load(&options.oracle)?;
    let faces = source::faces(&options.unified, &wanted)?;
    let old = match &options.reuse.spec {
        Some(spec) => Engine::load_from_dir(std::path::Path::new(&spec.grammar)).ok(),
        None => None,
    };
    // A previous run that cannot be used only costs a full print.
    let previous = match (&options.reuse.spec, &old) {
        (Some(spec), Some(old)) => match reuse::Previous::load(spec, old, &engine, &options) {
            Ok(previous) => Some(previous),
            Err(why) => {
                if let Some(path) = &options.reuse.report {
                    reuse::unavailable(path, &why)?;
                }
                None
            }
        },
        (Some(_), None) => {
            if let Some(path) = &options.reuse.report {
                reuse::unavailable(path, "the previous grammar does not load")?;
            }
            None
        }
        _ => None,
    };
    let judged = judge_faces(&engine, &faces, &oracle, &options, previous.as_ref())?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(&options.out)?);
    let mut score = Score::default();
    let mut stuck: BTreeMap<String, u64> = BTreeMap::new();
    let mut traces = Vec::new();
    for (card, face) in judged.into_iter().zip(&faces) {
        let (card, saved, key) = card?;
        score.add(&card.score);
        if let Some(prefix) = &card.stuck {
            *stuck.entry(prefix.clone()).or_default() += 1;
        }
        if !options.aggregate {
            match &card.raw {
                Some(raw) => writeln!(out, "{raw}")?,
                None => writeln!(out, "{}", card.row)?,
            }
        }
        if options.reuse.trace_out.is_some() {
            let name = face["name"].as_str().unwrap_or_default().to_string();
            traces.push((name, key, saved, card.score, card.stuck));
        }
    }
    out.flush()?;
    if let Some(path) = &options.reuse.trace_out {
        reuse::write_traces(path, &engine, &options, &traces)?;
    }
    if let (Some(path), Some(previous)) = (&options.reuse.report, &previous) {
        previous.report(path)?;
    }
    let pct = |n: u64| 100.0 * n as f64 / score.cards.max(1) as f64;
    println!("cards            {}", score.cards);
    println!(
        "recognized       {} ({:.1}%)",
        score.recognized,
        pct(score.recognized)
    );
    println!("one reading      {}", score.unique);
    println!(
        "IR equal         {} ({:.1}%)",
        score.ir_equal,
        pct(score.ir_equal)
    );
    if options.render {
        println!(
            "exact with key   {} ({:.1}%)",
            score.exact_key,
            pct(score.exact_key)
        );
        println!("ordered keys     {} of those", score.ordered_keys);
        println!(
            "exact, no key    {} ({:.1}%)",
            score.exact_no_key,
            pct(score.exact_no_key)
        );
        println!(
            "key rate         {:.1}% of IR equal",
            100.0 * score.reachable as f64 / score.ir_equal.max(1) as f64
        );
        println!(
            "round trip       {} of {} IR equal ({:.1}%)",
            score.round_trip,
            score.ir_equal,
            100.0 * score.round_trip as f64 / score.ir_equal.max(1) as f64
        );
    }
    if options.round_trip {
        println!(
            "candidates kept  {} of {} ({:.1}%)",
            score.candidates_kept,
            score.candidates,
            100.0 * score.candidates_kept as f64 / score.candidates.max(1) as f64
        );
    }
    println!(
        "refused: lexical {} limit {} no-parse {} binding {}",
        score.lexical, score.limit, score.no_parse, score.binding
    );
    if let Some(path) = &options.totals {
        let mut totals = json!({
            "cards": score.cards,
            "recognized": score.recognized,
            "one_reading": score.unique,
            "ir_equal": score.ir_equal,
            "exact_with_key": score.exact_key,
            "exact_no_key": score.exact_no_key,
            "round_trip": score.round_trip,
            "refused_lexical": score.lexical,
            "refused_limit": score.limit,
            "refused_no_parse": score.no_parse,
            "refused_binding": score.binding,
        });
        if options.raw {
            totals["input_mode"] = json!("raw");
            if options.round_trip {
                totals["candidate_pairs"] = json!(score.candidates);
                totals["candidate_pairs_kept"] = json!(score.candidates_kept);
            }
        }
        std::fs::write(path, format!("{totals}\n"))?;
    }
    if options.aggregate {
        return Ok(());
    }
    let mut stuck: Vec<_> = stuck.into_iter().collect();
    stuck.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (prefix, count) in stuck.iter().take(240) {
        println!("{count:5}  {prefix}");
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(|| run().map_err(|e| e.to_string()))?
        .join()
        .map_err(|_| "scoreboard panicked")??;
    Ok(())
}
