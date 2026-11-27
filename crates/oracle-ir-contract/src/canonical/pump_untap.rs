//! A Snapshot pump and its immediate untap share one recorded population.
use super::{
    trace,
    walk::{Uses, lossless},
};
use crate::ir::Instruction;
use serde_json::{Value, json};

pub(super) fn may_need(operation: &Value) -> bool {
    operation["kind"] == "NoteObjects"
        && super::battlefield_filter(&operation["args"]["objects"]).is_some()
}

fn selection(objects: &Value, id: u64) -> bool {
    objects["kind"] == "Selected" && trace::number(&objects["args"]) == Some(id)
}

fn pair(items: &[Value], uses: &Uses, tracing: bool) -> Option<u64> {
    let [note, pump, untap] = items else {
        return None;
    };
    let selection_op = &note["args"]["operation"];
    let effect = &pump["args"]["operation"];
    let tapped = &untap["args"]["operation"];
    let id = trace::number(&note["args"]["result"])?;
    let pump_id = trace::number(&pump["args"]["result"])?;
    let change = &effect["args"]["changes"];
    if !may_need(selection_op)
        || effect["kind"] != "CreateContinuousEffect"
        || effect["args"]["source"] != json!({"kind":"Source"})
        || effect["args"]["evaluation"] != "Snapshot"
        || effect["args"]["duration"] != "EndOfTurn"
        || !change.as_array().is_some_and(|changes| {
            matches!(changes.as_slice(),[pump]
            if pump["kind"]=="Pump" && super::scalar_reads::constant(&pump["args"]["power"])
            && super::scalar_reads::constant(&pump["args"]["toughness"]))
        })
        || !selection(&effect["args"]["affected"]["Objects"], id)
        || tapped["kind"] != "SetTapped"
        || tapped["args"]["tapped"] != false
        || !selection(&tapped["args"]["objects"], id)
        || uses.definitions.get(&id) != Some(&1)
        || uses.total.get(&id) != Some(&3)
        || uses.definitions.get(&pump_id) != Some(&1)
        || items
            .iter()
            .any(|i| i["kind"] != "Effect" || lossless::<Instruction>(i, tracing).is_none())
    {
        return None;
    }
    Some(pump_id)
}

pub(super) fn program(items: &mut Vec<Value>, uses: &Uses, tracing: bool) -> bool {
    let mut index = 0;
    let mut changed = false;
    while index + 2 < items.len() {
        if let Some(id) = pair(&items[index..index + 3], uses, tracing) {
            let population = items[index]["args"]["operation"]["args"]["objects"].take();
            items[index + 1]["args"]["operation"]["args"]["affected"]["Objects"] = population;
            let objects = &mut items[index + 2]["args"]["operation"]["args"]["objects"];
            let receipt = trace::with_number(&objects["args"], id);
            *objects = json!({"kind":"Affected","args":receipt});
            items.remove(index);
            changed = true;
            index += 1;
        }
        index += 1;
    }
    changed
}
