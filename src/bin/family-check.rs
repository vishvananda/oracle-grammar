//! Check a phrase family's synthetic cases with one grammar load and no rebuild.
use oracle_ir_grammar::engine::{
    CardInputContext, CardNote, Engine, Refusal, canonical_abilities, validate_abilities,
};
use oracle_ir_grammar::model::ir::Characteristics;
use oracle_ir_grammar::parse::RuleSet;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::Read;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Family {
    family: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    text: String,
    #[serde(default)]
    types: Vec<String>,
    #[serde(default)]
    context: Option<Characteristics>,
    #[serde(default)]
    source_context: Option<CardInputContext>,
    #[serde(default)]
    expected_notes: Option<Vec<CardNote>>,
    /// None explicitly allows every distinct reading; the default expects one.
    #[serde(default = "one")]
    readings: Option<usize>,
    #[serde(default)]
    reject: bool,
    #[serde(default)]
    expected: Option<Value>,
    #[serde(default)]
    exact: bool,
    /// Require a verified print using the default search, independently of a key.
    #[serde(default)]
    default: bool,
}

fn one() -> Option<usize> {
    Some(1)
}

fn exact(engine: &Engine, case: &Case, reading: &Value) -> Result<bool, String> {
    if let Some(key) = engine.derivation_of(&case.name, reading, &case.text)?
        && engine
            .print_keyed(&case.name, reading, &case.types, &key)?
            .as_deref()
            == Some(&case.text)
    {
        return Ok(true);
    }
    match engine.trail_of(&case.name, reading, &case.text)? {
        Some(trail) => {
            Ok(engine.print_trail(&case.name, reading, &trail)?.as_deref() == Some(&case.text))
        }
        None => Ok(false),
    }
}

fn check(engine: &Engine, off: &RuleSet, case: &Case) -> Result<usize, String> {
    if case.reject
        && (case.expected.is_some() || case.expected_notes.is_some() || case.exact || case.default)
    {
        return Err("a refusal case cannot also request IR or exact printing".into());
    }
    if case.context.is_some() && case.source_context.is_some() {
        return Err("choose one card context".into());
    }
    if let Some(context) = &case.source_context {
        return check_source_card(engine, off, case, context);
    }
    if let Some(context) = &case.context {
        return check_card(engine, off, case, context);
    }
    if case.expected_notes.is_some() {
        return Err("expected_notes requires card context".into());
    }
    let parsed = match engine.parse_without(&case.name, &case.text, off) {
        Err(Refusal::Limit(why)) => return Err(format!("work limit: {why}")),
        Err(_) if case.reject => return Ok(0),
        Err(why) => return Err(format!("unexpected refusal: {why:?}")),
        Ok(_) if case.reject => return Err("expected refusal, got a valid reading".into()),
        Ok(parsed) => parsed,
    };
    if case.readings.is_some_and(|n| n != parsed.readings.len()) {
        return Err(format!(
            "expected {:?} readings, got {}",
            case.readings,
            parsed.readings.len()
        ));
    }
    if let Some(expected) = &case.expected {
        validate_abilities(expected)?;
        if !parsed.readings.contains(&canonical_abilities(expected)?) {
            return Err("no reading equals the expected IR".into());
        }
    }
    for reading in &parsed.readings {
        validate_abilities(reading)?;
        if case.exact && !exact(engine, case, reading)? {
            return Err("a reading has no exact keyed print".into());
        }
        if case.default
            && engine
                .print_keyed(&case.name, reading, &case.types, &[])?
                .is_none()
        {
            return Err("a reading has no verified default print".into());
        }
    }
    Ok(parsed.readings.len())
}

fn check_card(
    engine: &Engine,
    off: &RuleSet,
    case: &Case,
    context: &Characteristics,
) -> Result<usize, String> {
    let parsed = match engine.parse_card_without(&case.name, &case.text, context, off) {
        Err(Refusal::Limit(why)) => return Err(format!("work limit: {why}")),
        Err(_) if case.reject => return Ok(0),
        Err(why) => return Err(format!("unexpected refusal: {why:?}")),
        Ok(_) if case.reject => return Err("expected refusal, got a card reading".into()),
        Ok(parsed) => parsed,
    };
    if case.readings.is_some_and(|n| n != parsed.readings.len()) {
        return Err(format!(
            "expected {:?} card readings, got {}",
            case.readings,
            parsed.readings.len()
        ));
    }
    let expected = case
        .expected
        .as_ref()
        .map(|v| {
            validate_abilities(v)?;
            canonical_abilities(v)
        })
        .transpose()?;
    if !parsed.readings.iter().any(|r| {
        expected.as_ref().is_none_or(|v| r.abilities == *v)
            && case
                .expected_notes
                .as_ref()
                .is_none_or(|notes| r.notes == *notes)
    }) {
        return Err("no card reading equals the expected IR and notes".into());
    }
    for reading in &parsed.readings {
        if case.exact {
            let mut exact = false;
            if let Some(key) =
                engine.card_key_of(&case.name, reading, context, &case.text, false)?
            {
                exact = engine
                    .print_card_keyed(&case.name, reading, context, &key)?
                    .as_deref()
                    == Some(&case.text);
            }
            if !exact
                && let Some(trail) =
                    engine.card_key_of(&case.name, reading, context, &case.text, true)?
            {
                exact = engine
                    .print_card_trail(&case.name, reading, context, &trail)?
                    .as_deref()
                    == Some(&case.text);
            }
            if !exact {
                return Err("a card reading has no exact keyed print".into());
            }
        }
        if case.default
            && engine
                .print_card_keyed(&case.name, reading, context, &[])?
                .is_none()
        {
            return Err("a card reading has no verified default print".into());
        }
    }
    Ok(parsed.readings.len())
}

fn check_source_card(
    engine: &Engine,
    off: &RuleSet,
    case: &Case,
    context: &CardInputContext,
) -> Result<usize, String> {
    let parsed = match engine.parse_card_without_source(&case.name, &case.text, context, off) {
        Err(Refusal::Limit(why)) => return Err(format!("work limit: {why}")),
        Err(_) if case.reject => return Ok(0),
        Err(why) => return Err(format!("unexpected refusal: {why:?}")),
        Ok(_) if case.reject => return Err("expected refusal, got a card reading".into()),
        Ok(parsed) => parsed,
    };
    if case.readings.is_some_and(|n| n != parsed.readings.len()) {
        return Err(format!(
            "expected {:?} card readings, got {}",
            case.readings,
            parsed.readings.len()
        ));
    }
    let expected = case
        .expected
        .as_ref()
        .map(|v| {
            validate_abilities(v)?;
            canonical_abilities(v)
        })
        .transpose()?;
    if !parsed.readings.iter().any(|r| {
        expected.as_ref().is_none_or(|v| r.abilities == *v)
            && case
                .expected_notes
                .as_ref()
                .is_none_or(|notes| r.notes == *notes)
    }) {
        return Err("no card reading equals the expected IR and notes".into());
    }
    for reading in &parsed.readings {
        if case.exact {
            let mut exact = false;
            if let Some(key) =
                engine.card_key_of_with_source(&case.name, reading, context, &case.text, false)?
            {
                exact = engine
                    .print_card_keyed_with_source(&case.name, reading, context, &key)?
                    .as_deref()
                    == Some(&case.text);
            }
            if !exact
                && let Some(trail) = engine
                    .card_key_of_with_source(&case.name, reading, context, &case.text, true)?
            {
                exact = engine
                    .print_card_trail_with_source(&case.name, reading, context, &trail)?
                    .as_deref()
                    == Some(&case.text);
            }
            if !exact {
                return Err("a card reading has no exact keyed print".into());
            }
        }
        if case.default
            && engine
                .print_card_keyed_with_source(&case.name, reading, context, &[])?
                .is_none()
        {
            return Err("a card reading has no verified default print".into());
        }
    }
    Ok(parsed.readings.len())
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let paths: Vec<_> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if paths.is_empty() || paths.len() > 64 {
        return Err(
            "usage: family-check <family.json>... (at most 64) [--compositional] [--module=name=path]...".into(),
        );
    }
    for arg in args.iter().filter(|a| a.starts_with("--")) {
        if arg != "--compositional" && !arg.starts_with("--module=") {
            return Err(format!("unknown option {arg}"));
        }
    }
    let families = paths
        .iter()
        .map(|path| load_family(path))
        .collect::<Result<Vec<_>, _>>()?;
    let engine = Engine::load_from_args()?;
    let compositional = args.iter().any(|arg| arg == "--compositional");
    let off =
        engine.rule_set(|name| compositional && (name.starts_with("m_") || name.starts_with("t_")));
    let reports: Vec<_> = families
        .iter()
        .map(|family| report_family(&engine, &off, family, compositional))
        .collect();
    let passed = reports.iter().all(|(_, passed)| *passed);
    if reports.len() == 1 {
        println!("{}", reports[0].0);
    } else {
        println!(
            "{}",
            json!({"schema":1,"families": reports.iter().map(|(report, _)| report).collect::<Vec<_>>() })
        );
    }
    if passed {
        Ok(())
    } else {
        Err("family checks failed".into())
    }
}

fn load_family(path: &str) -> Result<Family, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("family fixture exceeds 4 MiB".into());
    }
    let family: Family = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if family.family.is_empty() || family.cases.is_empty() || family.cases.len() > 4096 {
        return Err("a named family needs between 1 and 4096 cases".into());
    }
    Ok(family)
}

fn report_family(
    engine: &Engine,
    off: &RuleSet,
    family: &Family,
    compositional: bool,
) -> (Value, bool) {
    let mut failures = Vec::new();
    let mut readings = 0;
    for case in &family.cases {
        match check(engine, off, case) {
            Ok(n) => readings += n,
            Err(why) => failures.push(json!({"name":case.name,"why":why})),
        }
    }
    let passed = failures.is_empty();
    let report = json!({"family":family.family,"cases":family.cases.len(),
            "passed":family.cases.len()-failures.len(),"compositional":compositional,
            "readings":readings,"expected_ir_cases":family.cases.iter().filter(|c| c.expected.is_some()).count(),
            "exact_cases":family.cases.iter().filter(|c| c.exact).count(),
            "default_cases":family.cases.iter().filter(|c| c.default).count(),
            "refusal_cases":family.cases.iter().filter(|c| c.reject).count(),"failures":failures});
    (report, passed)
}

fn main() -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(run)
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "family worker panicked")?
}
