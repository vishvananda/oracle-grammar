//! A return can use an exile result only while that incarnation remains exiled.
use crate::traverse::{self, Boundary};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn guard(root: &mut Value) {
    visit(root, &BTreeSet::new(), true);
}

fn visit(value: &mut Value, prior: &BTreeSet<u64>, root: bool) {
    // Nested abilities are bound independently. Their result addresses can
    // overlap their creator's; neither a number nor a target slot is a capture.
    if !root
        && matches!(
            traverse::boundary(value),
            Boundary::Ability | Boundary::Mode | Boundary::Holder | Boundary::Lexical
        )
    {
        return;
    }
    if let Some(instructions) = value.get_mut("instructions").and_then(Value::as_array_mut) {
        let mut available = prior.clone();
        for instruction in instructions {
            visit(instruction, &available, false);
            if instruction["kind"] == "Effect"
                && instruction["args"]["operation"]["kind"] == "Move"
                && super::movement::zone(&instruction["args"]["operation"]["args"]["destination"])
                    == Some("Exile")
                && let Some(result) = instruction["args"]["result"].as_u64()
            {
                available.insert(result);
            }
        }
        return;
    }
    if value["kind"] == "Move" && value["args"]["destination"].get("Battlefield").is_some() {
        let objects = &mut value["args"]["objects"];
        if super::symbol_name(objects) == Some("zone_members") {
            let mut operand = &mut objects["args"][0];
            if super::symbol_name(operand) == Some("exile_followup") {
                operand = &mut operand["args"][0];
            }
            if operand["kind"] == "One"
                && operand["args"]["kind"] == "Result"
                && operand["args"]["args"]["phase"] == "After"
                && operand["args"]["args"]["result"]
                    .as_u64()
                    .is_some_and(|result| prior.contains(&result))
            {
                *operand = json!({"kind":"InZone","args":{
                    "objects":operand.take(),"zone":"Exile"
                }});
            }
        }
    }
    // Only a delayed trigger retains this lexical program's captured results.
    // A fresh event program must establish its own preceding exile operation.
    let empty = BTreeSet::new();
    let context = if matches!(
        value["kind"].as_str(),
        Some("CreateTrigger" | "CreateReplacement")
    ) {
        &empty
    } else {
        prior
    };
    traverse::children(value, |child| visit(child, context, false));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(from: &str, to: &str, reference: Value) -> Value {
        json!({"instructions":[{"kind":"Effect","args":{"result":7,"operation":{"kind":"Move","args":{"objects":{"kind":"One","args":{"kind":"Source"}},"destination":super::super::movement::test_destination(json!(from)),"qualifiers":[]}}}},{"kind":"Effect","args":{"result":8,"operation":{"kind":"Move","args":{"objects":{"$sym":"zone_members","args":[{"kind":"One","args":reference}]},"destination":super::super::movement::test_destination(json!(to)),"qualifiers":[]}}}}]})
    }

    fn result() -> Value {
        json!({"kind":"Result","args":{"result":7,"phase":"After","index":0}})
    }

    fn guarded(value: &Value) -> bool {
        value["instructions"][1]["args"]["operation"]["args"]["objects"]["args"][0]["kind"]
            == "InZone"
    }

    #[test]
    fn only_actual_prior_exile_after_results_returning_to_battlefield_are_guarded() {
        let mut yes = program("Exile", "OwnersBattlefield", result());
        guard(&mut yes);
        assert!(guarded(&yes));
        for mut no in [
            program("OwnersGraveyard", "OwnersBattlefield", result()),
            program("Exile", "OwnersHand", result()),
            program(
                "Exile",
                "OwnersBattlefield",
                json!({"kind":"Target","args":{"slot":7,"index":0}}),
            ),
            program(
                "Exile",
                "OwnersBattlefield",
                json!({"kind":"Result","args":{"result":7,"phase":"Before","index":0}}),
            ),
            program(
                "Exile",
                "OwnersBattlefield",
                json!({"kind":"Result","args":{"result":6,"phase":"After","index":0}}),
            ),
        ] {
            guard(&mut no);
            assert!(!guarded(&no));
        }
        let mut forward = program("Exile", "OwnersBattlefield", result());
        forward["instructions"].as_array_mut().unwrap().swap(0, 1);
        let old = forward.clone();
        guard(&mut forward);
        assert_eq!(forward, old);
    }

    #[test]
    fn delayed_capture_is_lexical_but_branch_and_independent_ability_do_not_leak() {
        let p = program("Exile", "OwnersBattlefield", result());
        let mut delayed = p.clone();
        let instruction = delayed["instructions"][1].take();
        delayed["instructions"][1] = json!({"kind":"Effect","args":{"result":9,
            "operation":{"kind":"CreateDelayedTrigger","args":{"body":{"instructions":[instruction]}}}}});
        guard(&mut delayed);
        assert_eq!(
            delayed["instructions"][1]["args"]["operation"]["args"]["body"]["instructions"][0]["args"]
                ["operation"]["args"]["objects"]["args"][0]["kind"],
            "InZone"
        );
        let mut branch = p.clone();
        let definition = branch["instructions"][0].take();
        branch["instructions"][0] =
            json!({"kind":"Optional","args":{"body":{"instructions":[definition]}}});
        guard(&mut branch);
        assert!(!guarded(&branch));
        let mut independent = p;
        let instruction = independent["instructions"][1].take();
        independent["instructions"][1] = json!({"kind":"Static","args":{"changes":[
            {"kind":"GrantAbility","args":{"kind":"Activated","args":{"body":{"instructions":[instruction]}}}}
        ]}});
        let old = independent.clone();
        guard(&mut independent);
        assert_eq!(independent, old);
    }
}
