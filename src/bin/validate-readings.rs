//! Audit saved unsealed score readings with the dependency's full IR validator.
//! This does not change recognition or count IR-equality as validation evidence.
use oracle_ir_grammar::corpus;
use oracle_ir_grammar::engine::canonical_abilities;
use oracle_ir_grammar::model::{ir::Card, validate};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, BufReader, BufWriter, Write};

#[path = "validate-readings/preserve.rs"]
mod preserve;

fn check(face: &Value, abilities: &Value) -> Result<(), String> {
    let mut value = face.clone();
    // The audit checks operand compatibility, using the current schema header.
    // Earlier gold stays immutable; its source schema is reported separately.
    value["schema_version"] = json!(oracle_ir_grammar::model::ir::SCHEMA_VERSION);
    value["abilities"] = abilities.clone();
    let card: Card = serde_json::from_value(value).map_err(|e| format!("typed decode: {e}"))?;
    // card() already checks every ability and unreduced form. Do not traverse
    // the complete card once more through first_invalid() before that check.
    validate::card(&card).map_err(|why| {
        why.strip_prefix(&format!("{}: ", card.name))
            .unwrap_or(&why)
            .to_owned()
    })
}

fn load_rows(path: &str) -> Result<Vec<Value>, String> {
    BufReader::new(std::fs::File::open(path).map_err(|e| e.to_string())?)
        .lines()
        .map(|line| {
            let text = line.map_err(|e| e.to_string())?;
            serde_json::from_str(&text).map_err(|e| e.to_string())
        })
        .collect()
}

fn canonical(value: &Value) -> Result<bool, String> {
    Ok(canonical_abilities(value)? == *value)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (plain, previous) = match args.as_slice() {
        [_, _, _, _] => (args.as_slice(), None),
        [_, _, _, _, flag, old] if flag == "--preserve-from" => (&args[..4], Some(old)),
        _ => return Err("usage: validate-readings <gold.jsonl> <unsealed-score.jsonl> <report.json> <failures.jsonl> [--preserve-from OLD-SCORE.jsonl]".into()),
    };
    let [gold, saved, report, failures] = plain else {
        unreachable!();
    };
    let rows = load_rows(saved)?;
    let mut preservation = previous
        .map(|path| preserve::Preservation::new(load_rows(path)?, &rows))
        .transpose()?;
    let wanted: HashSet<&str> = rows.iter().filter_map(|r| r["name"].as_str()).collect();
    let mut faces = corpus::faces(gold)?.filter(|face| {
        face.as_ref().map_or(true, |f| {
            f["name"].as_str().is_some_and(|n| wanted.contains(n))
        })
    });
    let mut failed = BufWriter::new(std::fs::File::create(failures)?);
    let mut gold_valid = 0;
    let mut canonical_gold_valid = 0;
    let mut canonical_gold_idempotent = 0;
    let mut recognized = 0;
    let mut readings = 0;
    let mut valid_readings = 0;
    let mut canonical_readings = 0;
    let mut any_valid = 0;
    let mut all_valid = 0;
    let mut gold_reasons = BTreeMap::<String, usize>::new();
    let mut canonical_gold_reasons = BTreeMap::<String, usize>::new();
    let mut reading_reasons = BTreeMap::<String, usize>::new();
    let mut source_schemas = BTreeMap::<String, usize>::new();
    for (row_index, row) in rows.iter().enumerate() {
        let face = faces
            .next()
            .ok_or("score rows outnumber matching gold faces")??;
        if face["name"] != row["name"] {
            return Err("score/gold order or populations differ".into());
        }
        *source_schemas
            .entry(face["schema_version"].to_string())
            .or_default() += 1;
        match check(&face, &face["abilities"]) {
            Ok(()) => gold_valid += 1,
            Err(why) => {
                *gold_reasons.entry(why.clone()).or_default() += 1;
                writeln!(
                    failed,
                    "{}",
                    json!({"name":row["name"],"population":"gold","why":why})
                )?;
            }
        }
        let canonical_gold = canonical_abilities(&face["abilities"])?;
        if canonical(&canonical_gold)? {
            canonical_gold_idempotent += 1;
        } else {
            writeln!(
                failed,
                "{}",
                json!({"name":row["name"],"population":"canonical_gold","why":"normalization is not idempotent"})
            )?;
        }
        match check(&face, &canonical_gold) {
            Ok(()) => canonical_gold_valid += 1,
            Err(why) => {
                *canonical_gold_reasons.entry(why.clone()).or_default() += 1;
                writeln!(
                    failed,
                    "{}",
                    json!({"name":row["name"],"population":"canonical_gold","why":why})
                )?;
            }
        }
        let values = preserve::readings(row, &canonical_gold)?;
        if let Some(comparison) = &mut preservation {
            comparison.inspect(row_index, &values, &canonical_gold, &face)?;
        }
        if row["status"] == "refused" {
            continue;
        }
        recognized += 1;
        let mut valid = 0;
        for (index, value) in values.iter().enumerate() {
            readings += 1;
            if canonical(value)? {
                canonical_readings += 1;
            } else {
                writeln!(
                    failed,
                    "{}",
                    json!({"name":row["name"],"population":"reading","index":index,"why":"saved reading is not canonical"})
                )?;
            }
            match check(&face, value) {
                Ok(()) => {
                    valid += 1;
                    valid_readings += 1;
                }
                Err(why) => {
                    *reading_reasons.entry(why.clone()).or_default() += 1;
                    writeln!(
                        failed,
                        "{}",
                        json!({"name":row["name"],"population":"reading","index":index,"why":why})
                    )?;
                }
            }
        }
        any_valid += usize::from(valid > 0);
        all_valid += usize::from(valid == values.len());
    }
    if faces.next().is_some() {
        return Err("matching gold faces outnumber score rows".into());
    }
    failed.flush()?;
    let mut result = json!({
        "compiled_schema":oracle_ir_grammar::model::ir::SCHEMA_VERSION,
        "gold_source_schemas":source_schemas,
        "source_gold":{"faces":rows.len(),"valid":gold_valid,"invalid_reasons":gold_reasons},
        "canonical_gold":{"faces":rows.len(),"valid":canonical_gold_valid,"idempotent":canonical_gold_idempotent,"invalid_reasons":canonical_gold_reasons},
        "recognized_faces":recognized,
        "faces_with_any_valid_reading":any_valid,
        "faces_with_all_readings_valid":all_valid,
        "readings":{"total":readings,"valid":valid_readings,"canonical":canonical_readings,"invalid_reasons":reading_reasons},
        "scope":"Saved unsealed readings only; no parsing/printing performed. Full-card IR validation with the current schema header; source operands and gold remain unchanged. Both-unsupported is never agreement."
    });
    if let Some(comparison) = &preservation {
        result["reading_preservation"] = comparison.report();
    }
    std::fs::write(report, serde_json::to_vec_pretty(&result)?)?;
    println!(
        "source gold valid {gold_valid}/{}; canonical gold valid {canonical_gold_valid}; readings valid {valid_readings}/{readings}; any-valid faces {any_valid}/{recognized}",
        rows.len()
    );
    if gold_valid != rows.len()
        || canonical_gold_valid != rows.len()
        || canonical_gold_idempotent != rows.len()
        || valid_readings != readings
        || canonical_readings != readings
        || preservation.as_ref().is_some_and(|p| !p.passed())
    {
        return Err(
            "full-card validity or canonical-form audit failed; see report/failures".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_valid_ids_are_not_mistaken_for_a_canonical_reading() {
        let raw = json!([{"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},
            "body":{"instructions":[{"kind":"Effect","args":{"result":0,"operation":{
                "kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},
                "amount":{"kind":"Constant","args":1}}}}}]}}}]);
        oracle_ir_grammar::engine::validate_abilities(&raw).unwrap();
        assert!(!canonical(&raw).unwrap());
        assert!(canonical(&canonical_abilities(&raw).unwrap()).unwrap());
    }
}
