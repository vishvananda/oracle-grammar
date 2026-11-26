//! A directly preceding maximum-one choice has no arbitrary index-zero member.
use super::walk::lossless;
use crate::ir::{Instruction, Operation};
use serde_json::{Value, json};

fn selected(objects: &Value, result: u64) -> bool {
    let reference = &objects["args"];
    let args = &reference["args"];
    objects["kind"] == "One"
        && objects.as_object().is_some_and(|o| o.len() == 2)
        && reference["kind"] == "Result"
        && reference.as_object().is_some_and(|o| o.len() == 2)
        && args.as_object().is_some_and(|o| o.len() == 3)
        && super::trace::number(&args["result"]) == Some(result)
        && args["phase"] == "After"
        && args["index"] == 0
}

pub(super) fn program(program: &mut Value, tracing: bool) -> bool {
    rewrite(program, tracing, false)
}

pub(super) fn print_program(program: &mut Value) -> bool {
    rewrite(program, false, true)
}

fn rewrite(program: &mut Value, tracing: bool, inverse: bool) -> bool {
    let Some(items) = program
        .get_mut("instructions")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for index in 1..items.len() {
        let (before, after) = items.split_at_mut(index);
        let first = &before[index - 1];
        let next = &mut after[0];
        let Some(result) = candidate(first, next, inverse) else {
            continue;
        };
        let Some(Instruction::Effect { operation, .. }) = lossless::<Instruction>(first, tracing)
        else {
            continue;
        };
        if !matches!(operation, Operation::ChooseObjects { .. })
            || !crate::semantics::singular_operation(&operation, |_| false)
            || lossless::<Instruction>(next, tracing).is_none()
        {
            continue;
        }
        let objects = &mut next["args"]["operation"]["args"]["objects"];
        if inverse {
            *objects = json!({"kind":"One","args":{"kind":"Result","args":{
                "result":result,"phase":"After","index":0}}});
        } else {
            let result = objects["args"]["args"]["result"].take();
            *objects = json!({"kind":"Selected","args":result});
        }
        changed = true;
    }
    changed
}

fn candidate(first: &Value, next: &Value, inverse: bool) -> Option<u64> {
    if first["kind"] != "Effect"
        || first["args"]["operation"]["kind"] != "ChooseObjects"
        || next["kind"] != "Effect"
    {
        return None;
    }
    let operation = &next["args"]["operation"];
    if !matches!(
        operation["kind"].as_str(),
        Some("Move" | "Reveal" | "Destroy" | "DiscardObjects" | "SetTapped")
    ) {
        return None;
    }
    let id = super::trace::number(&first["args"]["result"])?;
    let objects = &operation["args"]["objects"];
    let matches = if inverse {
        objects["kind"] == "Selected"
            && objects.as_object().is_some_and(|o| o.len() == 2)
            && objects["args"].as_u64() == Some(id)
    } else {
        selected(objects, id)
    };
    matches.then_some(id)
}

pub(super) fn may_print(items: &[Value]) -> bool {
    items
        .windows(2)
        .any(|pair| candidate(&pair[0], &pair[1], true).is_some())
}
