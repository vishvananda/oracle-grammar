//! "Of the chosen type" names the kind of type the card itself chose.
use serde_json::Value;
use std::collections::BTreeSet;

const WORK: usize = 1 << 20;
const DEPTH: usize = 256;

struct Walk<'a> {
    work: usize,
    chosen: BTreeSet<&'a str>,
    used: BTreeSet<&'a str>,
}

fn type_kind(kind: &str) -> bool {
    kind.ends_with("Type")
}

/// The chosen kind a choice supplies: a permanent type is a card type, and a
/// basic or nonbasic land type is a land type.
fn supplied(kind: &str) -> &str {
    match kind {
        "PermanentType" => "CardType",
        "BasicLandType" | "NonbasicLandType" => "LandType",
        other => other,
    }
}

fn visit<'a>(node: &'a Value, depth: usize, walk: &mut Walk<'a>) -> Result<(), String> {
    if depth > DEPTH || walk.work == 0 {
        return Err("card exceeds the chosen-kind check bounds".into());
    }
    walk.work -= 1;
    match node {
        Value::Object(fields) => {
            match node["kind"].as_str() {
                Some("ChooseValue") => {
                    if let Some(kind) = node["args"]["kind"].as_str().filter(|k| type_kind(k)) {
                        walk.chosen.insert(supplied(kind));
                    }
                }
                Some("Chosen") => {
                    if let Some(kind) = node["args"].as_str().filter(|k| type_kind(k)) {
                        walk.used.insert(kind);
                    }
                }
                _ => {}
            }
            for child in fields.values() {
                visit(child, depth + 1, walk)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                visit(item, depth + 1, walk)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// A card that chooses a kind of type reads every chosen type as that kind.
/// Without its own choice, a chosen type keeps every kind its words allow.
pub(crate) fn check(card: &Value) -> Result<(), String> {
    let mut walk = Walk {
        work: WORK,
        chosen: BTreeSet::new(),
        used: BTreeSet::new(),
    };
    visit(card, 0, &mut walk)?;
    if walk.chosen.is_empty() || walk.used.is_subset(&walk.chosen) {
        return Ok(());
    }
    Err("a chosen type names the kind of type the card chose".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn card(chosen: &str, used: &str) -> Value {
        json!([
            {"kind":"ChooseValue","args":{"kind":chosen}},
            {"kind":"Chosen","args":used},
        ])
    }

    #[test]
    fn a_chosen_type_follows_the_card_choice() {
        assert!(check(&card("CreatureType", "CreatureType")).is_ok());
        assert!(check(&card("PlaneswalkerType", "CreatureType")).is_err());
        assert!(check(&json!([{"kind":"Chosen","args":"CardType"}])).is_ok());
        assert!(check(&card("Color", "CardType")).is_ok());
        assert!(check(&card("PermanentType", "CardType")).is_ok());
        assert!(check(&card("BasicLandType", "LandType")).is_ok());
        assert!(check(&card("NonbasicLandType", "CreatureType")).is_err());
    }

    #[test]
    fn the_walk_is_bounded() {
        let mut deep = json!({"kind":"Chosen","args":"CardType"});
        for _ in 0..300 {
            deep = json!([deep]);
        }
        assert!(check(&deep).is_err());
    }
}
