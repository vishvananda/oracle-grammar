//! Original printed fields remain independent of native game characteristics.
use oracle_ir_grammar::engine::PrintedPowerToughness;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, hash_map::Entry};
use std::io::{BufRead, BufReader};

pub(super) type Sources = HashMap<(Vec<String>, String), SourceFace>;

fn record_names(faces: &[Value]) -> Vec<String> {
    let mut names: Vec<_> = faces
        .iter()
        .filter_map(|face| face["name"].as_str().map(str::to_owned))
        .collect();
    names.sort();
    names
}

/// Join by the enclosing card's face names as well as the face itself. A spell
/// face and an unrelated standalone card can legitimately share a name.
pub(super) fn faces(path: &str, wanted: &BTreeSet<String>) -> Result<Vec<Value>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut selected = Vec::new();
    for line in BufReader::new(file).lines() {
        let record: Value =
            serde_json::from_str(&line.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let faces = record["faces"].as_array().map_or(&[][..], Vec::as_slice);
        let names = record_names(faces);
        for face in faces {
            if face["name"]
                .as_str()
                .is_some_and(|name| wanted.contains(name))
            {
                let mut face = face.clone();
                face["_source_record_names"] = serde_json::json!(names);
                selected.push(face);
            }
        }
    }
    Ok(selected)
}

pub(super) fn original<'a>(sources: &'a Sources, face: &Value) -> Option<&'a SourceFace> {
    let names = face["_source_record_names"]
        .as_array()?
        .iter()
        .map(|name| name.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    sources.get(&(names, face["name"].as_str()?.to_owned()))
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SourceFace {
    pub text: String,
    pub printed_power_toughness: PrintedPowerToughness,
    printed_source_fields: (Option<Value>, Option<Value>),
}

pub(super) fn load(path: &str) -> Result<Sources, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let records: Vec<Value> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    records_source(&records)
}

fn records_source(records: &[Value]) -> Result<Sources, String> {
    let mut faces = HashMap::new();
    for record in records {
        let cards = record["cards"].as_array().map_or(&[][..], Vec::as_slice);
        let names = record_names(cards);
        for face in cards {
            let Some(name) = face["name"].as_str() else {
                continue;
            };
            let printed_power_toughness = if face.get("power").is_none_or(Value::is_null)
                && face.get("toughness").is_none_or(Value::is_null)
            {
                PrintedPowerToughness::Absent
            } else if let Some((power, toughness)) = face["power"]
                .as_str()
                .and_then(|p| p.parse::<i32>().ok())
                .zip(
                    face["toughness"]
                        .as_str()
                        .and_then(|t| t.parse::<i32>().ok()),
                )
            {
                PrintedPowerToughness::Known { power, toughness }
            } else {
                PrintedPowerToughness::Unsupported
            };
            let source = SourceFace {
                text: face["text"].as_str().unwrap_or_default().to_owned(),
                printed_power_toughness,
                printed_source_fields: (face.get("power").cloned(), face.get("toughness").cloned()),
            };
            match faces.entry((names.clone(), name.to_owned())) {
                Entry::Vacant(entry) => {
                    entry.insert(source);
                }
                Entry::Occupied(entry) if entry.get() == &source => {}
                Entry::Occupied(_) => {
                    return Err(
                        "conflicting original source metadata for the same card and face".into(),
                    );
                }
            }
        }
    }
    Ok(faces)
}

#[cfg(test)]
mod tests {
    use super::records_source;
    use serde_json::json;
    #[test]
    fn duplicate_source_names_require_identical_text_and_raw_printed_fields() {
        let card = json!({"name":"Duplicate","text":"Station","power":"3","toughness":"3"});
        assert_eq!(
            records_source(&[json!({"cards":[card.clone(),card.clone()]})])
                .unwrap()
                .len(),
            1
        );
        for (field, value) in [("text", "Station\n7+ | Flying"), ("power", "4")] {
            let mut changed = card.clone();
            changed[field] = json!(value);
            assert!(records_source(&[json!({"cards":[card.clone(),changed]})]).is_err());
        }
        let mut star = card.clone();
        star["power"] = json!("*");
        let mut changed = star.clone();
        changed["power"] = json!("1+*");
        assert!(records_source(&[json!({"cards":[star,changed]})]).is_err());
    }

    #[test]
    fn same_face_name_on_distinct_cards_has_independent_source_metadata() {
        let sources = records_source(&[
            json!({"cards":[{"name":"Saw","text":"Equip {2}."}]}),
            json!({"cards":[{"name":"Smelt","text":"Destroy target artifact."},
                {"name":"Saw","text":"Destroy target creature."}]}),
        ])
        .unwrap();
        assert_eq!(
            sources[&(vec!["Saw".into()], "Saw".into())].text,
            "Equip {2}."
        );
        let key = (vec!["Saw".into(), "Smelt".into()], "Saw".into());
        assert_eq!(sources[&key].text, "Destroy target creature.");
        assert_eq!(
            super::original(
                &sources,
                &json!({"name":"Saw","_source_record_names":["Saw","Smelt"]})
            )
            .unwrap()
            .text,
            "Destroy target creature."
        );
    }
}
