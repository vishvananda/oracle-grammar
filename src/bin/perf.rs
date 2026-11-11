//! Times one thread over a slice: grammar load, corpus I/O, parse, print and reparse, each
//! separately and repeated, with medians. Run the release binary on an idle machine.
use oracle_ir_grammar::engine::Engine;
use oracle_ir_grammar::render::Work;
use std::time::{Duration, Instant};

mod profile_support;
use profile_support::read;

const REPEATS: usize = 5;
const LOADS: usize = 3;
/// One thread prints slowly, so time one card in this many.
const STRIDE: usize = 10;

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn timed<T>(work: impl FnOnce() -> T) -> (Duration, T) {
    let start = Instant::now();
    let out = work();
    (start.elapsed(), out)
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [unified, oracle, slice] = args.as_slice() else {
        return Err("usage: perf <unified.jsonl> <oracle.json> <slice.json>".into());
    };
    let (mut load, mut io) = (Vec::new(), Vec::new());
    let mut engine = None;
    let mut cards = Vec::new();
    for _ in 0..LOADS {
        let (spent, loaded) = timed(Engine::load);
        load.push(spent);
        engine = Some(loaded?);
        let (spent, read_cards) = timed(|| read(unified, oracle, slice));
        io.push(spent);
        cards = read_cards?;
    }
    let sampled = cards.len();
    cards = cards.into_iter().step_by(STRIDE).collect();
    let engine = engine.ok_or("no engine")?;
    let (mut parse, mut print, mut reparse) = (Vec::new(), Vec::new(), Vec::new());
    let mut steps = 0;
    let mut printed = Vec::new();
    for round in 0..REPEATS {
        let (spent, totals) = timed(|| {
            cards
                .iter()
                .filter_map(|card| engine.parse(&card.name, &card.text).ok())
                .map(|parsed| parsed.counters.steps)
                .sum::<u64>()
        });
        parse.push(spent);
        steps = totals;
        let (spent, texts) = timed(|| {
            cards
                .iter()
                .filter_map(|card| {
                    let texts = engine
                        .render(&card.name, &card.abilities, &card.context, 1)
                        .ok()?;
                    Some((card.name.clone(), texts.into_iter().next()?))
                })
                .collect::<Vec<_>>()
        });
        print.push(spent);
        if round == 0 {
            printed = texts;
        }
        let (spent, ()) = timed(|| {
            for (name, text) in &printed {
                let _ = engine.parse(name, text);
            }
        });
        reparse.push(spent);
    }
    let per = |total: Duration| total.as_secs_f64() * 1e6 / cards.len().max(1) as f64;
    println!(
        "cards {} of {sampled}  printed {}  rules {}",
        cards.len(),
        printed.len(),
        engine.rule_count()
    );
    println!("grammar load  {:>10.1?}  median of {LOADS}", median(load));
    println!("corpus i/o    {:>10.1?}  median of {LOADS}", median(io));
    for (name, times) in [
        ("parse", parse),
        ("print+verify", print),
        ("reparse", reparse),
    ] {
        let middle = median(times);
        println!(
            "{name:<13} {middle:>10.1?}  {:>9.0} us/card  median of {REPEATS}",
            per(middle)
        );
    }
    println!("parse steps   {steps:>10}");
    let mut work = Work::default();
    for card in &cards {
        if let Ok(found) = engine.enumerated(&card.name, &card.abilities, &card.context, 8, &[]) {
            work.add(&found.work);
        }
    }
    println!("print work    {work:?}");
    Ok(())
}

fn main() -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(run)
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "perf panicked".to_string())?
}
