//! "You may exile another creature card from your graveyard. When you do, put a +1/+1
//! counter on each creature you control that shares a creature type with the exiled
//! card." The reflexive trigger's "the exiled card" is the one card its performed
//! action exiled, that move's receipt, not every card exiled with the source. Binding
//! names the receipt; the surface restores the linked spelling for the printer.
use crate::traverse::{Visit, rewrite};
use serde_json::{Value, json};

fn linked() -> Value {
    json!({"kind":"Query","args":{"objects":{"kind":"Zone","args":{"scope":{"Shared":"Exile"},
        "kind":"Card","filter":{"kind":"ExiledWith","args":{"kind":"Source"}}}},"index":0}})
}

fn receipt(moved: u64) -> Value {
    json!({"kind":"Result","args":{"result":moved,"phase":"After","index":0}})
}

fn effect(instruction: &Value, result: u64) -> Option<&Value> {
    (instruction["kind"] == "Effect" && instruction["args"]["result"].as_u64() == Some(result))
        .then(|| &instruction["args"]["operation"])
}

/// Whether the instructions before a reflexive trigger exile exactly one card as `moved`.
fn exiles_one(before: &[Value], moved: u64) -> bool {
    let Some(movement) = before.iter().find_map(|i| effect(i, moved)) else {
        return false;
    };
    let objects = &movement["args"]["objects"];
    if movement["kind"] != "Move" || movement["args"]["destination"].get("Exile").is_none() {
        return false;
    }
    if objects["kind"] == "One" {
        return true;
    }
    let Some(chosen) = (objects["kind"] == "Selected")
        .then(|| objects["args"].as_u64())
        .flatten()
    else {
        return false;
    };
    let one = json!({"kind":"Constant","args":1});
    before
        .iter()
        .find_map(|i| effect(i, chosen))
        .is_some_and(|choice| {
            let count = &choice["args"]["count"];
            choice["kind"] == "ChooseObjects"
                && (count == &json!({"AsManyAsCan": one}) || count == &json!({"Exactly": one}))
        })
}

/// Replace `old` with `new` in each reflexive trigger performed by a one-card exile.
fn rename(ability: &mut Value, old: &dyn Fn(u64) -> Value, new: &dyn Fn(u64) -> Value) -> bool {
    let mut changed = false;
    rewrite(ability, &mut |node| {
        let Some(instructions) = node.get_mut("instructions").and_then(Value::as_array_mut) else {
            return Visit::Children;
        };
        for at in 0..instructions.len() {
            let (before, rest) = instructions.split_at_mut(at);
            // Read before writing: indexing a missing key mutably would insert it.
            let operation = &rest[0]["args"]["operation"];
            let Some(moved) = (operation["kind"] == "ReflexiveTrigger")
                .then(|| operation["args"]["performed"].as_u64())
                .flatten()
            else {
                continue;
            };
            if exiles_one(before, moved)
                && let Some(body) = rest[0].pointer_mut("/args/operation/args/body")
            {
                let (old, new) = (old(moved), new(moved));
                rewrite(body, &mut |inner| {
                    if *inner == old {
                        *inner = new.clone();
                        changed = true;
                        Visit::Skip
                    } else {
                        Visit::Children
                    }
                });
            }
        }
        Visit::Children
    });
    changed
}

pub(super) fn lower(ability: &mut Value) -> bool {
    rename(ability, &|_| linked(), &receipt)
}

/// The linked spelling of each reflexive receipt. Whether any changed.
pub(super) fn surface(ability: &mut Value) -> bool {
    rename(ability, &receipt, &|_| linked())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reflexive(reference: Value) -> Value {
        let one = json!({"kind":"Constant","args":1});
        json!({"kind":"Triggered","args":{"body":{"instructions":[
            {"kind":"Effect","args":{"result":0,"operation":{"kind":"ChooseObjects","args":{
                "count":{"AsManyAsCan":one}}}}},
            {"kind":"Effect","args":{"result":1,"operation":{"kind":"Move","args":{
                "objects":{"kind":"Selected","args":0},"destination":{"Exile":{}}}}}},
            {"kind":"Effect","args":{"result":2,"operation":{"kind":"ReflexiveTrigger","args":{
                "performed":1,"body":{"instructions":[{"kind":"Effect","args":{"operation":{
                    "kind":"Destroy","args":{"objects":{"kind":"One","args":reference}}}}}]}}}}}
        ]}}})
    }

    #[test]
    fn the_exiled_card_after_a_one_card_exile_is_its_receipt_and_back() {
        let printed = reflexive(linked());
        let mut ability = printed.clone();
        assert!(lower(&mut ability));
        assert_eq!(ability, reflexive(receipt(1)));
        assert!(surface(&mut ability));
        assert_eq!(ability, printed);
    }

    #[test]
    fn a_many_card_exile_keeps_the_linked_cards() {
        let mut ability = reflexive(linked());
        ability["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["count"] =
            json!({"AsManyAsCan":{"kind":"Constant","args":2}});
        assert!(!lower(&mut ability));
    }
}
