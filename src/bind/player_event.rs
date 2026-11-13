//! A player event's selector is distinct from its available object referent.
use crate::model::ir::{EventSpec, PlayerEvent, PlayerSet};
use serde_json::Value;

/// Other event families keep their existing binding rules. Published Player
/// bindings depend only on the typed event discriminant, so unresolved selector
/// and player operands need not be cloned or guessed before ordinary binding.
/// An unknown or over-bound discriminant cannot prove an object reference.
pub(super) fn object_binding(event: &Value) -> Option<bool> {
    let player = event.get("Player")?;
    Some(proven_object(player).unwrap_or(false))
}

fn proven_object(player: &Value) -> Option<bool> {
    let kind = player.get("event")?;
    crate::functions::branch_bounds(kind).ok()?;
    let event = serde_json::from_value::<PlayerEvent>(kind.clone()).ok()?;
    let matcher = EventSpec::Player {
        event,
        players: PlayerSet::All,
        objects: None,
    };
    Some(crate::model::semantics::event_bindings(&matcher).object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn published_player_bindings_keep_actual_objects_and_exclude_player_only_events() {
        for (event, expected) in [
            (PlayerEvent::EntersUnderControl, true),
            (PlayerEvent::EntersUnplayed, true),
            (PlayerEvent::CreatesToken, true),
            (PlayerEvent::Sacrifices, true),
            (PlayerEvent::RollsDice, false),
            (PlayerEvent::Expends, false),
            (PlayerEvent::PlaneswalksAway, false),
            (PlayerEvent::GainsLife, false),
            (PlayerEvent::Draws, false),
        ] {
            let matcher = EventSpec::Player {
                event,
                players: PlayerSet::All,
                objects: None,
            };
            assert_eq!(
                crate::model::semantics::event_bindings(&matcher).object,
                expected,
                "{event:?}"
            );
            assert_eq!(
                object_binding(&serde_json::to_value(matcher).unwrap()),
                Some(expected)
            );
        }
    }

    #[test]
    fn selector_presence_never_invents_an_object_and_symbolic_players_do_not_erase_one() {
        for (event, expected) in [("PlaneswalksAway", false), ("CreatesToken", true)] {
            assert_eq!(
                object_binding(&json!({"Player":{
                    "event":event,
                    "players":{"$sym":"that_player","args":[]},
                    "objects":{"kind":"SameObject","args":{"$sym":"self","args":[]}}
                }})),
                Some(expected)
            );
        }
        assert_eq!(object_binding(&json!({"Turn":{}})), None);
    }

    #[test]
    fn missing_unknown_and_excessive_discriminants_supply_no_proof() {
        for player in [
            json!({}),
            json!({"event":"UnknownPlayerEvent"}),
            json!({"event":{"$sym":"event","args":[]}}),
            json!({"event":"x".repeat(1024 * 1024 + 1)}),
            json!({"event":vec![0; 16_384]}),
        ] {
            assert_eq!(object_binding(&json!({"Player":player})), Some(false));
        }
        let mut deep = Value::Null;
        for _ in 0..130 {
            deep = json!([deep]);
        }
        assert_eq!(
            object_binding(&json!({"Player":{"event":deep}})),
            Some(false)
        );
    }

    #[test]
    fn original_whole_card_player_event_source_focus_preflight() {
        let engine = crate::test_support::engine();
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/player-event-object-bindings-native.json"
        ))
        .unwrap();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let text = engine.presentation_free(case["text"].as_str().unwrap());
            let actual = engine
                .parse(name, &text)
                .unwrap_or_else(|why| panic!("{name}: {why:?}"));
            let mut expected: Vec<_> = case["expected_readings"]
                .as_array()
                .unwrap()
                .iter()
                .map(crate::bind::normal_form)
                .collect();
            expected.sort_by_key(Value::to_string);
            expected.dedup();
            assert_eq!(actual.readings.len(), expected.len(), "{name}");
            for reading in expected {
                assert!(actual.readings.contains(&reading), "{name}");
            }
        }
    }

    #[test]
    fn player_and_reflexive_holder_creator_library_preflight() {
        use crate::engine::canonical_abilities;
        let engine = crate::test_support::engine();
        let family: Value = serde_json::from_str(include_str!(
            "../../families/player-event-object-bindings.json"
        ))
        .unwrap();
        let off = engine.rule_set(|name| name.starts_with("m_") || name.starts_with("t_"));
        for case in family["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let actual = engine
                .parse_without(name, case["text"].as_str().unwrap(), &off)
                .unwrap();
            assert_eq!(
                actual.readings,
                vec![canonical_abilities(&case["expected"]).unwrap()],
                "{name}"
            );
        }
    }
}
