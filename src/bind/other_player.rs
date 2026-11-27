//! A written player action can establish who a following "other player" excludes.
use super::{Scope, Step, symbol_name};
use serde_json::{Value, json};

fn keys(path: &[Step], wanted: &[&str]) -> bool {
    path.len() == wanted.len()
        && path
            .iter()
            .zip(wanted)
            .all(|(step, wanted)| matches!(step, Step::Key(key) if key.as_str() == *wanted))
}

fn operation(site: &Value) -> Option<&Value> {
    (site["kind"] == "Effect").then(|| &site["args"]["operation"])
}

fn after_controller_draw(instructions: &Value, index: usize) -> Option<()> {
    let current = operation(instructions.get(index)?)?;
    let previous = operation(instructions.get(index.checked_sub(1)?)?)?;
    (current["kind"] == "Draw"
        && previous["kind"] == "Draw"
        && previous["args"]["players"]["kind"] == "One"
        && previous["args"]["players"]["args"]["kind"] == "AbilityController")
        .then_some(())
}

fn after_controller_payment(instructions: &Value, index: usize, path: &[Step]) -> Option<()> {
    let previous = instructions.get(index.checked_sub(1)?)?;
    let pay = operation(previous)?;
    let mut branch = instructions.get(index)?;
    let mut tail = path;
    if matches!(symbol_name(branch), Some("leading_if" | "resolution")) {
        let [Step::Key(args), Step::Index(0), rest @ ..] = path else {
            return None;
        };
        if args != "args" {
            return None;
        }
        branch = branch["args"].get(0)?;
        tail = rest;
    }
    let [
        _,
        _,
        _,
        Step::Index(0),
        Step::Key(args),
        Step::Key(operation_key),
        suffix @ ..,
    ] = tail
    else {
        return None;
    };
    let condition = &branch["args"]["condition"];
    let receipt = &condition["args"];
    let current = operation(branch["args"]["then"]["instructions"].get(0)?)?;
    let (current, suffix) = if symbol_name(current) == Some("sacrificed_result") {
        let [Step::Key(args), Step::Index(0), rest @ ..] = suffix else {
            return None;
        };
        if args != "args" {
            return None;
        }
        (current["args"].get(0)?, rest)
    } else {
        (current, suffix)
    };
    (keys(&tail[..3], &["args", "then", "instructions"])
        && args == "args"
        && operation_key == "operation"
        && keys(suffix, &["args", "players", "args"])
        && branch["kind"] == "If"
        && condition["kind"] == "PaymentChosen"
        && (receipt.as_u64().is_some() && receipt == &previous["args"]["result"]
            || symbol_name(receipt) == Some("last_result"))
        && current["kind"] == "Sacrifice"
        && pay["kind"] == "PayCost"
        && pay["args"]["optional"] == true
        && pay["args"]["player"]["kind"] == "AbilityController"
        && pay["args"]["cost"]["parts"]
            .as_array()
            .is_some_and(|parts| matches!(parts.as_slice(), [part] if part.get("Life").is_some())))
    .then_some(())
}

pub(super) fn resolve(root: &Value, path: &[Step], scope: &Scope) -> Value {
    let from_ability_controller = (|| {
        let [_, _, _, Step::Index(index), suffix @ ..] = path else {
            return None;
        };
        if root["kind"] != "Triggered" || !keys(&path[..3], &["args", "body", "instructions"]) {
            return None;
        }
        let instructions = root.pointer("/args/body/instructions")?;
        if keys(suffix, &["args", "operation", "args", "players", "args"]) {
            after_controller_draw(instructions, *index)
        } else {
            after_controller_payment(instructions, *index, suffix)
        }
    })()
    .is_some();
    if !from_ability_controller && scope.player() == Some(json!({"kind":"EventSubject"})) {
        json!({"kind":"EventSubject"})
    } else {
        json!({"kind":"AbilityController"})
    }
}
