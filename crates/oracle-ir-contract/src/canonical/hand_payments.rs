//! Collapse a proved pay/receipt form of discarding the payer's whole hand.
use super::{
    trace,
    walk::{Uses, lossless},
};
use crate::ir::CostPart;
use serde_json::{Value, json};

fn controller() -> Value {
    json!({"kind":"One","args":{"kind":"AbilityController"}})
}

pub fn hand_discard(instruction: &Value) -> bool {
    if instruction["kind"] != "Effect" {
        return false;
    }
    let operation = &instruction["args"]["operation"];
    operation["kind"] == "Discard"
        && operation["args"]["players"] == controller()
        && operation["args"]["random"] == false
        && operation["args"]["amount"]
            == json!({"kind":"CountObjects","args":{"kind":"Zone","args":{
                "scope":{"Players":{"zone":"Hand","players":controller()}},
                "kind":"Card","filter":{"kind":"Any"}
            }}})
}

fn uses_of(uses: &Uses, value: &Value, count: usize) -> bool {
    trace::number(value).is_some_and(|id| {
        uses.total.get(&id) == Some(&count) && uses.definitions.get(&id) == Some(&1)
    })
}

fn whole_hand_part(effect: &Value, check: &Value, uses: &Uses, tracing: bool) -> bool {
    let pay = &effect["args"]["operation"];
    let controller = json!({"kind":"AbilityController"});
    if effect["kind"] != "Effect"
        || pay["kind"] != "PayCost"
        || pay["args"]["player"] != controller
        || pay["args"]["optional"] != true
        || check["kind"] != "If"
        || check["args"]["condition"]["kind"] != "PaymentChosen"
        || trace::number(&check["args"]["condition"]["args"])
            != trace::number(&effect["args"]["result"])
        || !uses_of(uses, &effect["args"]["result"], 2)
    {
        return false;
    }
    let Some([part]) = pay["args"]["cost"]["parts"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let action = &part["PlayerAction"];
    action["player"] == controller
        && action["body"]["instructions"].as_array().is_some_and(|items| {
            matches!(items.as_slice(), [inner] if hand_discard(inner) && uses_of(uses, &inner["args"]["result"], 1))
        })
        && lossless::<CostPart>(part, tracing).is_some()
}

pub(super) fn may_need(value: &Value) -> bool {
    let action = &value["PlayerAction"];
    action["body"]["instructions"]
        .as_array()
        .is_some_and(|items| matches!(items.as_slice(), [inner] if hand_discard(inner)))
}

/// The one following test reads only whether the payment was made. No reader
/// observes the discard instruction's private ID or the paid object collection.
pub(super) fn program(items: &mut [Value], uses: &Uses, tracing: bool) -> bool {
    let mut changed = false;
    for at in 1..items.len() {
        if whole_hand_part(&items[at - 1], &items[at], uses, tracing) {
            items[at - 1]["args"]["operation"]["args"]["cost"]["parts"][0] = json!("DiscardHand");
            changed = true;
        }
    }
    changed
}
