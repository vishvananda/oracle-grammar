//! One pass with optional allocator counters, separate from elapsed-time claims.
use oracle_ir_grammar::engine::{Engine, Parsed, Refusal};
use oracle_ir_grammar::render::Work;
use serde_json::{Value, json};

mod profile_support;
use profile_support::{Card, read};
#[path = "profile_support/allocation.rs"]
mod allocation;

const STRIDE: usize = 10;
type Read = Result<Parsed, Refusal>;

// Named frames also make these API calls identifiable to an external profiler.
#[inline(never)]
fn phase_load() -> Result<Engine, String> {
    let result = Engine::load();
    std::hint::black_box(&result);
    result
}

#[inline(never)]
fn phase_io(unified: &str, oracle: &str, slice: &str) -> Result<Vec<Card>, String> {
    let result = read(unified, oracle, slice);
    std::hint::black_box(&result);
    result
}

#[inline(never)]
fn phase_parse(engine: &Engine, cards: &[Card], results: &mut [Option<Read>]) {
    for (card, result) in cards.iter().zip(results) {
        *result = Some(engine.parse(&card.name, &card.text));
    }
}

#[inline(never)]
fn phase_print(
    engine: &Engine,
    cards: &[Card],
    results: &mut [Option<Result<Vec<String>, String>>],
) {
    for (card, result) in cards.iter().zip(results) {
        *result = Some(engine.render(&card.name, &card.abilities, &card.context, 1));
    }
}

#[inline(never)]
fn phase_reparse(engine: &Engine, printed: &[(&str, &str)], results: &mut [Option<Read>]) {
    for ((name, text), result) in printed.iter().zip(results) {
        *result = Some(engine.parse(name, text));
    }
}

#[inline(never)]
fn phase_enumerate(engine: &Engine, cards: &[Card]) -> Work {
    let mut work = Work::default();
    for card in cards {
        if let Ok(found) = engine.enumerated(&card.name, &card.abilities, &card.context, 8, &[]) {
            work.add(&found.work);
        }
    }
    work
}

fn reading(result: &Option<Read>) -> Value {
    match result {
        Some(Ok(parsed)) => json!({
            "readings": parsed.readings,
            "counters": {
                "steps": parsed.counters.steps,
                "rule_calls": parsed.counters.rule_calls,
                "memo_hits": parsed.counters.memo_hits,
                "first_prunes": parsed.counters.first_prunes,
            },
        }),
        Some(Err(error)) => json!({"refusal": format!("{error:?}")}),
        None => json!({"error": "missing driver result"}),
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [unified, oracle, slice] = args.as_slice() else {
        return Err("usage: allocations <unified.jsonl> <oracle.json> <slice.json>".into());
    };
    let whole = allocation::Window::new();
    let (engine, load) = allocation::measure(phase_load);
    let engine = engine?;
    let (cards, io) = allocation::measure(|| phase_io(unified, oracle, slice));
    let cards = cards?;
    let sampled = cards.len();
    let cards: Vec<Card> = cards.into_iter().step_by(STRIDE).collect();
    // Allocate the observation slots outside the measured phase frames. Keep API
    // results until the report so the instrumented run can prove output identity.
    let mut parses: Vec<_> = (0..cards.len()).map(|_| None).collect();
    let mut prints: Vec<_> = (0..cards.len()).map(|_| None).collect();
    let (_, parse) = allocation::measure(|| phase_parse(&engine, &cards, &mut parses));
    let (_, print) = allocation::measure(|| phase_print(&engine, &cards, &mut prints));
    let printed: Vec<_> = cards
        .iter()
        .zip(&prints)
        .filter_map(|(card, result)| {
            let text = result.as_ref()?.as_ref().ok()?.first()?;
            Some((card.name.as_str(), text.as_str()))
        })
        .collect();
    let mut reparses: Vec<_> = (0..printed.len()).map(|_| None).collect();
    let (_, reparse) = allocation::measure(|| phase_reparse(&engine, &printed, &mut reparses));
    let (work, enumerate) = allocation::measure(|| phase_enumerate(&engine, &cards));
    let total = whole.finish();
    let rows: Vec<_> = cards
        .iter()
        .zip(parses)
        .zip(prints.iter())
        .map(|((card, parsed), printed)| {
            json!({"name": card.name, "text": card.text,
                "parse": reading(&parsed), "print": printed})
        })
        .collect();
    let reread: Vec<_> = printed
        .iter()
        .zip(reparses)
        .map(
            |((name, text), parsed)| json!({"name": name, "text": text, "parse": reading(&parsed)}),
        )
        .collect();
    let report = json!({
        "sample_cards": sampled, "stride": STRIDE, "cards": rows,
        "reparsed": reread, "rules": engine.rule_count(),
        "print_cap": 1, "enumeration_cap": 8,
        "allocation_profile": {
            "enabled": cfg!(feature = "allocation-profile"),
            "work_total": total,
            "phases": {"load": load, "io": io, "parse": parse, "print": print,
                "reparse": reparse, "enumerate": enumerate},
        },
        "work": {
            "nodes": work.nodes, "unified": work.unified, "matched": work.matched,
            "walked": work.walked, "joined": work.joined, "ranked": work.ranked,
            "read": work.read, "cuts": work.cuts, "branch_nodes": work.branch_nodes,
        },
    });
    println!(
        "{}",
        serde_json::to_string(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn main() -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(run)
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "allocations panicked".to_string())?
}
