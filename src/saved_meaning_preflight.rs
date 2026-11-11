//! Opt-in rules-text diagnostic. No printing, full-card validation, or coverage claim.
use crate::engine::{Engine, canonical_abilities, validate_abilities};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::io::{Read, Write};

const REQUEST_BYTES: usize = 16 * 1024 * 1024;
const OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const CASES: usize = 4096;
const TEXT_BYTES: usize = 64 * 1024;
const READINGS: usize = 256;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: u32,
    mode: String,
    allowed_names: Vec<String>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    occurrence: usize,
    name: String,
    text: String,
    required_readings: Vec<Value>,
}

fn request(path: &std::path::Path) -> Result<Request, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take((REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > REQUEST_BYTES {
        return Err("request exceeds 16777216 bytes".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let allowed = value["allowed_names"]
        .as_array()
        .ok_or("allowed_names required")?;
    let cases = value["cases"].as_array().ok_or("cases required")?;
    if cases.is_empty() || cases.len() > CASES || allowed.is_empty() || allowed.len() > 100_000 {
        return Err("requires 1..4096 cases and 1..100000 allowed names".into());
    }
    let mut names = HashSet::new();
    for name in allowed {
        let name = name.as_str().ok_or("allowed name must be a string")?;
        if name.is_empty() || name.len() > 1024 || !names.insert(name) {
            return Err("invalid or duplicate allowed name".into());
        }
    }
    let mut occurrences = HashSet::new();
    // Check every identity before inspecting any case text or IR.
    for case in cases {
        let name = case["name"].as_str().ok_or("case name required")?;
        let occurrence = case["occurrence"].as_u64().ok_or("occurrence required")?;
        if !names.contains(name) || !occurrences.insert(occurrence) {
            return Err("outside explicit allowed names or duplicate occurrence".into());
        }
    }
    let request: Request = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if request.schema != 1 || request.mode != "rules_text_parse_only" {
        return Err("only schema 1 rules_text_parse_only is supported".into());
    }
    if request
        .cases
        .iter()
        .any(|case| case.text.len() > TEXT_BYTES || case.required_readings.len() > READINGS)
    {
        return Err("case exceeds 65536 text bytes or 256 required readings".into());
    }
    if request.cases.iter().any(|case| {
        case.text.lines().any(|line| {
            line.trim_start()
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .is_some_and(|word| word.eq_ignore_ascii_case("station"))
        })
    }) {
        return Err("Station/source-context cases are outside rules-text protocol".into());
    }
    // Retain the explicit population in the typed protocol; it is already checked.
    assert!(!request.allowed_names.is_empty());
    Ok(request)
}

fn inspect(engine: &Engine, case: &Case) -> Value {
    let mut expected = Vec::new();
    let mut old_errors = Vec::new();
    for (index, reading) in case.required_readings.iter().enumerate() {
        match validate_abilities(reading).and_then(|()| canonical_abilities(reading)) {
            Ok(value) => {
                if !expected.contains(&value) {
                    expected.push(value);
                }
            }
            Err(why) => old_errors.push(json!({"reading":index,"why":why})),
        }
    }
    // Exactly one whole rules-text parse per case, including initially refused cases.
    let attempt = engine.inspect_with_candidate_ir(&case.name, &case.text);
    let (current, refusal) = match attempt.result {
        Ok(parsed) => (parsed.readings, None),
        Err(why) => (Vec::new(), Some(format!("{why:?}"))),
    };
    let missing: Vec<_> = expected.iter().filter(|v| !current.contains(v)).collect();
    let added: Vec<_> = current.iter().filter(|v| !expected.contains(v)).collect();
    let evidence = (!missing.is_empty()).then_some(attempt.evidence);
    json!({"occurrence":case.occurrence,"name":case.name,
        "required_readings":case.required_readings.len(),"canonical_required":expected.len(),
        "current_readings":current.len(),"missing":missing,"old_errors":old_errors,
        "passed":missing.is_empty() && old_errors.is_empty(),"refusal":refusal,
        "new_recognition":case.required_readings.is_empty() && !current.is_empty(),
        "added_readings_for_review":added,"failure_evidence":evidence})
}

struct BoundedOutput(Vec<u8>);
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > OUTPUT_BYTES {
            return Err(std::io::Error::other(
                "output exceeds 67108864 bytes; no partial report",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
#[ignore = "external request; set ORACLE_PREFLIGHT_REQUEST and ORACLE_PREFLIGHT_OUTPUT"]
fn saved_meaning_cohort() {
    let input = std::env::var_os("ORACLE_PREFLIGHT_REQUEST").expect("ORACLE_PREFLIGHT_REQUEST");
    let output = std::env::var_os("ORACLE_PREFLIGHT_OUTPUT").expect("ORACLE_PREFLIGHT_OUTPUT");
    let request = request(std::path::Path::new(&input)).expect("valid bounded request");
    let engine = crate::test_support::engine();
    let mut rows = Vec::new();
    let mut row_bytes = BoundedOutput(Vec::new());
    for case in &request.cases {
        let row = inspect(engine, case);
        // Bound accumulated evidence while collecting, before the final report
        // serialization. Do not retain unbounded results across many cases.
        serde_json::to_writer(&mut row_bytes, &row).expect("bounded row evidence");
        rows.push(row);
    }
    let passed = rows.iter().all(|row| row["passed"] == true);
    let report = json!({"schema":1,"mode":"rules_text_parse_only","passed":passed,
        "limits":{"request_bytes":REQUEST_BYTES,"output_bytes":OUTPUT_BYTES,"cases":CASES,
                  "text_bytes_per_case":TEXT_BYTES,"required_readings_per_case":READINGS,
                  "current_readings_per_case":engine.reading_limit},
        "scope":"Saved-meaning preservation diagnostic only. Old abilities validate before independent native canonicalization. Every old reading must remain. All refusals are reported; initially refused cases prove no coverage. Added readings and new recognitions require semantic review. No printing, full-card/source-context validation, equality or coverage proof.",
        "rows":rows});
    let mut bytes = BoundedOutput(Vec::new());
    serde_json::to_writer_pretty(&mut bytes, &report).expect("bounded complete output");
    std::fs::write(output, bytes.0).expect("write report");
    assert!(
        passed,
        "saved meaning lost or old operands invalid; see output JSON"
    );
}
