//! Upgrade allowed reference records through their frozen input descriptors.
use oracle_ir_grammar::model::migrate;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args()
        .skip(1)
        .filter(|arg| !arg.starts_with("--module="))
        .collect();
    let [source] = args.as_slice() else {
        return Err("usage: migrate-reference <allowed-reference.jsonl>".into());
    };
    let mut input = BufReader::new(std::fs::File::open(source)?);
    let mut output = BufWriter::new(std::io::stdout().lock());
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = input
            .by_ref()
            .take(16 * 1024 * 1024 + 1)
            .read_line(&mut line)?;
        if bytes == 0 {
            break;
        }
        if bytes > 16 * 1024 * 1024 {
            return Err("reference record exceeds 16 MiB".into());
        }
        let card = migrate::read_json(&line)?;
        serde_json::to_writer(&mut output, &card)?;
        output.write_all(b"\n")?;
    }
    output.flush()?;
    Ok(())
}
