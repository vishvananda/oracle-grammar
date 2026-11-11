use super::*;
use crate::engine::{Engine, canonical_abilities};
fn cue() -> Value {
    json!({"$sym":"indefinite_opponent_controller","args":[{"kind":"ControlledBy","args":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}}}],"at":3})
}
fn filter() -> Value {
    json!({"kind":"AllOf","args":[{"kind":"CardType","args":"Artifact"},cue()]})
}
fn annotated() -> Value {
    annotate(&native(&filter()), &filter(), 2).unwrap()
}
fn ability(event: Value) -> Value {
    json!({"kind":"Triggered","args":{"event":event,"limits":{"per_turn":null,"total":null},"announcement":{"targets":[],"modes":null},"body":{"instructions":[]}}})
}
fn engine() -> &'static Engine {
    crate::test_support::engine()
}
#[test]
fn actual_public_original_edits_explicit_owner_and_quoted_roles_before_cli() {
    let f: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/departure-controller-cases.json"
    ))
    .unwrap();
    for case in f["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let wanted = canonical_abilities(&case["expected"]).unwrap();
        let parsed = engine().parse(name, text).unwrap();
        assert_eq!(
            parsed.readings.len(),
            case["readings"].as_u64().unwrap() as usize,
            "{text}"
        );
        assert!(parsed.readings.contains(&wanted), "{text}");
        for frame in parsed.readings {
            let trail = engine().trail_of(name, &frame, text).unwrap().unwrap();
            assert_eq!(
                engine()
                    .print_trail(name, &frame, &trail)
                    .unwrap()
                    .as_deref(),
                Some(text)
            );
            let printed = engine()
                .print_keyed(name, &frame, &[], &[])
                .unwrap()
                .unwrap();
            assert!(
                engine()
                    .parse(name, &printed)
                    .unwrap()
                    .readings
                    .contains(&frame)
            );
        }
    }
}
#[test]
fn edited_damage_old_trail_reads_current_operands() {
    let f: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/departure-controller-cases.json"
    ))
    .unwrap();
    let old = &f["cases"][1];
    let edited = &f["cases"][2];
    let old_ir = canonical_abilities(&old["expected"]).unwrap();
    let new_ir = canonical_abilities(&edited["expected"]).unwrap();
    let text = old["text"].as_str().unwrap();
    let name = old["name"].as_str().unwrap();
    assert_ne!(old_ir, new_ir);
    let trail = engine().trail_of(name, &old_ir, text).unwrap().unwrap();
    assert!(
        !engine()
            .parse(name, text)
            .unwrap()
            .readings
            .contains(&new_ir)
    );
    if let Some(printed) = engine().print_trail(name, &new_ir, &trail).unwrap() {
        assert_ne!(printed, text);
        assert!(
            engine()
                .parse(name, &printed)
                .unwrap()
                .readings
                .contains(&new_ir)
        );
    }
}
#[test]
fn exact_native_event_is_kept_and_original_evidence_is_not_duplicated() {
    let raw = filter();
    let event = native(&raw);
    let annotation = annotate(&event, &raw, 5).unwrap();
    assert_eq!(annotation["args"].as_array().unwrap().len(), 1);
    assert_eq!(annotation["args"][0], event);
    let mut a = ability(annotation);
    assert!(prepare(&mut a).unwrap());
    assert_eq!(a["args"]["event"], event);
    assert!(!a.to_string().contains("departure_controller_event"));
    let mut wrong = event;
    wrong["ZoneChange"]["to"] = json!("Hand");
    assert!(annotate(&wrong, &raw, 0).is_err());
}
#[test]
fn evidence_does_not_duplicate_a_symbolic_target_introduction() {
    let raw = json!({"kind":"AllOf","args":[cue(),{"kind":"SameObject","args":{"$sym":"target","args":[{}],"at":9}}]});
    let annotation = annotate(&native(&raw), &raw, 0).unwrap();
    assert_eq!(
        annotation
            .to_string()
            .matches("\"$sym\":\"target\"")
            .count(),
        1
    );
}
#[test]
fn real_owners_negative_alternative_and_nested_predicates_never_introduce_controller() {
    for raw in [
        json!({"kind":"AllOf","args":[cue(),synthetic_owner()]}),
        json!({"kind":"AllOf","args":[cue(),{"kind":"Not","args":{"kind":"OwnedBy","args":{"kind":"One","args":{"kind":"AbilityController"}}}}]}),
        json!({"kind":"Not","args":cue()}),
        json!({"kind":"AnyOf","args":[cue(),{"kind":"Any"}]}),
        json!({"kind":"SameObject","args":cue()}),
        json!({"kind":"ControlledBy","args":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}}}),
    ] {
        assert!(!proven(&raw).unwrap());
        assert_eq!(annotate(&native(&raw), &raw, 0).unwrap(), native(&raw));
    }
}
#[test]
fn every_either_arm_must_prove_and_all_annotations_are_stripped() {
    for (other, expected) in [(annotated(), true), (native(&filter()), false)] {
        let mut a = ability(json!({"AnyOf":[annotated(),other]}));
        assert_eq!(prepare(&mut a).unwrap(), expected);
        assert!(!a.to_string().contains("departure_controller_event"));
    }
    for wrapper in ["FirstEachTurn", "FirstForObjectEachTurn", "FirstEachGame"] {
        let mut event = json!({});
        event[wrapper] = annotated();
        assert!(prepare(&mut ability(event)).unwrap());
    }
    let mut batch = ability(json!({"Batch":{"event":annotated(),"per":"All"}}));
    assert!(!prepare(&mut batch).unwrap());
    assert!(!batch.to_string().contains("departure_controller_event"));
}
#[test]
fn initial_controller_seed_does_not_override_later_player_or_another_holder() {
    let mut a = ability(annotated());
    let fact = prepare(&mut a).unwrap();
    let mut scope = super::super::events::scope_for(&a, "Sardian Avenger", 0);
    seed(&mut scope, fact);
    assert_eq!(
        scope.players.last().unwrap(),
        &json!({"kind":"ControllerOf","args":{"kind":"EventSubject"}})
    );
    let target = json!({"kind":"Target","args":{"slot":2,"index":0}});
    scope.players.push(target.clone());
    seed(&mut scope, fact);
    assert_eq!(scope.players.last(), Some(&target));
    let other = ability(native(&filter()));
    let mut independent = super::super::events::scope_for(&other, "Sardian Avenger", 1);
    seed(&mut independent, false);
    assert_eq!(
        independent.players.last().unwrap(),
        &json!({"kind":"OwnerOf","args":{"kind":"EventSubject"}})
    );
}
#[test]
fn malformed_arity_and_bounded_work_fail_closed() {
    let mut bad = cue();
    bad["args"].as_array_mut().unwrap().push(json!({}));
    assert!(annotate(&native(&bad), &bad, 0).is_err());
    let mut event = annotated();
    event["args"].as_array_mut().unwrap().push(json!({}));
    assert!(prepare(&mut ability(event)).is_err());
    let mut deep = filter();
    for _ in 0..130 {
        deep = json!({"kind":"AllOf","args":[deep]});
    }
    assert!(proven(&deep).is_err());
    assert!(annotate(&json!("x".repeat(1_048_577)), &filter(), 0).is_err());
}
