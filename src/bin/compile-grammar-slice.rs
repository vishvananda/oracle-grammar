//! Compile a grammar slice from spelled forms through the ordinary typed loader.
use oracle_ir_grammar::engine::Engine;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::io::Read;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    cases: Vec<Case>,
    #[serde(default)]
    complete_rules: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    text: String,
    #[serde(default)]
    types: Vec<String>,
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [profile, output] = args.as_slice() else {
        return Err("usage: compile-grammar-slice profile.json output.json".into());
    };
    const PROFILE_BYTES: usize = 256 * 1024;
    let mut bytes = Vec::new();
    std::fs::File::open(profile)
        .and_then(|file| file.take(PROFILE_BYTES as u64 + 1).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    if bytes.len() > PROFILE_BYTES {
        return Err("slice profile exceeds byte limit".into());
    }
    let profile: Profile = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if !(1..=64).contains(&profile.cases.len()) {
        return Err("slice profile needs one to 64 cases".into());
    }
    let engine = Engine::load()?;
    let mut selected = BTreeSet::new();
    for case in &profile.cases {
        let parsed = engine
            .parse_types(&case.name, &case.text, &case.types)
            .map_err(|error| format!("{}: {error:?}", case.name))?;
        for reading in &parsed.readings {
            let trail = engine
                .trail_of_types(&case.name, &case.types, reading, &case.text)?
                .ok_or_else(|| format!("{} has a reading with no spelling trail", case.name))?;
            selected.extend(trail);
        }
    }
    let rules: Vec<_> = profile.complete_rules.iter().map(String::as_str).collect();
    let compiled = engine.compile_slice(&selected.into_iter().collect::<Vec<_>>(), &rules)?;
    let slice = Engine::from_compiled(&compiled)?;
    for case in &profile.cases {
        let old = engine
            .parse_types(&case.name, &case.text, &case.types)
            .map_err(|error| format!("{}: {error:?}", case.name))?;
        let new = slice
            .parse_types(&case.name, &case.text, &case.types)
            .map_err(|error| format!("{} in compiled slice: {error:?}", case.name))?;
        if old.readings != new.readings {
            return Err(format!(
                "{} changes readings in the compiled slice",
                case.name
            ));
        }
        for reading in new.readings {
            if slice
                .trail_of_types(&case.name, &case.types, &reading, &case.text)?
                .is_none()
            {
                return Err(format!("{} lost its original spelling", case.name));
            }
        }
    }
    std::fs::write(output, compiled).map_err(|error| error.to_string())?;
    println!("Compiled {} rules", slice.rule_count());
    Ok(())
}

fn main() -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(1 << 26)
        .spawn(run)
        .map_err(|error| error.to_string())?
        .join()
        .map_err(|_| "grammar slice compiler panicked".to_string())?
}
