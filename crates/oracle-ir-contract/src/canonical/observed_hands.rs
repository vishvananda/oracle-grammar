//! An immediate choice from a whole observed hand names that observation.
use super::walk::{Uses, lossless};
use crate::ir::{
    EventZone, Instruction, ObjectFilter, ObjectKind, ObjectSet, Operation, PlayerSet, ZoneScope,
};
use serde_json::{Value, json};

fn hand(objects: &ObjectSet) -> Option<&PlayerSet> {
    match objects {
        ObjectSet::Zone {
            scope:
                ZoneScope::Players {
                    zone: EventZone::Hand,
                    players,
                },
            kind: ObjectKind::Card,
            ..
        } if matches!(players, PlayerSet::One(_)) => Some(players),
        _ => None,
    }
}

fn candidate(first: &Value, next: &Value, uses: &Uses, tracing: bool) -> bool {
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
    let Some(Instruction::Effect { result, operation }) = lossless(first, tracing) else {
        return false;
    };
    let (objects, viewer) = match &operation {
        Operation::Reveal { objects } => (objects, None),
        Operation::LookAt { objects, player } => (objects, Some(player)),
        _ => return false,
    };
    let ObjectSet::Zone {
        filter: ObjectFilter::Any,
        ..
    } = objects
    else {
        return false;
    };
    let Some(owner) = hand(objects) else {
        return false;
    };
    let Some(Instruction::Effect {
        operation:
            Operation::ChooseObjects {
                player,
                objects: selected,
                ..
            },
        ..
    }) = lossless(next, tracing)
    else {
        return false;
    };
    if let Some(viewer) = viewer {
        let (Ok(viewer), Ok(player)) =
            (serde_json::to_value(viewer), serde_json::to_value(&player))
        else {
            return false;
        };
        if viewer != player {
            return false;
        }
    }
    let Some(selected_owner) = hand(&selected) else {
        return false;
    };
    // Typed player identities retain their original ID spelling until the
    // final numbering pass. No interval, branch or independent scope intervenes.
    let (Ok(owner), Ok(selected_owner)) = (
        serde_json::to_value(owner),
        serde_json::to_value(selected_owner),
    ) else {
        return false;
    };
    owner == selected_owner && uses.definitions.get(&u64::from(result.0)) == Some(&1)
}

pub(super) fn normalize(items: &mut [Value], uses: &Uses, tracing: bool) -> bool {
    let mut changed = false;
    for index in 1..items.len() {
        if !candidate(&items[index - 1], &items[index], uses, tracing) {
            continue;
        }
        let result = items[index - 1]["args"]["result"].clone();
        let objects = &mut items[index]["args"]["operation"]["args"]["objects"];
        let owner = objects["args"]["scope"]["Players"]["players"].clone();
        let filter = objects["args"]["filter"].take();
        *objects = json!({"kind":"Matching","args":{
            "objects":{"kind":"InZone","args":{
                "objects":{"kind":"Observed","args":result},"zone":"Hand"
            }},
            "filter":{"kind":"AllOf","args":[filter,{"kind":"OwnedBy","args":owner}]}
        }});
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests;
