//! "That card" names the one card the instruction just before it acted on.
//!
//! Each producer leaves that card under its own receipt: a reveal observes it, a
//! discard leaves its pre-move information, a one-card payment keeps its inputs, an
//! exile of a top card is its actual result and a moved target stays the target. A
//! reveal and then a move of the same top card still read the observed card. Without
//! one such producer immediately before, the phrase has no antecedent: it never falls
//! back to this card, an event object or an earlier instruction's card. "The revealed
//! card" and "the discarded card" name only a reveal's or a discard's card.
use super::{Scope, Step, exiled_card, references, symbol_name};
use serde_json::{Value, json};

const DEPTH: usize = 256;

/// How the instruction before acted on its one card.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Producer {
    Revealed,
    Discarded,
    Moved,
}

/// The noun that names the receipt: any producer's, or only one kind's.
fn named(name: &str) -> Option<Option<Producer>> {
    match name {
        "card_receipt" => Some(None),
        "revealed_card_receipt" => Some(Some(Producer::Revealed)),
        "discarded_card_receipt" => Some(Some(Producer::Discarded)),
        _ => None,
    }
}

/// Every receipt symbol, for the binder's dispatch.
pub(super) fn is_receipt(name: &str) -> bool {
    named(name).is_some()
}

fn at<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get(key)?,
            Step::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}

/// The innermost instruction sequence holding `path`, and the instruction's index.
fn enclosing(path: &[Step]) -> Option<(&[Step], usize)> {
    (1..path.len())
        .rev()
        .find_map(|depth| match (&path[depth - 1], &path[depth]) {
            (Step::Key(key), Step::Index(index)) if key == "instructions" => {
                Some((&path[..depth], *index))
            }
            _ => None,
        })
}

fn constant_one(value: &Value) -> bool {
    *value == json!({"kind":"Constant","args":1})
}

fn query(objects: Value) -> Value {
    json!({"kind":"Query","args":{"objects":objects,"index":0}})
}

/// A single card read straight off one player's library top.
fn top_card(objects: &Value) -> bool {
    let zone = &objects["args"]["objects"];
    objects["kind"] == "Take"
        && constant_one(&objects["args"]["count"])
        && objects["args"]["from"] == "Top"
        && zone["kind"] == "Zone"
        && zone["args"]["scope"]["Players"]["zone"] == "Library"
        && zone["args"]["scope"]["Players"]["players"]["kind"] == "One"
        && zone["args"]["kind"] == "Card"
        && zone["args"]["filter"]["kind"] == "Any"
}

fn effect(instruction: &Value) -> Option<(u64, &Value)> {
    if instruction["kind"] != "Effect" {
        return None;
    }
    let mut operation = &instruction["args"]["operation"];
    if symbol_name(operation) == Some("exiled_card_result") {
        operation = operation["args"].get(0)?;
    }
    Some((instruction["args"]["result"].as_u64()?, operation))
}

/// The receipt one producer leaves for its single card, if it proves one.
fn produced(
    instructions: &[Value],
    index: usize,
    introduced: &[references::TargetIntroduction],
    base: usize,
) -> Result<Option<(Value, Producer)>, String> {
    let Some((result, operation)) = effect(&instructions[index]) else {
        return Ok(None);
    };
    let args = &operation["args"];
    // A battlefield entry's operand is shaped into members only when the scope ends.
    let mut objects = &args["objects"];
    if symbol_name(objects) == Some("zone_members") {
        objects = &objects["args"][0];
    }
    Ok(match operation["kind"].as_str() {
        Some("Reveal") if top_card(&args["objects"]) || args["objects"]["kind"] == "One" => Some((
            query(json!({"kind":"Observed","args":result})),
            Producer::Revealed,
        )),
        Some("RevealFromHand")
            if args["actor"]["kind"] == "One" && constant_one(&args["count"]) =>
        {
            Some((
                query(json!({"kind":"Observed","args":result})),
                Producer::Revealed,
            ))
        }
        Some("Discard") if args["players"]["kind"] == "One" && constant_one(&args["amount"]) => {
            Some((
                query(json!({"kind":"Transition","args":{"result":result,"phase":"Before"}})),
                Producer::Discarded,
            ))
        }
        Some("Move") if top_card(&args["objects"]) => index
            .checked_sub(1)
            .and_then(|before| effect(&instructions[before]))
            .filter(|(_, reveal)| {
                reveal["kind"] == "Reveal" && reveal["args"]["objects"] == args["objects"]
            })
            .map(|(revealed, _)| {
                (
                    query(json!({"kind":"Observed","args":revealed})),
                    Producer::Revealed,
                )
            }),
        Some("Move") if objects["kind"] == "One" => {
            let moved = references::bound_reference(&objects["args"], introduced, base)?;
            (moved["kind"] == "Target" && moved["args"]["slot"].is_u64())
                .then_some((moved, Producer::Moved))
        }
        _ => None,
    })
}

/// A one-card payment "if you do" refers to: its inputs, before they moved.
fn paid(conditional: &Value, instructions: &[Value], index: usize) -> Option<(Value, Producer)> {
    let payment = &conditional["args"]["condition"];
    if payment["kind"] != "PaymentChosen" {
        return None;
    }
    let result = payment["args"].as_u64()?;
    let (id, operation) = effect(instructions.get(index.checked_sub(1)?)?)?;
    let parts = operation["args"]["cost"]["parts"].as_array()?;
    let producer = match parts.as_slice() {
        [part]
            if part
                .get("Discard")
                .is_some_and(|d| constant_one(&d["count"])) =>
        {
            Producer::Discarded
        }
        [part]
            if part
                .get("Reveal")
                .is_some_and(|r| r["count"].get("Exactly").is_some_and(constant_one)) =>
        {
            Producer::Revealed
        }
        _ => return None,
    };
    (id == result && operation["kind"] == "PayCost").then(|| {
        let inputs = json!({"kind":"CostObjects","args":{
            "payment":{"Resolution":result},"component":[0],"view":"Inputs","phase":"Before"
        }});
        (query(inputs), producer)
    })
}

/// The receipt of the one card the instruction before acted on, with its producer.
fn receipt(
    root: &Value,
    path: &[Step],
    scope: &Scope,
    introduced: &[references::TargetIntroduction],
) -> Result<(Value, Producer), String> {
    if let Ok(reference) = exiled_card::reference(scope, path) {
        return Ok((reference, Producer::Moved));
    }
    let mut path = path;
    for _ in 0..DEPTH {
        let (sequence, index) = enclosing(path).ok_or("that card needs an instruction")?;
        let instructions = at(root, sequence)
            .and_then(Value::as_array)
            .ok_or("that card's instruction sequence")?;
        if index > 0 {
            return produced(instructions, index - 1, introduced, scope.target_base)?
                .ok_or_else(|| "that card needs one card the instruction before acted on".into());
        }
        // First in a branch: what the branch's own guard follows, or a payment's inputs.
        let [.., Step::Key(args), Step::Key(branch), Step::Key(_)] = sequence else {
            return Err("that card has no preceding instruction".into());
        };
        if args != "args" || !matches!(branch.as_str(), "then" | "otherwise") {
            return Err("that card has no preceding instruction".into());
        }
        let conditional_path = &sequence[..sequence.len() - 3];
        let conditional = at(root, conditional_path).ok_or("that card's branch")?;
        if conditional["kind"] != "If" {
            return Err("that card has no preceding instruction".into());
        }
        let (outer, position) = enclosing(conditional_path).ok_or("that card's branch")?;
        let outer_instructions = at(root, outer)
            .and_then(Value::as_array)
            .ok_or("that card's branch sequence")?;
        if conditional["args"]["condition"]["kind"] == "PaymentChosen" {
            return paid(conditional, outer_instructions, position)
                .ok_or_else(|| "that card needs a one-card payment".into());
        }
        path = conditional_path;
    }
    Err("that card's branch depth".into())
}

pub(super) fn reference(
    name: &str,
    root: &Value,
    path: &[Step],
    scope: &Scope,
    introduced: &[references::TargetIntroduction],
) -> Result<Value, String> {
    let wanted = named(name).ok_or("unknown card receipt")?;
    let (reference, producer) = receipt(root, path, scope, introduced)?;
    if wanted.is_some_and(|wanted| wanted != producer) {
        return Err("the card before was not revealed or discarded as this noun says".into());
    }
    Ok(reference)
}
