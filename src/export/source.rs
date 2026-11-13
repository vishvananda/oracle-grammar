//! Conservative original-provider metadata conversion, independent of gold IR.
use super::SourceFace;
use crate::engine::PrintedPowerToughness;
use crate::model::{
    ir,
    ir_schema::{Fields, Schema, TypeDef},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::OnceLock;

fn vocabulary<T: DeserializeOwned>(family: &str, word: &str) -> Result<T, String> {
    static SCHEMA: OnceLock<Result<Schema, String>> = OnceLock::new();
    let schema = SCHEMA
        .get_or_init(|| Schema::from_sources(&[oracle_ir::declarations::SOURCE_VOCABULARY]))
        .as_ref()
        .map_err(Clone::clone)?;
    let Some(TypeDef::Enum { variants, .. }) = schema.types.get(family) else {
        return Err(format!("missing vocabulary {family}"));
    };
    let key = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let matches: Vec<_> = variants
        .iter()
        .filter(|(name, fields)| matches!(fields, Fields::Unit) && key(name) == key(word))
        .collect();
    if matches.len() != 1 {
        return Err(format!("unsupported {family}: {word}"));
    }
    serde_json::from_value(json!(matches[0].0)).map_err(|e| e.to_string())
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

fn characteristics(face: &Value) -> Result<(ir::Characteristics, PrintedPowerToughness), String> {
    let mut out = crate::engine::empty_card_characteristics();
    let types = face["type_line"]
        .as_str()
        .ok_or("missing source type_line")?;
    let (head, tail) = types
        .split_once('—')
        .map_or((types, ""), |(a, b)| (a.trim(), b.trim()));
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
    if !tail.is_empty() {
        // Plane subtypes can contain spaces. Prefer the entire known subtype;
        // otherwise each space-separated word must be a closed vocabulary item.
        if let Ok(value) = vocabulary("SubType", tail) {
            out.subtypes.push(value);
        } else {
            for word in tail.split_whitespace() {
                out.subtypes.push(vocabulary("SubType", word)?);
            }
        }
    }
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
    let power = integer(face, "power")?;
    let toughness = integer(face, "toughness")?;
    let printed = match (power, toughness) {
        (None, None) => PrintedPowerToughness::Absent,
        (Some(power), Some(toughness)) => PrintedPowerToughness::Known { power, toughness },
        _ => return Err("incomplete printed P/T".into()),
    };
    out.power_toughness = power.zip(toughness);
    out.loyalty = integer(face, "loyalty")?.map(|n| ir::Expr::Constant(i64::from(n)));
    out.defense = integer(face, "defense")?;
    Ok((out, printed))
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
            out.push(SourceFace {
                identity: json!({"provider_id":id,"record_index":record_index,"face_index":face_index,"layout":layout,"record_name":record["name"]}),
                name, text, source, context,
            });
        }
    }
    Ok(out)
}
