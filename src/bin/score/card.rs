//! Contextual raw printing retains note references separately from game meaning.
use super::{Engine, Score, Value, json};
use oracle_ir_grammar::engine::CardReading;
use oracle_ir_grammar::model::ir::Characteristics;

pub(super) fn render(
    engine: &Engine,
    face: &Value,
    reading: &CardReading,
    text: &str,
    row: &mut Value,
    score: &mut Score,
) -> Result<(), String> {
    if reading.notes.is_empty() && face.get("_source_context").is_none() {
        return super::render_row(engine, face, text, row, score);
    }
    let name = face["name"].as_str().unwrap_or_default();
    let context: Characteristics =
        serde_json::from_value(face["characteristics"].clone()).map_err(|e| e.to_string())?;
    let source = face
        .get("_source_context")
        .map(|value| {
            serde_json::from_value::<oracle_ir_grammar::engine::CardInputContext>(value.clone())
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    let printed = match &source {
        Some(source) => engine.print_card_keyed_with_source(name, reading, source, &[])?,
        None => engine.print_card_keyed(name, reading, &context, &[])?,
    };
    score.round_trip += u64::from(printed.is_some());
    let first = printed.as_deref() == Some(text);
    let mut exact = first;
    if !first {
        row["default"] = json!(printed);
        let key = match &source {
            Some(source) => engine.card_key_of_with_source(name, reading, source, text, false)?,
            None => engine.card_key_of(name, reading, &context, text, false)?,
        };
        if let Some(key) = key {
            score.reachable += 1;
            exact = match &source {
                Some(source) => engine.print_card_keyed_with_source(name, reading, source, &key)?,
                None => engine.print_card_keyed(name, reading, &context, &key)?,
            }
            .as_deref()
                == Some(text);
            row["key"] = json!(key);
        }
        let trail = if exact {
            None
        } else {
            match &source {
                Some(source) => {
                    engine.card_key_of_with_source(name, reading, source, text, true)?
                }
                None => engine.card_key_of(name, reading, &context, text, true)?,
            }
        };
        if let Some(trail) = trail {
            exact = match &source {
                Some(source) => {
                    engine.print_card_trail_with_source(name, reading, source, &trail)?
                }
                None => engine.print_card_trail(name, reading, &context, &trail)?,
            }
            .as_deref()
                == Some(text);
            if exact {
                score.ordered_keys += 1;
                row["key"] = json!(trail);
                row["ordered"] = json!(true);
            }
        }
        row["first"] = json!(printed);
    }
    score.exact_no_key += u64::from(first);
    score.exact_key += u64::from(exact);
    row["exact"] = json!(exact);
    if !exact {
        row["rendered"] = json!(printed.iter().collect::<Vec<_>>());
    }
    Ok(())
}

pub(super) fn round_trip(
    engine: &Engine,
    face: &Value,
    reading: &CardReading,
    row: &mut Value,
    score: &mut Score,
) -> Result<(), String> {
    if reading.notes.is_empty() && face.get("_source_context").is_none() {
        return super::round_trip_row(engine, face, row, score);
    }
    let name = face["name"].as_str().unwrap_or_default();
    let context: Characteristics =
        serde_json::from_value(face["characteristics"].clone()).map_err(|e| e.to_string())?;
    let source = face
        .get("_source_context")
        .map(|value| {
            serde_json::from_value::<oracle_ir_grammar::engine::CardInputContext>(value.clone())
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    let texts = match &source {
        Some(source) => engine.enumerate_card_with_source(name, reading, source, 8, &[])?,
        None => engine.enumerate_card(name, reading, &context, 8, &[])?,
    }
    .texts;
    let count = texts.len() as u64;
    let lost: Vec<_> = texts
        .into_iter()
        .filter(|text| {
            let parsed = match &source {
                Some(source) => engine.parse_card_with_source(name, text, source),
                None => engine.parse_card(name, text, &context),
            };
            !parsed.is_ok_and(|parsed| parsed.readings.contains(reading))
        })
        .collect();
    let same = count - lost.len() as u64;
    score.candidates += count;
    score.candidates_kept += same;
    row["round_trip"] = json!([count, same]);
    if !lost.is_empty() {
        row["lost"] = json!(lost);
    }
    Ok(())
}
