//! The perturbed and refused gates. Swap one selector, number, color, type or duration
//! in an IR, print it and read it back; corrupt a text and require a refusal.
use oracle_ir_grammar::corpus;
use oracle_ir_grammar::engine::{Engine, canonical_abilities};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

#[path = "perturb/corruption.rs"]
mod corruption;
#[path = "perturb/token_recipes.rs"]
mod token_recipes;

const COLORS: [&str; 5] = ["White", "Blue", "Black", "Red", "Green"];
const TYPES: [&str; 5] = [
    "Creature",
    "Artifact",
    "Enchantment",
    "Land",
    "Planeswalker",
];
const SUBTYPES: [&str; 6] = ["Goblin", "Elf", "Zombie", "Soldier", "Wizard", "Beast"];
const DURATIONS: [&str; 2] = ["EndOfTurn", "EndOfCombat"];

fn next_of<'a>(list: &[&'a str], item: &str) -> Option<&'a str> {
    let at = list.iter().position(|x| *x == item)?;
    Some(list[(at + 1) % list.len()])
}

/// Every swap the IR offers, as a family and the changed value. Keeps the targets of an
/// announcement and their mentions alike, because both hold the same specification.
fn swaps(value: &Value, out: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(_) if token_recipes::rules_defined(value) => swaps(&value["amount"], out),
        Value::Object(_) if wordless(value) => {}
        Value::Object(object) => {
            token_recipes::sites(value, out);
            let kind = object.get("kind").and_then(Value::as_str);
            let args = object.get("args");
            let name = args.and_then(Value::as_str);
            match (kind, name) {
                (Some("Color"), Some(color)) if next_of(&COLORS, color).is_some() => {
                    out.push(("color".into(), value.clone()));
                }
                (Some("CardType"), Some(ty)) if next_of(&TYPES, ty).is_some() => {
                    out.push(("type".into(), value.clone()));
                }
                (Some("Subtype"), Some(ty)) if next_of(&SUBTYPES, ty).is_some() => {
                    out.push(("selector".into(), value.clone()));
                }
                _ => {}
            }
            if kind == Some("Constant") && args.and_then(Value::as_i64).is_some_and(|n| n >= 1) {
                out.push(("number".into(), value.clone()));
            }
            for (key, item) in object {
                if key == "duration" && item.as_str().is_some_and(|d| DURATIONS.contains(&d)) {
                    out.push(("duration".into(), item.clone()));
                }
                swaps(item, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| swaps(item, out)),
        _ => {}
    }
}

/// Whether a value holds fixed meaning that no words of a card spell. A keyword's ability
/// and a named token's recipe are what the name means, and `attacks each combat if able`
/// says no number. Changing one of these makes an IR with no text in the language at all,
/// so no site is taken from inside one.
fn wordless(value: &Value) -> bool {
    let keyword_body = value["origin"].as_str() != Some("Ordinary") && !value["origin"].is_null();
    keyword_body
        || value.get("RequireCombat").is_some()
        || value["property"] == "ColorCount"
        || (value["kind"] == "Keyword" && value["args"].get("action").is_some())
}

/// Whether a list now names one thing twice, which a swap can do to `artifact or creature`.
fn repeats(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.values().any(repeats),
        Value::Array(items) => {
            let names = items.iter().filter(|item| item["kind"] == "CardType");
            let mut seen: Vec<&Value> = Vec::new();
            for name in names {
                if seen.contains(&name) {
                    return true;
                }
                seen.push(name);
            }
            items.iter().any(repeats)
        }
        _ => false,
    }
}

fn changed(old: &Value, family: &str) -> Option<Value> {
    if old["words"] == "CreatingEffect" {
        return token_recipes::changed(old, family);
    }
    if let Some(duration) = old.as_str() {
        return next_of(&DURATIONS, duration).map(Value::from);
    }
    let name = old["args"].as_str();
    match old["kind"].as_str()? {
        "Color" => Some(json!({"kind": "Color", "args": next_of(&COLORS, name?)?})),
        "CardType" => Some(json!({"kind": "CardType", "args": next_of(&TYPES, name?)?})),
        "Subtype" => Some(json!({"kind": "Subtype", "args": next_of(&SUBTYPES, name?)?})),
        "Constant" => Some(json!({"kind": "Constant", "args": old["args"].as_i64()? + 1})),
        _ => None,
    }
}

/// Change one number in place. Some numbers are not free to change, and leaving them alone
/// is not a pass: how many targets a spell announces sets the wording of their later
/// mentions, and an object discarding itself is one object. Mana is written unit by unit,
/// so two `{C}` is a list of two units and never an amount of two.
fn renumbered(value: &mut Value, old: &Value, new: &Value) -> bool {
    if value == old {
        *value = new.clone();
        return true;
    }
    match value {
        Value::Object(_) if token_recipes::rules_defined(value) => {
            renumbered(&mut value["amount"], old, new)
        }
        Value::Object(_) if wordless(value) => false,
        Value::Object(object) => {
            // `choose one` holds its number twice.
            if object.get("minimum") == Some(old) && object.get("maximum") == Some(old) {
                object.insert("minimum".into(), new.clone());
                object.insert("maximum".into(), new.clone());
                return true;
            }
            if let Some(Value::Array(units)) = object.get_mut("All")
                && let Some(at) = units.iter().position(|unit| is_mana_unit(unit, old))
            {
                units.insert(at, units[at].clone());
                return true;
            }
            if object.contains_key("Amount") && is_mana_unit(&Value::Object(object.clone()), old) {
                let unit = Value::Object(object.clone());
                *value = json!({"All": [unit, unit]});
                return true;
            }
            let target = object.contains_key("slot");
            let itself = object.get("filter").is_some_and(is_itself);
            object
                .iter_mut()
                .filter(|(key, _)| !((target || itself) && key.as_str() == "count"))
                .any(|(_, item)| renumbered(item, old, new))
        }
        Value::Array(items) => items.iter_mut().any(|item| renumbered(item, old, new)),
        _ => false,
    }
}

fn is_mana_unit(value: &Value, amount: &Value) -> bool {
    value["Amount"]["amount"] == *amount
}

fn is_itself(filter: &Value) -> bool {
    filter["kind"] == "SameObject" && filter["args"]["kind"] == "Source"
}

/// Replace the first value equal to `old`, and any field that repeats it.
fn replaced(value: &Value, old: &Value, new: &Value) -> Value {
    if value == old {
        return new.clone();
    }
    match value {
        Value::Object(_) if token_recipes::rules_defined(value) => {
            let mut copy = value.clone();
            copy["amount"] = replaced(&value["amount"], old, new);
            copy
        }
        Value::Object(_) if wordless(value) => value.clone(),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(k, v)| (k.clone(), replaced(v, old, new)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(|v| replaced(v, old, new)).collect()),
        other => other.clone(),
    }
}

/// Where two values first differ, for reporting why a text read back differently.
fn first_difference(got: &Value, wanted: &Value) -> Value {
    fn walk(got: &Value, wanted: &Value, path: String) -> Option<Value> {
        if got == wanted {
            return None;
        }
        match (got, wanted) {
            (Value::Object(a), Value::Object(b)) if a.len() == b.len() => a
                .iter()
                .find_map(|(key, part)| walk(part, b.get(key)?, format!("{path}.{key}")))
                .or_else(|| Some(json!([path, got, wanted]))),
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => a
                .iter()
                .zip(b)
                .find_map(|(a, b)| walk(a, b, format!("{path}[]")))
                .or_else(|| Some(json!([path, got, wanted]))),
            _ => Some(json!([path, got, wanted])),
        }
    }
    walk(got, wanted, String::new()).unwrap_or(Value::Null)
}

#[derive(Default, Serialize)]
struct Tally {
    tried: u64,
    passed: u64,
    unprintable: u64,
}

fn merge(total: &mut BTreeMap<String, Tally>, part: BTreeMap<String, Tally>) {
    for (family, entry) in part {
        let sum: &mut Tally = total.entry(family).or_default();
        sum.tried += entry.tried;
        sum.passed += entry.passed;
        sum.unprintable += entry.unprintable;
    }
}

fn perturbed(
    engine: &Engine,
    name: &str,
    gold: &Value,
    types: &[String],
    tally: &mut BTreeMap<String, Tally>,
) -> Result<(), String> {
    let mut sites = Vec::new();
    swaps(gold, &mut sites);
    let mut seen = BTreeSet::new();
    for (family, old) in sites {
        if seen.contains(&family) {
            continue;
        }
        let Some(new) = changed(&old, &family) else {
            continue;
        };
        let mutated = if family == "number" {
            let mut copy = gold.clone();
            if !renumbered(&mut copy, &old, &new) {
                continue;
            }
            copy
        } else {
            replaced(gold, &old, &new)
        };
        if repeats(&mutated) && !repeats(gold) {
            continue;
        }
        seen.insert(family.clone());
        let entry = tally.entry(family).or_default();
        entry.tried += 1;
        let printed = engine.render(name, &mutated, types, 4)?;
        let Some(text) = printed.first() else {
            entry.unprintable += 1;
            if std::env::var_os("PERTURB_SHOW").is_some() {
                let stopped = engine.stopped_at(gold, &mutated, types)?;
                let tried = engine.enumerated(name, &mutated, types, 8, &[])?.texts;
                let wanted = canonical_abilities(&mutated)?;
                let read = tried.first().map(|text| match engine.parse(name, text) {
                    Ok(parsed) => parsed
                        .readings
                        .first()
                        .map_or(json!("no reading"), |reading| {
                            first_difference(reading, &wanted)
                        }),
                    Err(refused) => {
                        json!(format!("{refused:?}").chars().take(60).collect::<String>())
                    }
                });
                let mut failing = None;
                if let (Some(now), Some(before)) = (mutated.as_array(), gold.as_array()) {
                    for (at, (ability, was)) in now.iter().zip(before).enumerate() {
                        let one = Value::Array(vec![ability.clone()]);
                        if engine.render(name, &one, types, 4)?.is_empty() {
                            let was = Value::Array(vec![was.clone()]);
                            let printed = engine.render(name, &was, types, 4)?;
                            let changed = ability != was.get(0).unwrap_or(&Value::Null);
                            let tries = engine.enumerated(name, &one, types, 16, &[])?.texts;
                            failing = Some(json!([at, changed, printed.first(), tries.first()]));
                            break;
                        }
                    }
                }
                let row = json!([stopped, tried.first(), read, failing]);
                eprintln!("UNPRINTABLE {name}\t{old}\t{new}\t{row}");
            }
            continue;
        };
        let want = canonical_abilities(&mutated)?;
        let reads = engine
            .parse(name, text)
            .is_ok_and(|parsed| parsed.readings.contains(&want));
        entry.passed += u64::from(reads);
        if !reads && std::env::var_os("PERTURB_SHOW").is_some() {
            eprintln!("UNREAD {name}\t{old}\t{new}\t{text}");
        }
    }
    Ok(())
}

fn report(title: &str, tally: &BTreeMap<String, Tally>) {
    println!("{title}");
    let (mut tried, mut passed) = (0, 0);
    for (family, entry) in tally {
        println!(
            "  {family:10} {:6} tried {:6} passed {:5.1}%  unprintable {}",
            entry.tried,
            entry.passed,
            100.0 * entry.passed as f64 / entry.tried.max(1) as f64,
            entry.unprintable
        );
        tried += entry.tried;
        passed += entry.passed;
    }
    println!(
        "  all        {tried:6} tried {passed:6} passed {:5.1}%",
        100.0 * passed as f64 / tried.max(1) as f64
    );
}

struct Options {
    unified: String,
    oracle: String,
    slice: String,
    corruption_only: bool,
    totals: Option<String>,
    accepted: Option<String>,
}

fn options() -> Result<Options, String> {
    let (mut plain, mut corruption_only, mut totals, mut accepted) =
        (Vec::new(), false, None, None);
    for arg in std::env::args().skip(1) {
        if arg.starts_with("--module=") {
            continue;
        } else if arg == "--corruption-only" {
            corruption_only = true;
        } else if let Some(path) = arg.strip_prefix("--totals=") {
            totals = Some(path.to_string());
        } else if let Some(path) = arg.strip_prefix("--accepted=") {
            accepted = Some(path.to_string());
        } else if arg.starts_with("--") {
            return Err(format!("unknown option {arg}"));
        } else {
            plain.push(arg);
        }
    }
    let [unified, oracle, slice] = plain.as_slice() else {
        return Err("usage: perturb <unified.jsonl> <oracle.json> <slice.json> [--corruption-only] [--totals=FILE] [--accepted=FILE] [--module=NAME=PATH]".into());
    };
    Ok(Options {
        unified: unified.clone(),
        oracle: oracle.clone(),
        slice: slice.clone(),
        corruption_only,
        totals,
        accepted,
    })
}

#[derive(Default)]
struct Checked {
    original_refusal: Option<corruption::Outcome>,
    ir_equal: bool,
    readings: usize,
    parse_steps: u64,
    swaps: BTreeMap<String, Tally>,
    corruptions: corruption::Checked,
    name: String,
    text: String,
}

fn check_face(
    engine: &Engine,
    face: &Value,
    text: String,
    index: usize,
    options: &Options,
) -> Result<Checked, String> {
    let name = face["name"].as_str().unwrap_or_default();
    let original = match engine.parse(name, &text) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Ok(Checked {
                original_refusal: Some(corruption::Outcome::refusal(&error)),
                ..Checked::default()
            });
        }
    };
    let gold = canonical_abilities(&face["abilities"])?;
    let ir_equal = original.readings.contains(&gold);
    let mut checked = Checked {
        ir_equal,
        readings: original.readings.len(),
        parse_steps: original.counters.steps,
        ..Checked::default()
    };
    drop(original);
    if ir_equal && !options.corruption_only {
        let types = oracle_ir_grammar::corpus::print_context(face);
        perturbed(engine, name, &face["abilities"], &types, &mut checked.swaps)?;
    }
    checked.corruptions = corruption::check(
        engine,
        name,
        &text,
        &gold,
        index,
        options.accepted.is_some(),
    );
    if !checked.corruptions.accepted.is_empty() {
        checked.name = name.to_string();
        checked.text = text;
    }
    Ok(checked)
}

fn run() -> Result<(), String> {
    let options = options()?;
    let engine = Engine::load_from_args()?;
    let oracle = corpus::face_texts(&options.oracle)?;
    let slice: Value =
        serde_json::from_slice(&std::fs::read(&options.slice).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let wanted: BTreeSet<&str> = slice["cards"]
        .as_array()
        .ok_or("slice has no cards")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let mut faces = Vec::new();
    for face in corpus::faces(&options.unified)? {
        let face = face?;
        if face["name"].as_str().is_some_and(|n| wanted.contains(n)) {
            faces.push(face);
        }
    }
    let indexed: Vec<(usize, &Value)> = faces.iter().enumerate().collect();
    let cards = corpus::parallel(&indexed, |(index, face)| {
        let name = face["name"].as_str().unwrap_or_default();
        let text = engine.presentation_free(oracle.get(name).map_or("", String::as_str));
        check_face(&engine, face, text, *index, &options)
    })?;
    let (mut swaps_tally, mut all, mut equal) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let (mut recognized, mut ir_equal, mut with_trials, mut one_reading, mut parse_steps) =
        (0_u64, 0_u64, 0_u64, 0_u64, 0_u64);
    let mut original_refusals = corruption::Tally::default();
    let mut accepted = options
        .accepted
        .as_ref()
        .map(std::fs::File::create)
        .transpose()
        .map_err(|e| e.to_string())?
        .map(std::io::BufWriter::new);
    for card in cards {
        let card = card?;
        if let Some(outcome) = card.original_refusal {
            original_refusals.record(outcome);
            continue;
        }
        recognized += 1;
        one_reading += u64::from(card.readings == 1);
        parse_steps += card.parse_steps;
        ir_equal += u64::from(card.ir_equal);
        with_trials += u64::from(!card.corruptions.families.is_empty());
        merge(&mut swaps_tally, card.swaps);
        corruption::merge(&mut all, &card.corruptions.families);
        if card.ir_equal {
            corruption::merge(&mut equal, &card.corruptions.families);
        }
        if let Some(writer) = accepted.as_mut() {
            for case in card.corruptions.accepted {
                let row = json!({"name":card.name,"original":card.text,"original_ir_equal":card.ir_equal,"original_readings":card.readings,"case":case});
                writeln!(writer, "{row}").map_err(|e| e.to_string())?;
            }
        }
    }
    if let Some(writer) = accepted.as_mut() {
        writer.flush().map_err(|e| e.to_string())?;
    }
    println!(
        "originals: {} faces, {recognized} recognized, {ir_equal} IR equal, {with_trials} with corruption candidates",
        faces.len()
    );
    if !options.corruption_only {
        report(
            "perturbed: the changed IR prints and reads back",
            &swaps_tally,
        );
    }
    corruption::report(
        "strict refusal: all recognized originals; every accepted candidate fails",
        &all,
        false,
    );
    corruption::report("strict refusal: IR-equal originals only", &equal, false);
    corruption::report(
        "no-other-reading proxy: IR-equal originals only; gold-only acceptance also passes",
        &equal,
        true,
    );
    if let Some(path) = &options.totals {
        let totals = json!({"schema":1,"mode":if options.corruption_only {"corruption_only"} else {"all"},
            "originals":{"faces":faces.len(),"recognized":recognized,"ir_equal":ir_equal,"one_reading":one_reading,"with_candidates":with_trials,"refused":original_refusals.json(),"successful_parse_steps":parse_steps},
            "perturbed":if options.corruption_only {Value::Null} else {json!(swaps_tally)},
            "corruption":{"recognized_originals":corruption::summary(&all),"ir_equal_originals":corruption::summary(&equal)},
            "definitions":{"strict_refused":"Every API refusal, including limits; acceptance never passes.",
                "nonlimit_refused":"Lexical, no-parse or binding refusal; limits excluded.",
                "no_other_reading":"Refusal or acceptance of only gold, on the IR-equal population.",
                "candidates":"Historical word mutations, not adjudicated invalid Oracle. Swaps can remain legal."}});
        std::fs::write(path, format!("{totals:#}\n")).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(1 << 28)
        .spawn(run)
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| "perturb panicked".to_string())?
}
