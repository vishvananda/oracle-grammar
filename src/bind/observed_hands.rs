//! Spell an immediate whole-hand observation through its equivalent live hand.
//! The owned normalizer proves the inverse; no source wording is retained here.
use crate::model::canonical;
use serde_json::{Value, json};

pub(super) fn surface(root: &Value) -> Option<Value> {
    crate::functions::branch_bounds(root).ok()?;
    let mut copy = root.clone();
    let mut work = 131_072;
    if !walk(&mut copy, 0, &mut work)? {
        return None;
    }
    // This also rejects mismatched owners, ambiguous result definitions and
    // observation scopes that the owned contract cannot normalize back.
    let mut before = json!([root]);
    let mut after = json!([copy]);
    canonical::abilities(&mut before).ok()?;
    canonical::abilities(&mut after).ok()?;
    (before == after).then_some(copy)
}

fn choice(first: &Value, next: &mut Value) -> bool {
    if first["kind"] != "Effect"
        || next["kind"] != "Effect"
        || !matches!(
            first["args"]["operation"]["kind"].as_str(),
            Some("Reveal" | "LookAt")
        )
        || next["args"]["operation"]["kind"] != "ChooseObjects"
    {
        return false;
    }
    let hand = &first["args"]["operation"]["args"]["objects"];
    if hand["kind"] != "Zone"
        || hand["args"]["kind"] != "Card"
        || hand["args"]["filter"] != json!({"kind":"Any"})
        || hand["args"]["scope"]["Players"]["zone"] != "Hand"
        || hand["args"]["scope"]["Players"]["players"]["kind"] != "One"
    {
        return false;
    }
    let owner = &hand["args"]["scope"]["Players"]["players"];
    let objects = &mut next["args"]["operation"]["args"]["objects"];
    if objects["kind"] != "Matching"
        || objects["args"]["objects"]
            != json!({"kind":"InZone","args":{
                "objects":{"kind":"Observed","args":first["args"]["result"]},"zone":"Hand"
            }})
    {
        return false;
    }
    let filter = &objects["args"]["filter"];
    let owned = json!({"kind":"OwnedBy","args":owner});
    let filter = if *filter == owned {
        json!({"kind":"Any"})
    } else {
        let Some(parts) = filter["args"]
            .as_array()
            .filter(|_| filter["kind"] == "AllOf")
        else {
            return false;
        };
        let Some(index) = parts.iter().position(|part| *part == owned) else {
            return false;
        };
        let mut rest = parts.clone();
        rest.remove(index);
        match rest.len() {
            0 => json!({"kind":"Any"}),
            1 => rest.remove(0),
            _ => json!({"kind":"AllOf","args":rest}),
        }
    };
    let mut zone = hand.clone();
    zone["args"]["filter"] = filter;
    *objects = zone;
    true
}

fn walk(value: &mut Value, depth: usize, work: &mut usize) -> Option<bool> {
    if depth > 128 {
        return None;
    }
    *work = work.checked_sub(1)?;
    let mut changed = false;
    if let Some(items) = value.get_mut("instructions").and_then(Value::as_array_mut) {
        for index in 1..items.len() {
            let (before, after) = items.split_at_mut(index);
            changed |= choice(&before[index - 1], &mut after[0]);
        }
    }
    match value {
        Value::Array(items) => {
            for child in items {
                changed |= walk(child, depth + 1, work)?;
            }
        }
        Value::Object(fields) => {
            for child in fields.values_mut() {
                changed |= walk(child, depth + 1, work)?;
            }
        }
        _ => {}
    }
    Some(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_normalizers_exact_adjacent_hand_proof_supplies_a_surface() {
        let text = "Target opponent reveals their hand. You choose a nonland card from it. That player discards that card.";
        let parsed = crate::test_support::engine()
            .parse("Example", text)
            .unwrap();
        let root = &parsed.readings[0][0];
        assert!(surface(root).is_some());
        let mut foreign = root.clone();
        foreign["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["objects"]["args"]
            ["objects"]["args"]["objects"]["args"] = json!(123);
        assert!(surface(&foreign).is_none());
        let mut intervening = root.clone();
        intervening["args"]["body"]["instructions"].as_array_mut().unwrap().insert(1,
            json!({"kind":"Effect","args":{"result":987,"operation":{"kind":"Draw","args":{
                "players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}
            }}}}));
        assert!(surface(&intervening).is_none());
    }
}
