//! Spell nouns retain their typed antecedent across unrelated object actions.
use crate::model::ir::{
    DamageEvent, DamageGrouping, DamageKind, DamageRecipients, DamageSubject, EventSpec,
    ObjectFilter, ObjectKind, Operand, PlayerSet, SpellEvent, TargetingKind,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Clone, Copy, Default)]
pub(super) struct Facts {
    pub(super) subject: bool,
    pub(super) stack: bool,
    pub(super) damage_source: bool,
}

fn discriminant<T: DeserializeOwned>(value: &Value) -> Option<T> {
    let text = value.as_str()?;
    if text.len() > 64 {
        return None;
    }
    serde_json::from_value(value.clone()).ok()
}

/// Native binding capabilities depend on these discriminators. Pending player
/// and filter operands stay untouched; a whole unbound matcher need not decode.
pub(super) fn facts(event: &Value) -> Facts {
    fn visit(event: &Value, depth: usize, work: &mut usize) -> Option<Facts> {
        if depth > 128 || *work == 0 {
            return None;
        }
        *work -= 1;
        if let Some(inner) = super::events::inner(event) {
            return visit(inner, depth + 1, work);
        }
        if let Some(parts) = event.get("AnyOf").and_then(Value::as_array) {
            let (first, rest) = parts.split_first()?;
            if parts.len() > 256 {
                return None;
            }
            let mut result = visit(first, depth + 1, work)?;
            for part in rest {
                let next = visit(part, depth + 1, work)?;
                result.subject &= next.subject;
                result.stack &= next.stack;
                result.damage_source &= next.damage_source;
            }
            return Some(result);
        }
        if let Some(spell) = event.get("Spell") {
            let typed = EventSpec::Spell {
                event: discriminant::<SpellEvent>(&spell["event"])?,
                players: PlayerSet::All,
                spells: ObjectFilter::Any,
            };
            return Some(Facts {
                subject: crate::model::semantics::event_bindings(&typed).spell,
                stack: false,
                damage_source: false,
            });
        }
        if let Some(targeting) = event.get("BecomesTarget") {
            let kind: TargetingKind = discriminant(&targeting["kind"])?;
            let is_spell = kind == TargetingKind::Spell;
            let typed = EventSpec::BecomesTarget {
                objects: ObjectFilter::Any,
                kind,
                by: Box::new(Operand::Filter(ObjectFilter::Any)),
            };
            return Some(Facts {
                subject: false,
                stack: is_spell && crate::model::semantics::event_bindings(&typed).stack_object,
                damage_source: false,
            });
        }
        if let Some(damage) = event.get("Damage") {
            let source_kind = discriminant::<ObjectKind>(&damage["source_kind"])?;
            let is_spell = source_kind == ObjectKind::Spell;
            let typed = EventSpec::Damage(DamageEvent {
                source_kind,
                sources: ObjectFilter::Any,
                recipients: DamageRecipients::Any,
                kind: DamageKind::Any,
                grouping: discriminant::<DamageGrouping>(&damage["grouping"])?,
                subject: discriminant::<DamageSubject>(&damage["subject"])?,
            });
            let native = crate::model::semantics::event_bindings(&typed);
            return Some(Facts {
                subject: native.spell,
                stack: false,
                damage_source: is_spell && native.damage_source,
            });
        }
        Some(Facts::default())
    }
    visit(event, 0, &mut 4096).unwrap_or_default()
}

/// A cast spell's temporary grant follows it onto the battlefield only as a
/// spell change; a continuous effect on the spell itself ends with the spell.
/// A `Continuing` spell change in turn names that cast spell or a target that
/// can be on the stack.
pub(super) fn check_spell_grants(ability: &Value) -> Result<(), String> {
    struct Walk {
        work: usize,
        stack_slots: Vec<u64>,
    }
    fn fields<'a>(
        node: &'a Value,
        depth: usize,
        walk: &mut Walk,
    ) -> Result<Vec<&'a Value>, String> {
        if depth > 256 || walk.work == 0 {
            return Err("ability exceeds the spell-grant check bounds".into());
        }
        walk.work -= 1;
        Ok(match node {
            Value::Array(items) => items.iter().collect(),
            Value::Object(fields) => fields.values().collect(),
            _ => Vec::new(),
        })
    }
    fn slots(node: &Value, depth: usize, walk: &mut Walk) -> Result<(), String> {
        let zones = &node["kind"]["Object"]["zones"];
        if let (Some(slot), Some(zones)) = (node["slot"].as_u64(), zones.as_array())
            && zones.iter().any(|zone| zone == "Stack")
        {
            walk.stack_slots.push(slot);
        }
        for child in fields(node, depth, walk)? {
            slots(child, depth + 1, walk)?;
        }
        Ok(())
    }
    fn visit(node: &Value, spell: bool, depth: usize, walk: &mut Walk) -> Result<(), String> {
        let subject = serde_json::json!({"kind": "EventSubject"});
        let spell = match (node.get("event"), node.get("body")) {
            (Some(event), Some(_)) => facts(event).subject,
            _ => spell,
        };
        let args = &node["args"];
        match node["kind"].as_str() {
            Some("CreateContinuousEffect")
                if spell
                    && args["affected"]["Objects"]
                        == serde_json::json!({"kind": "One", "args": subject}) =>
            {
                return Err("a cast spell's own changes are spell changes".into());
            }
            Some("ChangeSpell") if continuing(&args["changes"]) => {
                let named = &args["subject"]["One"];
                let target = named["kind"] == "Target"
                    && named["args"]["slot"]
                        .as_u64()
                        .is_some_and(|slot| walk.stack_slots.contains(&slot));
                if !named.is_null() && !target && !(spell && *named == subject) {
                    return Err("a continuing spell change names a spell".into());
                }
            }
            _ => {}
        }
        for child in fields(node, depth, walk)? {
            visit(child, spell, depth + 1, walk)?;
        }
        Ok(())
    }
    fn continuing(changes: &Value) -> bool {
        changes
            .as_array()
            .is_some_and(|changes| changes.iter().any(|c| c["kind"] == "Continuing"))
    }
    let mut walk = Walk {
        work: 1 << 20,
        stack_slots: Vec::new(),
    };
    slots(ability, 0, &mut walk)?;
    visit(ability, false, 0, &mut walk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cast() -> Value {
        json!({"Spell":{"event":"Cast","players":{"$sym":"pending_player"},
            "spells":{"$sym":"pending_filter"}}})
    }

    #[test]
    fn cast_copy_and_pending_operands_share_the_spell_subject() {
        let mut copy = cast();
        copy["Spell"]["event"] = json!("Copy");
        let found = facts(&json!({"AnyOf":[cast(),copy]}));
        assert!(found.subject);
        assert!(!found.stack);
    }

    #[test]
    fn targeting_spell_is_distinct_from_target_and_from_abilities() {
        for (kind, expected) in [
            ("Spell", true),
            ("Ability", false),
            ("SpellOrAbility", false),
        ] {
            let found = facts(&json!({"BecomesTarget":{"kind":kind,"by":{"$sym":"pending"}}}));
            assert!(!found.subject);
            assert_eq!(found.stack, expected, "{kind}");
        }
        let mixed = json!({"AnyOf":[cast(),{"BecomesTarget":{"kind":"Spell"}}]});
        assert!(!facts(&mixed).subject);
        assert!(!facts(&mixed).stack);
    }

    #[test]
    fn proof_is_bounded_and_wrappers_preserve_guaranteed_facts() {
        assert!(
            facts(&json!({"Filtered":{"event":cast(),"condition":{"$sym":"pending"}}})).subject
        );
        assert!(!facts(&json!({"AnyOf":[]})).subject);
        let mut event = cast();
        for _ in 0..130 {
            event = json!({"FirstEachTurn":event});
        }
        assert!(!facts(&event).subject);
        assert!(!facts(&json!({"Spell":{"event":"X".repeat(65)}})).subject);
        assert!(!facts(&json!({"AnyOf":vec![cast();257]})).subject);
    }

    #[test]
    fn native_spell_references_and_unrelated_object_controls_preflight() {
        use crate::engine::semantic_abilities;
        let engine = crate::test_support::engine();
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../../tests/fixtures/spell-focus-native.json"))
                .unwrap();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let mut expected: Vec<_> = case["expected_readings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| semantic_abilities(v).unwrap())
                .collect();
            let mut actual: Vec<_> = engine
                .parse(name, case["text"].as_str().unwrap())
                .unwrap()
                .readings
                .iter()
                .map(|v| semantic_abilities(v).unwrap())
                .collect();
            expected.sort_by_key(Value::to_string);
            expected.dedup();
            actual.sort_by_key(Value::to_string);
            for prior in &expected {
                assert!(
                    actual.contains(prior),
                    "{name}: native/prior reading missing"
                );
            }
            let extra: Vec<_> = case["additional_source_correct_readings"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| semantic_abilities(v).unwrap())
                .collect();
            for reading in &actual {
                assert!(
                    expected.contains(reading) || extra.contains(reading),
                    "{name}: unreviewed added reading {reading}"
                );
            }
        }
    }

    #[test]
    fn spell_possessive_inverse_preflight() {
        use crate::engine::canonical_abilities;
        let engine = crate::test_support::engine();
        let family: Value =
            serde_json::from_str(include_str!("../../families/spell-focus.json")).unwrap();
        for case in family["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let text = case["text"].as_str().unwrap();
            let expected = canonical_abilities(&case["expected"]).unwrap();
            let trail = engine
                .trail_of(name, &expected, text)
                .unwrap()
                .unwrap_or_else(|| panic!("{name}: no exact trail"));
            let printed = engine
                .print_trail(name, &expected, &trail)
                .unwrap()
                .unwrap();
            assert_eq!(printed, text, "{name}");
        }
    }

    #[test]
    fn native_permission_sources_and_created_spell_events_keep_complete_readings() {
        use crate::engine::semantic_abilities;
        let engine = crate::test_support::engine();
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/spell-focus-compatibility-native.json"
        ))
        .unwrap();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let mut expected: Vec<_> = case["expected_readings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| semantic_abilities(v).unwrap())
                .collect();
            let mut actual: Vec<_> = engine
                .parse(name, case["text"].as_str().unwrap())
                .unwrap()
                .readings
                .iter()
                .map(|v| semantic_abilities(v).unwrap())
                .collect();
            expected.sort_by_key(Value::to_string);
            expected.dedup();
            actual.sort_by_key(Value::to_string);
            assert_eq!(
                actual, expected,
                "{name}: complete native/prior reading set"
            );
        }
    }
}
