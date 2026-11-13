//! Original Scryfall bulk JSON -> one explicit native outcome per face.
use oracle_ir_grammar::{corpus, engine::Engine, export};
use std::io::{BufWriter, Write};

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        return Err("usage: native-export <scryfall-oracle.json> <outcomes.jsonl>".into());
    };
    let bytes = std::fs::read(input).map_err(|e| e.to_string())?;
    let records: Vec<serde_json::Value> =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let faces = export::scryfall(&records)?;
    let engine = Engine::load()?;
    let outcomes = corpus::parallel(&faces, |face| export::outcome(&engine, face))?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    let mut stream = BufWriter::new(file);
    for row in outcomes {
        serde_json::to_writer(&mut stream, &row).map_err(|e| e.to_string())?;
        stream.write_all(b"\n").map_err(|e| e.to_string())?;
    }
    stream.flush().map_err(|e| e.to_string())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
