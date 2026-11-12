//! The scoreboard: every gate reported separately, per card, on a named slice.
use oracle_ir_grammar::corpus;
use oracle_ir_grammar::engine::{
    CardNote, CardReading, Engine, Parsed, Refusal, canonical_abilities, semantic_abilities,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

#[path = "score/card.rs"]
mod card;
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
    /// Reparse every enumerated text of each IR equal card, not only the ones kept.
    round_trip: bool,
    /// Totals only: a sealed slice must not print or store its cards.
    aggregate: bool,
    /// Also write the totals as JSON, for the gate check.
    totals: Option<String>,
}

fn options() -> Result<Options, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let render = args.iter().any(|a| a == "--render");
    let raw = args.iter().any(|a| a == "--raw");
    let round_trip = args.iter().any(|a| a == "--round-trip");
    let aggregate = args.iter().any(|a| a == "--aggregate");
    let totals = args
        .iter()
        .find_map(|a| a.strip_prefix("--totals=").map(str::to_string));
    let plain: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let [unified, oracle, slice, out] = plain.as_slice() else {
        return Err("usage: score <unified.jsonl> <oracle.json> <slice.json> <out.jsonl> [--raw] [--render] [--round-trip] [--aggregate] [--totals=FILE]".into());
    };
    Ok(Options {
        unified: (*unified).clone(),
        oracle: (*oracle).clone(),
        slice: (*slice).clone(),
        out: (*out).clone(),
        render,
        raw,
        round_trip,
        aggregate,
        totals,
    })
}

#[derive(Default)]
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
    let abilities = &face["abilities"];
    let printed = engine.print_keyed(name, abilities, &types, &[])?;
    score.round_trip += u64::from(printed.is_some());
    let first = printed.as_deref() == Some(text);
    let mut any = first;
    if !first {
        row["default"] = json!(printed);
        let default = engine.productions_of(abilities, &types)?;
        if let Some(full) = engine.derivation_of(name, abilities, text)? {
            score.reachable += 1;
            let trimmed = Engine::trim_key(&full, &default);
            for key in [trimmed, full] {
                let keyed = engine.print_keyed(name, abilities, &types, &key)?;
                row["key"] = json!(key);
                any = keyed.as_deref() == Some(text);
                if any {
                    break;
                }
            }
            // A set cannot tell two uses of one rule apart, and an ordered key can.
            if !any && let Some(trail) = engine.trail_of(name, abilities, text)? {
                let replayed = engine.print_trail(name, abilities, &trail)?;
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
    let (texts, lost) = engine.round_trip(name, &face["abilities"], &types, 8)?;
    let same = texts - lost.len() as u64;
    score.candidates += texts;
    score.candidates_kept += same;
    row["round_trip"] = json!([texts, same]);
    if !lost.is_empty() {
        row["lost"] = json!(lost);
    }
    Ok(())
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
    if options.raw {
        row["input_mode"] = json!("raw");
    }
    let mut notes = Vec::new();
    let parsed = if let Some(why) = source_error {
        Err(Refusal::Binding(why))
    } else if source_context.station_present {
        engine
            .parse_card_with_source(name, &text, &source_context)
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
        engine.parse(name, &text)
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
            } else if options.render && equal {
                render_row(engine, face, &text, &mut row, &mut score)?;
            }
            if !options.raw && !source_context.station_present && options.round_trip && equal {
                round_trip_row(engine, face, &mut row, &mut score)?;
            }
        }
    }
    Ok(Judged { row, score, stuck })
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

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let options = options()?;
    let engine = Engine::load_from_args()?;
    let slice: Value = serde_json::from_slice(&std::fs::read(&options.slice)?)?;
    let wanted: BTreeSet<String> = slice["cards"]
        .as_array()
        .ok_or("slice has no cards")?
        .iter()
        .filter_map(|c| c.as_str().map(str::to_string))
        .collect();
    let oracle = source::load(&options.oracle)?;
    let faces = source::faces(&options.unified, &wanted)?;
    let judged = corpus::parallel(&faces, |face| judge(&engine, face, &oracle, &options))?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(&options.out)?);
    let mut score = Score::default();
    let mut stuck: BTreeMap<String, u64> = BTreeMap::new();
    for card in judged {
        let card = card?;
        score.add(&card.score);
        if let Some(prefix) = card.stuck {
            *stuck.entry(prefix).or_default() += 1;
        }
        if !options.aggregate {
            writeln!(out, "{}", card.row)?;
        }
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
