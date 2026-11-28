//! Conservative original-provider metadata conversion, independent of gold IR.
use super::SourceFace;
use crate::engine::{PrintedPowerToughness, PrintedStat};
use crate::model::mtg_types::{CardType, SubType};
use crate::model::{
    ir,
    ir_schema::{Fields, Schema, TypeDef},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::OnceLock;

type Words = HashMap<String, Option<String>>;
type Vocabulary = HashMap<&'static str, Words>;

fn word_key(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn source_vocabulary() -> Result<&'static Vocabulary, String> {
    static VOCABULARY: OnceLock<Result<Vocabulary, String>> = OnceLock::new();
    VOCABULARY
        .get_or_init(|| {
            let schema = Schema::from_sources(&[oracle_ir::declarations::SOURCE_VOCABULARY])?;
            let mut vocabulary = HashMap::new();
            for family in ["CardType", "SuperType", "SubType"] {
                let Some(TypeDef::Enum { variants, .. }) = schema.types.get(family) else {
                    return Err(format!("missing vocabulary {family}"));
                };
                let mut words = Words::new();
                for (name, fields) in variants {
                    if matches!(fields, Fields::Unit) {
                        words
                            .entry(word_key(name))
                            .and_modify(|previous| *previous = None)
                            .or_insert_with(|| Some(name.clone()));
                    }
                }
                vocabulary.insert(family, words);
            }
            Ok(vocabulary)
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn vocabulary<T: DeserializeOwned>(family: &str, word: &str) -> Result<T, String> {
    let name = source_vocabulary()?[family]
        .get(&word_key(word))
        .and_then(Option::as_ref)
        .ok_or_else(|| format!("unsupported {family}: {word}"))?;
    serde_json::from_value(json!(name)).map_err(|e| e.to_string())
}

fn subtypes(text: &str) -> Result<Vec<SubType>, String> {
    const MAX_WORDS: usize = 32;
    if text.len() > 1024 {
        return Err("printed subtype byte limit".into());
    }
    let words: Vec<_> = text
        .split_whitespace()
        .take(MAX_WORDS + 1)
        .map(word_key)
        .collect();
    if words.len() > MAX_WORDS || words.iter().any(String::is_empty) {
        return Err("invalid printed subtype words".into());
    }
    let known = &source_vocabulary()?["SubType"];
    // Count complete segmentations, saturating at two. A multiword subtype
    // can share a type line with other subtypes and must consume whole words.
    let mut counts = vec![0; words.len() + 1];
    let mut successors = vec![None; words.len()];
    counts[words.len()] = 1;
    for start in (0..words.len()).rev() {
        let mut key = String::new();
        for (end, word) in words.iter().enumerate().skip(start) {
            key.push_str(word);
            if counts[end + 1] == 0 {
                continue;
            }
            if let Some(Some(name)) = known.get(&key) {
                counts[start] += counts[end + 1];
                if counts[start] > 1 {
                    counts[start] = 2;
                    break;
                }
                successors[start] = Some((name, end + 1));
            }
        }
    }
    match counts[0] {
        0 => return Err(format!("unsupported SubType sequence: {text}")),
        1 => {}
        _ => return Err(format!("ambiguous SubType sequence: {text}")),
    }
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(Some((name, next))) = successors.get(start) {
        out.push(serde_json::from_value(json!(name)).map_err(|e| e.to_string())?);
        start = *next;
    }
    Ok(out)
}

fn color(word: &str) -> Result<ir::ManaColor, String> {
    Ok(match word {
        "W" => ir::ManaColor::White,
        "U" => ir::ManaColor::Blue,
        "B" => ir::ManaColor::Black,
        "R" => ir::ManaColor::Red,
        "G" => ir::ManaColor::Green,
        "C" => ir::ManaColor::Colorless,
        _ => return Err(format!("unsupported mana color {word}")),
    })
}

fn mana(mut text: &str) -> Result<Vec<ir::ManaSymbol>, String> {
    use ir::ManaSymbol as M;
    let mut out = Vec::new();
    while !text.is_empty() {
        let rest = text
            .strip_prefix('{')
            .ok_or("mana symbol must start with brace")?;
        let (symbol, rest) = rest.split_once('}').ok_or("unclosed mana symbol")?;
        let parts: Vec<_> = symbol.split('/').collect();
        out.push(match parts.as_slice() {
            ["X"] => M::X,
            ["S"] => M::Snow,
            [n] if n.parse::<u32>().is_ok() => M::Generic(n.parse().map_err(|_| "mana number")?),
            [c] => M::Color(color(c)?),
            ["2", c] => M::MonocoloredHybrid(color(c)?),
            [a, "P"] => M::Phyrexian(vec![color(a)?]),
            [a, b, "P"] => M::Phyrexian(vec![color(a)?, color(b)?]),
            [a, b] => M::Hybrid(color(a)?, color(b)?),
            _ => return Err(format!("unsupported mana symbol {symbol}")),
        });
        text = rest;
    }
    Ok(out)
}

fn integer(face: &Value, field: &str) -> Result<Option<i32>, String> {
    match face.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => s
            .parse()
            .map(Some)
            .map_err(|_| format!("unsupported printed {field}: {s}")),
        _ => Err(format!("printed {field} must be a string")),
    }
}

fn type_line(face: &Value) -> Result<(&str, &str), String> {
    let types = face["type_line"]
        .as_str()
        .ok_or("missing source type_line")?;
    if types.len() > 4096 {
        return Err("printed type line byte limit".into());
    }
    Ok(types
        .split_once('—')
        .map_or((types, ""), |(a, b)| (a.trim(), b.trim())))
}

fn printed_stat(value: &Value, field: &str) -> Result<Option<PrintedStat>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let value = value
        .as_str()
        .ok_or_else(|| format!("printed {field} must be a string"))?;
    if let Ok(fixed) = value.parse() {
        return Ok(Some(PrintedStat::Fixed(fixed)));
    }
    if value == "*" {
        return Ok(Some(PrintedStat::Star {
            offset: 0,
            subtract: false,
        }));
    }
    let shifted = value
        .strip_suffix("+*")
        .or_else(|| value.strip_prefix("*+"));
    let (offset, subtract) = shifted
        .map(|n| (n, false))
        .or_else(|| value.strip_suffix("-*").map(|n| (n, true)))
        .ok_or_else(|| format!("unsupported printed {field}: {value}"))?;
    let offset = offset
        .parse::<u32>()
        .ok()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| format!("unsupported printed {field}: {value}"))?;
    Ok(Some(PrintedStat::Star { offset, subtract }))
}

fn printed_stats(face: &Value) -> Result<PrintedPowerToughness, String> {
    match (
        printed_stat(&face["power"], "power")?,
        printed_stat(&face["toughness"], "toughness")?,
    ) {
        (None, None) => Ok(PrintedPowerToughness::Absent),
        (Some(PrintedStat::Fixed(power)), Some(PrintedStat::Fixed(toughness))) => {
            Ok(PrintedPowerToughness::Known { power, toughness })
        }
        (Some(power), Some(toughness)) => Ok(PrintedPowerToughness::Variable { power, toughness }),
        _ => Err("incomplete printed P/T".into()),
    }
}

fn characteristics(face: &Value) -> Result<(ir::Characteristics, PrintedPowerToughness), String> {
    let mut out = crate::engine::empty_card_characteristics();
    let (head, tail) = type_line(face)?;
    for word in head.split_whitespace() {
        if let Ok(value) = vocabulary("SuperType", word) {
            out.supertypes.push(value);
        } else {
            out.card_types.push(vocabulary("CardType", word)?);
        }
    }
    if out.card_types.is_empty() {
        return Err("missing source card type".into());
    }
    out.subtypes = match out.card_types.as_slice() {
        // A plane has one subtype, even when its name contains several words (CR 311.3).
        [CardType::Plane] if !tail.is_empty() => vec![vocabulary("SubType", tail)?],
        _ => subtypes(tail)?,
    };
    out.mana_cost = face
        .get("mana_cost")
        .filter(|v| !v.is_null() && v.as_str() != Some(""))
        .map(|v| mana(v.as_str().ok_or("mana_cost must be a string")?))
        .transpose()?;
    if let Some(colors) = face.get("color_indicator") {
        for item in colors
            .as_array()
            .ok_or("color_indicator must be an array")?
        {
            out.color_indicator
                .push(color(item.as_str().ok_or("color_indicator member")?)?);
        }
    }
    let printed = printed_stats(face)?;
    out.power_toughness = printed.base();
    out.loyalty = if face["loyalty"] == "X"
        && out
            .card_types
            .iter()
            .any(|kind| matches!(kind, CardType::Planeswalker))
        && out.mana_cost.as_ref().is_some_and(|cost| {
            cost.iter()
                .any(|symbol| matches!(symbol, ir::ManaSymbol::X))
        }) {
        Some(ir::Expr::AnnouncedX)
    } else {
        integer(face, "loyalty")?.map(|n| ir::Expr::Constant(i64::from(n)))
    };
    out.defense = integer(face, "defense")?;
    Ok((out, printed))
}

fn adventure_partner_types(layout: &str, faces: &[Value], face_index: usize) -> Vec<CardType> {
    if layout != "adventure" || faces.len() != 2 {
        return Vec::new();
    }
    let Some((head, _)) = faces
        .get(face_index ^ 1)
        .and_then(|face| type_line(face).ok())
    else {
        return Vec::new();
    };
    head.split_whitespace()
        .filter_map(|word| vocabulary("CardType", word).ok())
        .collect()
}

pub(super) fn scryfall(records: &[Value]) -> Result<Vec<SourceFace>, String> {
    let mut out = Vec::new();
    for (record_index, record) in records.iter().enumerate() {
        if !record.is_object() {
            return Err(format!("record {record_index} is not an object"));
        }
        let id = record["oracle_id"]
            .as_str()
            .or_else(|| record["id"].as_str())
            .ok_or_else(|| format!("record {record_index} has no provider identity"))?;
        let layout = record["layout"].as_str().ok_or("record has no layout")?;
        let faces = match record.get("card_faces") {
            Some(value) => value
                .as_array()
                .filter(|v| !v.is_empty())
                .ok_or("empty or invalid card_faces")?
                .as_slice(),
            None => std::slice::from_ref(record),
        };
        for (face_index, face) in faces.iter().enumerate() {
            let name = face["name"]
                .as_str()
                .ok_or("source face has no name")?
                .to_owned();
            let text = match face.get("oracle_text") {
                None => "",
                Some(value) => value.as_str().ok_or("oracle_text must be a string")?,
            }
            .to_owned();
            let mut source = face.clone();
            source
                .as_object_mut()
                .ok_or("face must be an object")?
                .remove("card_faces");
            let context = characteristics(face);
            let transform_front = if layout == "transform" && face_index == 1 && faces.len() == 2 {
                faces[0]["name"].as_str().map(str::to_owned)
            } else {
                None
            };
            out.push(SourceFace {
                identity: json!({"provider_id":id,"record_index":record_index,"face_index":face_index,"layout":layout,"record_name":record["name"]}),
                name, text, source, context,
                adventure_partner_types: adventure_partner_types(layout, faces, face_index),
                transform_front,
            });
        }
    }
    Ok(out)
}
