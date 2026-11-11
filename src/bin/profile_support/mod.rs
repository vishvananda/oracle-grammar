//! Corpus preparation shared by the timing and allocation drivers.
use oracle_ir_grammar::corpus;
use oracle_ir_grammar::engine::presentation_free;
use serde_json::Value;
use std::collections::BTreeSet;

pub struct Card {
    pub name: String,
    pub text: String,
    pub abilities: Value,
    pub context: Vec<String>,
}

pub fn read(unified: &str, oracle: &str, slice: &str) -> Result<Vec<Card>, String> {
    let oracle = corpus::face_texts(oracle)?;
    let slice: Value = serde_json::from_slice(&std::fs::read(slice).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let wanted: BTreeSet<&str> = slice["cards"]
        .as_array()
        .ok_or("slice has no cards")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let mut cards = Vec::new();
    for face in corpus::faces(unified)? {
        let face = face?;
        let name = face["name"].as_str().unwrap_or_default();
        if wanted.contains(name) {
            cards.push(Card {
                name: name.to_string(),
                text: presentation_free(oracle.get(name).map_or("", String::as_str)),
                abilities: face["abilities"].clone(),
                context: corpus::print_context(&face),
            });
        }
    }
    Ok(cards)
}
