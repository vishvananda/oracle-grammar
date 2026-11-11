//! Preserve the provenance of a bare card predicate after an unscoped exile loop.
use super::{Scope, symbol_name};
use crate::model::ir::{
    EventZone, Expr, MovementDestination, ObjectFilter, ObjectKind, ObjectSet, Operation,
    PlayerRef, PlayerSet, SequenceEnd, ZoneScope,
};
use serde_json::{Value, json};

const MARKER: &str = "loop_card_it";

/// Called before transparent placement markers are removed. Only an atomic
/// prefix condition has this evidence; a suffix can name its own antecedent.
pub(super) fn mark(value: &mut Value) {
    let Some(items) = value.get_mut("instructions").and_then(Value::as_array_mut) else {
        return;
    };
    for index in 1..items.len() {
        if !single_exile_loop(&items[index - 1]) {
            continue;
        }
        let next = &mut items[index];
        if symbol_name(next) != Some("leading_if") {
            continue;
        }
        let condition = &mut next["args"][0]["args"]["condition"];
        if condition["kind"] == "ObjectMatches"
            && symbol_name(&condition["args"]["object"]) == Some("current_it")
        {
            condition["args"]["object"]["$sym"] = json!(MARKER);
        }
    }
}

fn single_exile_loop(value: &Value) -> bool {
    if value["kind"] != "ForEachPlayer" {
        return false;
    }
    let Some(binding) = value["args"]["binding"].as_u64() else {
        return false;
    };
    let Some(items) = value["args"]["body"]["instructions"].as_array() else {
        return false;
    };
    let [effect] = items.as_slice() else {
        return false;
    };
    if effect["kind"] != "Effect" {
        return false;
    }
    // The header can still contain a deferred target. Only the closed operation
    // in the loop body is typed here; no target or result is materialized.
    let Ok(Operation::Move {
        objects,
        destination: MovementDestination::Exile { .. },
        ..
    }) = serde_json::from_value::<Operation>(effect["args"]["operation"].clone())
    else {
        return false;
    };
    let ObjectSet::Take {
        objects,
        count: Expr::Constant(1),
        from: SequenceEnd::Top,
    } = objects
    else {
        return false;
    };
    matches!(*objects, ObjectSet::Zone {
        scope: ZoneScope::Players {
            zone: EventZone::Library,
            players: PlayerSet::One(PlayerRef::Bound(id)),
        },
        kind: ObjectKind::Card,
        filter: ObjectFilter::Any,
    } if u64::from(id.0) == binding)
}

/// Guarded resolution is complete; every common consumer sees ordinary `it`.
pub(super) fn ordinary_name(name: String) -> String {
    if name == MARKER {
        "current_it".into()
    } else {
        name
    }
}

/// An explicitly remembered Source is distinct from the initial default Source.
/// Established events, targets and receipts retain the ordinary pronoun rules.
pub(super) fn reference(scope: &Scope) -> Result<Value, String> {
    if scope.pronoun.is_none() && scope.objects == [json!({"kind":"Source"})] {
        return Err("an exile loop does not establish one card for this predicate".into());
    }
    scope.object_pronoun(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sequence(marker: &str, object: Value) -> Value {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/dimensional-loop-focus.json"
        ))
        .unwrap();
        // Keep the real previously accepted loop, including its deferred-header
        // compatible shape and unused result; only the following syntax is raw.
        let loop_instruction = fixture["retired"][3]["args"]["body"]["instructions"][0].clone();
        json!({"instructions":[loop_instruction, {
            "$sym":marker,"args":[{"kind":"If","args":{
                "condition":{"kind":"ObjectMatches","args":{
                    "object":object,"filter":{"kind":"CardType","args":"Land"}}},
                "then":{"instructions":[]},"otherwise":{"instructions":[]}
            }}],"at":17
        }]})
    }

    fn object(value: &Value) -> &Value {
        &value["instructions"][1]["args"][0]["args"]["condition"]["args"]["object"]
    }

    #[test]
    fn only_atomic_prefix_bare_it_after_exact_loop_is_marked() {
        let mut value = sequence("leading_if", json!({"$sym":"current_it","args":[],"at":19}));
        mark(&mut value);
        assert_eq!(symbol_name(object(&value)), Some(MARKER));
        assert_eq!(object(&value)["at"], 19);
        for (placement, name) in [
            ("resolution", "current_it"),
            ("leading_if", "self"),
            ("leading_if", "named_source"),
        ] {
            let mut value = sequence(placement, json!({"$sym":name,"args":[],"at":19}));
            let original = value.clone();
            mark(&mut value);
            assert_eq!(value, original);
        }
    }

    #[test]
    fn unrelated_or_multiple_loop_outputs_do_not_acquire_metadata() {
        let mut value = sequence("leading_if", json!({"$sym":"current_it","args":[],"at":19}));
        value["instructions"][0]["args"]["binding"] = json!(1);
        let original = value.clone();
        mark(&mut value);
        assert_eq!(value, original);
        let mut value = sequence("leading_if", json!({"$sym":"current_it","args":[],"at":19}));
        let extra = value["instructions"][0]["args"]["body"]["instructions"][0].clone();
        value["instructions"][0]["args"]["body"]["instructions"]
            .as_array_mut()
            .unwrap()
            .push(extra);
        let original = value.clone();
        mark(&mut value);
        assert_eq!(value, original);
    }

    #[test]
    fn header_targets_stay_symbolic_and_placement_stripping_keeps_provenance() {
        let mut value = sequence("leading_if", json!({"$sym":"current_it","args":[],"at":19}));
        let target = json!({"$sym":"target","args":[],"at":3});
        value["instructions"][0]["args"]["players"] = target.clone();
        super::super::guards::condition_placement(&mut value).unwrap();
        assert_eq!(value["instructions"][0]["args"]["players"], target);
        assert_eq!(value["instructions"][1]["kind"], "If");
        assert_eq!(
            symbol_name(&value["instructions"][1]["args"]["condition"]["args"]["object"]),
            Some(MARKER)
        );
        assert_eq!(
            value["instructions"][0]["args"]["body"]["instructions"][0]["args"]["result"],
            u32::MAX
        );
    }

    #[test]
    fn established_scalar_focus_is_preserved_but_default_source_refuses() {
        let mut scope = Scope {
            objects: vec![json!({"kind":"Source"})],
            ..Scope::default()
        };
        assert!(reference(&scope).is_err());
        for focus in [
            json!({"kind":"Source"}),
            json!({"kind":"EventSubject"}),
            json!({"kind":"Target","args":{"slot":0,"index":0}}),
            json!({"kind":"Result","args":{"result":0,"phase":"After","index":0}}),
        ] {
            scope.objects.push(focus.clone());
            assert_eq!(reference(&scope).unwrap(), focus);
            scope.objects.pop();
        }
        scope.pronoun = Some(json!({"kind":"EventSubject"}));
        assert_eq!(reference(&scope).unwrap(), scope.pronoun.unwrap());
    }
    #[test]
    fn marked_established_focus_tracks_the_same_public_move_as_current_it() {
        use super::super::{Step, events::Frame, transitions};
        let scope = Scope {
            objects: vec![json!({"kind":"Source"}), json!({"kind":"Source"})],
            ..Scope::default()
        };
        let reference = reference(&scope).unwrap();
        let root = json!({"instructions":[{"kind":"Effect","args":{"result":7,"operation":{"kind":"Move","args":{"objects":{"kind":"One","args":{"kind":"Source"}},"destination":{"Exile":{"visibility":"FaceUp","arrival":{"counters":[],"states":[]}}},"qualifiers":[]}}}},{"kind":"If","args":{"condition":{"kind":"ObjectMatches","args":{"object":{"$sym":"loop_card_it","args":[],"at":19},"filter":{"kind":"CardType","args":"Land"}}}}}]});
        let path = vec![
            Step::Key("instructions".into()),
            Step::Index(1),
            Step::Key("args".into()),
            Step::Key("condition".into()),
            Step::Key("args".into()),
            Step::Key("object".into()),
        ];
        let introductions = transitions::Introductions {
            slots: &[],
            keys: &[],
            base: 0,
        };
        let frame = Frame::default();
        let ordinary = transitions::reference(
            "current_it",
            &root,
            &path,
            &introductions,
            &frame,
            reference.clone(),
            None,
        )
        .unwrap();
        let marked = transitions::reference(
            &ordinary_name(MARKER.into()),
            &root,
            &path,
            &introductions,
            &frame,
            reference,
            None,
        )
        .unwrap();
        assert_eq!(marked, ordinary);
        assert_eq!(
            marked,
            json!({"kind":"Result","args":{"result":7,"phase":"After","index":0}})
        );
    }
    #[test]
    fn owning_frame_normalizes_marked_focus_before_common_movement_dispatch() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/dimensional-loop-focus.json"
        ))
        .unwrap();
        let mut ability = fixture["retired"][3].clone();
        let mut program = sequence("leading_if", json!({"$sym":"current_it","args":[],"at":19}));
        // An earlier explicit Source operand establishes a real scalar focus.
        // Copy the complete native Move fields, changing only its source set.
        let mut moved = program["instructions"][0]["args"]["body"]["instructions"][0].clone();
        moved["args"]["result"] = json!(7);
        moved["args"]["operation"]["args"]["objects"] = json!({
            "kind":"One","args":{"$sym":"remember_object","args":[{
                "kind":"Source"
            }],"at":3}
        });
        program["instructions"]
            .as_array_mut()
            .unwrap()
            .insert(0, moved);
        ability["args"]["body"] = program;
        super::super::bind_ability(&mut ability, "Dimensional Infiltrator").unwrap();
        assert_eq!(
            ability["args"]["body"]["instructions"][2]["args"]["condition"]["args"]["object"],
            json!({"kind":"Result","args":{"result":7,"phase":"After","index":0}})
        );
    }
}
