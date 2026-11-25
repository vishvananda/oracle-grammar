//! Check a phrase family's synthetic cases with one grammar load and no rebuild.
use oracle_ir_grammar::corpus;
use oracle_ir_grammar::engine::{
    CardInputContext, CardNote, Engine, Refusal, canonical_abilities, validate_abilities,
};
use oracle_ir_grammar::model::ir::Characteristics;
use oracle_ir_grammar::parse::RuleSet;
use oracle_ir_grammar::render::FaceMemo;
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
    /// None inherits the invocation's mode; false permits seeded rules for this case.
    #[serde(default)]
    compositional: Option<bool>,
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

/// Synthetic family cases without full `context` often specify bare card types
/// (e.g. `["Creature"]`) without supertypes even for legendary cards; keep
/// `Names::of` tokenization unless `"Legendary"` is listed.
fn case_lex_types(types: &[String]) -> &[String] {
    if types.iter().any(|t| t == "Legendary") {
        types
    } else {
        &[]
    }
}

fn exact(
    engine: &Engine,
    case: &Case,
    reading: &Value,
    memo: &mut FaceMemo,
) -> Result<bool, String> {
    let lex = case_lex_types(&case.types);
    if let Some(key) = engine.derivation_of_types(&case.name, lex, reading, &case.text)?
        && engine
            .print_keyed_memo(memo, &case.name, reading, &case.types, &key)?
            .as_deref()
            == Some(&case.text)
    {
        return Ok(true);
    }
    match engine.trail_of_types(&case.name, lex, reading, &case.text)? {
        Some(trail) => Ok(engine
            .print_trail_types(&case.name, lex, reading, &trail)?
            .as_deref()
            == Some(&case.text)),
        None => Ok(false),
    }
}

/// An authored operand order may carry a printable surface lost in canonical
/// reduction. The caller has already matched this IR to the current reading.
fn exact_authored(
    engine: &Engine,
    case: &Case,
    authored: &Value,
    types: &[String],
    memo: &mut FaceMemo,
) -> Result<bool, String> {
    let name = &case.name;
    let text = case.text.as_str();
    let lex = case_lex_types(types);
    if let Some(full) = engine.derivation_of_types(name, lex, authored, text)? {
        let default = engine.productions_of_memo(memo, name, authored, types)?;
        let trimmed = Engine::trim_key(&full, &default);
        if engine
            .print_keyed_narrow_memo(memo, name, authored, types, &trimmed)?
            .as_deref()
            == Some(text)
            || engine
                .print_keyed_memo(memo, name, authored, types, &full)?
                .as_deref()
                == Some(text)
            || (trimmed != full
                && engine
                    .print_keyed_memo(memo, name, authored, types, &trimmed)?
                    .as_deref()
                    == Some(text))
        {
            return Ok(true);
        }
    }
    if let Some(trail) = engine.trail_of_types(name, lex, authored, text)? {
        return Ok(engine
            .print_trail_types(name, lex, authored, &trail)?
            .as_deref()
            == Some(text));
    }
    Ok(false)
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
    let lex = case_lex_types(&case.types);
    let parsed = match engine.parse_without_types(&case.name, &case.text, lex, off) {
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
    let expected = case
        .expected
        .as_ref()
        .map(|authored| {
            validate_abilities(authored)?;
            Ok::<_, String>((authored, canonical_abilities(authored)?))
        })
        .transpose()?;
    if let Some((_, canonical)) = &expected
        && !parsed.readings.contains(canonical)
    {
        return Err("no reading equals the expected IR".into());
    }
    for reading in &parsed.readings {
        let mut memo = FaceMemo::new();
        validate_abilities(reading)?;
        // A verified default that is already the requested wording proves both
        // gates, as in score. Otherwise retain the independent keyed search.
        let default = case
            .default
            .then(|| engine.print_keyed_memo(&mut memo, &case.name, reading, &case.types, &[]));
        let exact_default = default
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .and_then(Option::as_deref)
            == Some(case.text.as_str());
        if case.exact && !exact_default && !exact(engine, case, reading, &mut memo)? {
            let reproduced = match &expected {
                Some((authored, canonical)) if canonical == reading => {
                    exact_authored(engine, case, authored, &case.types, &mut memo)?
                }
                _ => false,
            };
            if !reproduced {
                return Err("a reading has no exact keyed print".into());
            }
        }
        if let Some(default) = default
            && default?.is_none()
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
            if !exact
                && reading.notes.is_empty()
                && expected.as_ref() == Some(&reading.abilities)
                && let Some(authored) = &case.expected
            {
                let types = corpus::print_context(&json!({"characteristics": context}));
                exact = exact_authored(engine, case, authored, &types, &mut FaceMemo::new())?;
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
    let paths = family_paths(&args)?;
    let mut budget = FamilyBudget::default();
    let families = paths
        .iter()
        .map(|path| load_family(path, &mut budget))
        .collect::<Result<Vec<_>, _>>()?;
    let engine = Engine::load_from_args()?;
    let compositional = args.iter().any(|arg| arg == "--compositional");
    let off_seeded = engine.rule_set(|name| name.starts_with("m_") || name.starts_with("t_"));
    let off_none = RuleSet::default();
    // Cases are independent reads of one immutable engine. Results keep input order.
    let cases: Vec<&Case> = families.iter().flat_map(|f| &f.cases).collect();
    let mut checked = corpus::parallel(&cases, |case| {
        check_case(&engine, &off_seeded, &off_none, case, compositional)
    })?
    .into_iter();
    let reports: Vec<_> = families
        .iter()
        .map(|family| {
            let results: Vec<_> = checked.by_ref().take(family.cases.len()).collect();
            report_family(family, &results, compositional)
        })
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

fn check_case(
    engine: &Engine,
    off_seeded: &RuleSet,
    off_none: &RuleSet,
    case: &Case,
    compositional: bool,
) -> Result<usize, String> {
    let off = if case.compositional.unwrap_or(compositional) {
        off_seeded
    } else {
        off_none
    };
    check(engine, off, case)
}

const MAX_FAMILY_FILES: usize = 16_384;
const MAX_FIXTURE_BYTES: usize = 4 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 1024 * 1024 * 1024;
const MAX_TOTAL_CASES: usize = 262_144;

fn family_paths(args: &[String]) -> Result<Vec<&String>, String> {
    let paths: Vec<_> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if paths.is_empty() || paths.len() > MAX_FAMILY_FILES {
        return Err("usage: family-check <family.json>... (at most 16384) [--compositional] [--module=name=path]...".into());
    }
    for arg in args.iter().filter(|a| a.starts_with("--")) {
        if arg != "--compositional" && !arg.starts_with("--module=") {
            return Err(format!("unknown option {arg}"));
        }
    }
    Ok(paths)
}

struct FamilyBudget {
    remaining_bytes: usize,
    remaining_cases: usize,
}

impl Default for FamilyBudget {
    fn default() -> Self {
        Self {
            remaining_bytes: MAX_TOTAL_BYTES,
            remaining_cases: MAX_TOTAL_CASES,
        }
    }
}

fn load_family(path: &str, budget: &mut FamilyBudget) -> Result<Family, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    read_family(file, budget)
}

fn read_family(reader: impl Read, budget: &mut FamilyBudget) -> Result<Family, String> {
    // Count bytes actually read, and read one extra byte to detect either limit.
    let limit = MAX_FIXTURE_BYTES.min(budget.remaining_bytes);
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FIXTURE_BYTES {
        return Err("family fixture exceeds 4 MiB".into());
    }
    budget.remaining_bytes = budget
        .remaining_bytes
        .checked_sub(bytes.len())
        .ok_or("family fixtures exceed 1 GiB in total")?;
    let family: Family = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if family.family.is_empty() || family.cases.is_empty() || family.cases.len() > 4096 {
        return Err("a named family needs between 1 and 4096 cases".into());
    }
    budget.remaining_cases = budget
        .remaining_cases
        .checked_sub(family.cases.len())
        .ok_or("family fixtures exceed 262144 cases in total")?;
    Ok(family)
}

fn report_family(
    family: &Family,
    results: &[Result<usize, String>],
    compositional: bool,
) -> (Value, bool) {
    let mut failures = Vec::new();
    let mut readings = 0;
    for (case, result) in family.cases.iter().zip(results) {
        match result {
            Ok(n) => readings += n,
            Err(why) => failures.push(json!({"name":case.name,"why":why})),
        }
    }
    let passed = failures.is_empty();
    let compositional_cases = family
        .cases
        .iter()
        .filter(|case| case.compositional.unwrap_or(compositional))
        .count();
    let report = json!({"family":family.family,"cases":family.cases.len(),
            "passed":family.cases.len()-failures.len(),
            "compositional":compositional_cases == family.cases.len(),
            "compositional_cases":compositional_cases,
            "seeded_cases":family.cases.len()-compositional_cases,
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

#[cfg(test)]
#[path = "../tests/family_check_modes.rs"]
mod family_check_modes;
