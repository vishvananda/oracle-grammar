//! Parses one card's text and prints each reading and its first prints.
use oracle_ir_grammar::engine::Engine;
use oracle_ir_grammar::parse::RuleSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|arg| !arg.starts_with("--module="))
        .collect();
    let without: Vec<String> = args
        .iter()
        .filter_map(|arg| arg.strip_prefix("--without="))
        .flat_map(|prefixes| prefixes.split(',').map(str::to_string))
        .collect();
    let args: Vec<&String> = args
        .iter()
        .filter(|arg| !arg.starts_with("--without="))
        .collect();
    let [name, text] = args.as_slice() else {
        return Err(
            "usage: probe <name> <text> [--module=name=path]... [--without=PREFIX,...]".into(),
        );
    };
    let engine = Engine::load_from_args()?;
    let worker = std::thread::Builder::new().stack_size(1 << 28);
    let (name, text) = ((*name).clone(), (*text).clone());
    let types: Vec<String> = std::env::var("PROBE_TYPES")
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let done = worker.spawn(move || -> Result<(), String> {
        if std::env::var_os("PROBE_MINED").is_some() {
            let audited = engine.audited(&name, &text, &RuleSet::default())?;
            for (_, abilities) in &audited {
                println!("mined {abilities:?}");
            }
        }
        let off = engine.rule_set(|rule| without.iter().any(|p| rule.starts_with(p.as_str())));
        let parsed = engine
            .parse_without(&name, &text, &off)
            .map_err(|e| format!("{e:?}"))?;
        for reading in &parsed.readings {
            println!("{reading}");
            if let Some(steps) = engine.derivation_of(&name, reading, &text)? {
                println!("  rules {steps:?}");
            }
            for print in engine.render(&name, reading, &types, 8)? {
                println!("  {print}");
            }
            if let Ok(cap) = std::env::var("PROBE_CAP") {
                let cap = cap.parse().map_err(|_| "PROBE_CAP is a number")?;
                let printed = engine.enumerated(&name, reading, &types, cap, &[])?;
                for text in &printed.texts {
                    println!("  ? {text}");
                }
            }
        }
        Ok(())
    })?;
    done.join().map_err(|_| "worker panicked")??;
    Ok(())
}
