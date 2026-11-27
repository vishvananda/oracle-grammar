//! Consecutive independent player rules can share their continuous effect.
use super::{
    trace,
    walk::{Uses, lossless},
};
use crate::ir::Instruction;
use serde_json::{Value, json};

const LOSE_LIFE: u8 = 1;
const LOSE_GAME: u8 = 2;
const WIN_GAME: u8 = 4;
const PLAY_EXILE: u8 = 8;
const EXTRA_LAND: u8 = 16;
const OUTCOMES: u8 = LOSE_LIFE | LOSE_GAME | WIN_GAME;

fn dimensions(change: &Value) -> Option<u8> {
    if change["kind"] != "PlayerRule" {
        return None;
    }
    let rule = &change["args"];
    let permission = &rule["PlayPermission"];
    if rule["Prohibit"] == "LoseLife" {
        Some(LOSE_LIFE)
    } else if rule["Prohibit"] == json!({"LoseGame":{"reason":null}}) {
        Some(LOSE_GAME)
    } else if rule["Prohibit"] == "WinGame" {
        Some(WIN_GAME)
    } else if permission["from"] == "Exile"
        && permission["action"] == "Either"
        && permission["filter"] == json!({"kind":"Any"})
        && permission["cards"]["kind"] == "Transition"
        && permission["cards"]["args"]["phase"] == "After"
    {
        Some(PLAY_EXILE)
    } else if rule["AdditionalLandPlays"] == json!({"Exactly":{"kind":"Constant","args":1}}) {
        Some(EXTRA_LAND)
    } else {
        None
    }
}

fn effect(instruction: &Value) -> Option<&Value> {
    let operation = &instruction["args"]["operation"];
    (instruction["kind"] == "Effect"
        && operation["kind"] == "CreateContinuousEffect"
        && operation["args"]["source"] == json!({"kind":"Source"})
        && operation["args"]["affected"].get("Players").is_some()
        && operation["args"]["duration"] == "EndOfTurn"
        && operation["args"]["evaluation"] == "LiveObjects")
        .then_some(&operation["args"])
}

fn mask(changes: &[Value]) -> Option<u8> {
    if !(1..=3).contains(&changes.len()) {
        return None;
    }
    changes.iter().try_fold(0, |mask, change| {
        let bit = dimensions(change)?;
        (mask & bit == 0).then_some(mask | bit)
    })
}

pub(super) fn may_need(instruction: &Value) -> bool {
    effect(instruction)
        .and_then(|args| args["changes"].as_array())
        .is_some_and(|changes| mask(changes).is_some())
}

fn mergeable<'a>(
    first: &Value,
    next: &'a Value,
    uses: &Uses,
    tracing: bool,
) -> Option<&'a [Value]> {
    let (left, right) = (effect(first)?, effect(next)?);
    if !trace::same(&left["affected"], &right["affected"], tracing) {
        return None;
    }
    let a = left["changes"].as_array()?;
    let b = right["changes"].as_array()?;
    let (x, y) = (mask(a)?, mask(b)?);
    let combined = x | y;
    if x & y != 0 || !(combined & !OUTCOMES == 0 || combined == (PLAY_EXILE | EXTRA_LAND)) {
        return None;
    }
    for item in [first, next] {
        let id = trace::number(&item["args"]["result"])?;
        if uses.total.get(&id) != Some(&1)
            || uses.definitions.get(&id) != Some(&1)
            || lossless::<Instruction>(item, tracing).is_none()
        {
            return None;
        }
    }
    Some(b.as_slice())
}

pub(super) fn merge(items: &mut Vec<Value>, uses: &Uses, tracing: bool) -> bool {
    let mut index = 0;
    let mut changed = false;
    while index + 1 < items.len() {
        let (before, after) = items.split_at_mut(index + 1);
        if let Some(changes) = mergeable(&before[index], &after[0], uses, tracing) {
            before[index]["args"]["operation"]["args"]["changes"]
                .as_array_mut()
                .expect("checked change list")
                .extend(changes.iter().cloned());
            items.remove(index + 1);
            changed = true;
        } else {
            index += 1;
        }
    }
    changed
}
