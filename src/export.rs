//! Native face distribution: every input occurrence has an explicit outcome.
//! Source characteristics are read independently of reference IR. Printing is
//! deliberately a separate gate; these exports claim parsing and validation.
use crate::engine::{CardReading, Engine, Refusal};
use crate::model::{canonical, ir, validate};
use serde_json::{Value, json};

mod source;
#[cfg(test)]
#[path = "tests/native_export.rs"]
mod tests;

/// One source occurrence, identified by provider record and face position.
pub struct SourceFace {
    pub identity: Value,
    pub name: String,
    pub text: String,
    pub source: Value,
    pub context: Result<(ir::Characteristics, crate::engine::PrintedPowerToughness), String>,
}

/// Decode original Scryfall Oracle bulk JSON, without rewriting Oracle text.
/// Unsupported source characteristics become per-face refusals, never defaults.
pub fn scryfall(records: &[Value]) -> Result<Vec<SourceFace>, String> {
    source::scryfall(records)
}

fn refusal(why: Refusal) -> (&'static str, String) {
    match why {
        Refusal::Limit(why) => ("limit", why),
        Refusal::Lexical(why) => ("lexical", why),
        Refusal::Binding(why) => ("binding", why),
        Refusal::NoParse { furthest, tokens } => (
            "no_parse",
            format!("stopped at token {furthest} of {tokens}"),
        ),
    }
}

fn cards(
    name: &str,
    characteristics: &ir::Characteristics,
    readings: Vec<CardReading>,
) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    for reading in readings {
        let abilities = serde_json::from_value(reading.abilities).map_err(|e| e.to_string())?;
        let card = ir::Card {
            schema_version: ir::SCHEMA_VERSION,
            name: name.to_owned(),
            characteristics: characteristics.clone(),
            abilities,
        };
        validate::card(&card)?;
        let card = canonical::card(&card)?;
        validate::card(&card)?;
        let value = json!({"card":card,"notes":reading.notes});
        if !out.contains(&value) {
            out.push(value);
        }
    }
    out.sort_by_cached_key(Value::to_string);
    Ok(out)
}

/// Exhaustive native results, including all distinct supported interpretations.
/// Any resource-limit result discards partial success for this occurrence.
pub fn outcome(engine: &Engine, face: &SourceFace) -> Value {
    let mut row = json!({
        "format_version":1, "schema_version":ir::SCHEMA_VERSION,
        "identity":face.identity, "name":face.name, "source":face.source,
        "status":"refused", "readings":[]
    });
    let result = (|| {
        let (characteristics, printed) = face
            .context
            .as_ref()
            .map_err(|why| ("unsupported_source", why.clone()))?;
        let mut context = engine
            .card_input_context(&face.name, &face.text, characteristics, printed.clone())
            .map_err(|why| ("source_context", why))?;
        // A Station box describes its conditional animation stage. It is not
        // an unconditional base P/T characteristic of the source artifact.
        if context.station_present {
            context.characteristics.power_toughness = None;
        }
        let parsed = engine
            .parse_card_with_source(&face.name, &face.text, &context)
            .map_err(refusal)?;
        let readings = cards(&face.name, &context.characteristics, parsed.readings)
            .map_err(|why| ("validation", why))?;
        if readings.is_empty() {
            return Err(("validation", "empty native reading set".into()));
        }
        let mut distinct_cards: Vec<&Value> = Vec::new();
        for reading in &readings {
            if !distinct_cards.contains(&&reading["card"]) {
                distinct_cards.push(&reading["card"]);
            }
        }
        row["status"] = json!(if distinct_cards.len() == 1 {
            "unique"
        } else {
            "ambiguous"
        });
        row["semantic_readings"] = json!(distinct_cards.len());
        row["readings"] = json!(readings);
        row["context"] = json!(context);
        row["work"] = json!(parsed.counters);
        Ok(())
    })();
    if let Err((stage, message)) = result {
        row["status"] = json!(if stage == "limit" { "limit" } else { "refused" });
        row["stage"] = json!(stage);
        row["message"] = json!(message);
    }
    row
}
