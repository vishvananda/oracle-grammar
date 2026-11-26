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
        let Some(Instruction::Effect { result, operation }) =
            lossless::<Instruction>(&before[index - 1], tracing)
        else {
            continue;
        };
        if !matches!(operation, Operation::ChooseObjects { .. })
            || !crate::semantics::singular_operation(&operation, |_| false)
            || lossless::<Instruction>(&after[0], tracing).is_none()
        {
            continue;
        }
        if after[0]["kind"] != "Effect" {
            continue;
        }
        let next = &mut after[0]["args"]["operation"];
        if !matches!(
            next["kind"].as_str(),
            Some("Move" | "Reveal" | "Destroy" | "DiscardObjects" | "SetTapped")
        ) {
            continue;
        }
        let objects = &mut next["args"]["objects"];
        if inverse && *objects == json!({"kind":"Selected","args":result.0}) {
            *objects = json!({"kind":"One","args":{"kind":"Result","args":{
                "result":result.0,"phase":"After","index":0}}});
            changed = true;
        } else if !inverse && selected(objects, u64::from(result.0)) {
            let result = objects["args"]["args"]["result"].take();
            *objects = json!({"kind":"Selected","args":result});
            changed = true;
        }
    }
    changed
}
