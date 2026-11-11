//! Batched, parse-only diagnostics for synthetic experiments and unsealed triage.
use oracle_ir_grammar::{corpus, engine::Engine};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    cases: Vec<Case>,
    #[serde(default)]
    include_readings: bool,
    #[serde(default)]
    include_candidate_ir: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: usize,
    name: String,
    text: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--module="))
        .collect();
    let [path] = args.as_slice() else {
        return Err("usage: inspect <request.json> [--module=name=path]...".into());
    };
    if std::fs::metadata(path)?.len() > 16 * 1024 * 1024 {
        return Err("inspection input exceeds 16 MiB".into());
    }
    let request: Request = serde_json::from_slice(&std::fs::read(path)?)?;
    let limit = if request.include_candidate_ir {
        64
    } else if request.include_readings {
        256
    } else {
        4096
    };
    if request.cases.len() > limit {
        return Err("inspection case limit exceeded (64 with candidate IR; 256 with readings; 4096 otherwise)".into());
    }
    let engine = Engine::load_from_args()?;
    let rows: Vec<Value> = corpus::parallel(&request.cases, |case| {
        let attempt = if request.include_candidate_ir {
            engine.inspect_with_candidate_ir(&case.name, &case.text)
        } else {
            engine.inspect(&case.name, &case.text)
        };
        let canonical_ir = request.include_readings.then(|| {
            let values = attempt.result.as_ref().ok().map(|p| &p.readings);
            let mut bytes_left = 64 * 1024;
            let mut output = Vec::new();
            if let Some(values) = values {
                for value in values.iter().take(16) {
                    let Some(value) =
                        oracle_ir_grammar::engine::diagnostics::bounded_ir(value, bytes_left)
                    else {
                        break;
                    };
                    bytes_left -= serde_json::to_vec(&value).expect("JSON value").len();
                    output.push(value);
                }
            }
            json!({"truncated": values.is_some_and(|v| v.len() > output.len()), "values":output,
                   "max_readings":16,"max_bytes":65536})
        });
        let (status, readings, refusal) = match attempt.result {
            Ok(parsed) => ("recognized", parsed.readings.len(), None),
            Err(why) => ("refused", 0, Some(format!("{why:?}"))),
        };
        let mut row = json!({"id":case.id,"name":case.name,"status":status,"readings":readings,
               "refusal":refusal,"evidence":attempt.evidence});
        if let Some(ir) = canonical_ir {
            row["canonical_ir"] = ir;
        }
        row
    })?;
    println!(
        "{}",
        json!({"schema":1,"mode":"rules_text_parse_only","rows":rows})
    );
    Ok(())
}
