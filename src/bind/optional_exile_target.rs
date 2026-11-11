//! A target set proves one input only through its own exact announcement bound.
//! This proof is binding metadata: the actual producer remains ObjectSet::Targets.
use super::{Step, symbol_name, transitions::Introductions};
use crate::model::ir::{Count, Expr, TargetKind};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn single_object(spec: &Value) -> Result<bool, String> {
    crate::functions::branch_bounds(spec)?;
    // Filters/players may still contain binding symbols here. Decode only the
    // independent typed kind and literal bound; an unknown bound proves nothing.
    let Ok(kind) = serde_json::from_value::<TargetKind>(spec["kind"].clone()) else {
        return Ok(false);
    };
    let Ok(count) = serde_json::from_value::<Count>(spec["count"].clone()) else {
        return Ok(false);
    };
    Ok(matches!(kind, TargetKind::Object(_))
        && matches!(
            count,
            Count::Exactly(Expr::Constant(1)) | Count::UpTo(Expr::Constant(1))
        ))
}

fn announcement(value: &Value) -> &Value {
    if value.get("announcement").is_some() {
        &value["announcement"]
    } else {
        &value["args"]["announcement"]
    }
}

/// Slots owned by this exact ability/mode, never a containing holder's slots.
pub(super) fn slots(value: &Value) -> Result<BTreeSet<u16>, String> {
    let mut proved = BTreeSet::new();
    let Some(targets) = announcement(value)["targets"].as_array() else {
        return Ok(proved);
    };
    if targets.len() > 4096 {
        return Err("single exile target specification limit".into());
    }
    let mut seen = BTreeSet::new();
    for spec in targets {
        let Some(slot) = spec["slot"].as_u64().and_then(|n| u16::try_from(n).ok()) else {
            continue;
        };
        if !seen.insert(slot) {
            return Err("single exile target duplicate slot".into());
        }
        if single_object(spec)? {
            proved.insert(slot);
        }
    }
    Ok(proved)
}

fn node<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get(key)?,
            Step::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}

/// The new projection is restricted to lexically eligible implicit action
/// operands. Equal native target addresses do not prove an implicit wording.
pub(super) fn implicit_action(root: &Value, path: &[Step]) -> Result<bool, String> {
    if path.len() > 128 {
        return Err("single exile implicit action path limit".into());
    }
    for depth in (0..path.len()).rev() {
        let Some(marker) = node(root, &path[..depth]) else {
            continue;
        };
        if symbol_name(marker) != Some("exile_followup") {
            continue;
        }
        let Some(objects) = marker["args"].get(0) else {
            return Ok(false);
        };
        crate::functions::branch_bounds(objects)?;
        if symbol_name(&super::exile_followups::lexical(objects, 0)) != Some("exile_followup") {
            return Ok(false);
        }
        for parent in (0..depth).rev() {
            let Some(operation) = node(root, &path[..parent]) else {
                continue;
            };
            if matches!(
                operation["kind"].as_str(),
                Some("Move" | "CreatePerpetualEffect")
            ) {
                return Ok(matches!(path.get(parent),Some(Step::Key(k)) if k=="args")
                    && matches!(path.get(parent+1),Some(Step::Key(k)) if k=="objects"));
            }
        }
        return Ok(false);
    }
    Ok(false)
}

/// Compare a pending Targets producer with its exact announced scalar identity.
/// Nothing returned here replaces or narrows the producer ObjectSet.
pub(super) fn reference(
    root: &Value,
    objects: &Value,
    introductions: &Introductions<'_>,
) -> Result<Option<Value>, String> {
    if objects["kind"] != "Targets" {
        return Ok(None);
    }
    crate::functions::branch_bounds(objects)?;
    let slot = &objects["args"];
    if symbol_name(slot) == Some("target") {
        let Some(spec) = slot["args"].get(0) else {
            return Ok(None);
        };
        if !single_object(spec)? {
            return Ok(None);
        }
        let key = (
            slot["at"].as_u64().ok_or("single exile target position")?,
            spec.to_string(),
        );
        if !introductions.keys.iter().any(|(owned, _)| *owned == key) {
            return Ok(None);
        }
        let reference = json!({"kind":"Target","args":{"slot":slot,"index":0}});
        return super::references::bound_reference(
            &reference,
            introductions.keys,
            introductions.base,
        )
        .map(Some);
    }
    let Some(slot) = slot.as_u64().and_then(|n| u16::try_from(n).ok()) else {
        return Ok(None);
    };
    if slots(root)?.contains(&slot) {
        Ok(Some(
            json!({"kind":"Target","args":{"slot":slot,"index":0}}),
        ))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(slot: u16, count: Value) -> Value {
        json!({"slot":slot,"kind":{"Object":{"kind":"Card","zones":["Graveyard"]}},
            "count":count,"filter":"Any","players":null,"distribute":null,"constraints":[],"chooser":null})
    }
    fn movement(result: u32, objects: Value, destination: Value) -> Value {
        json!({"kind":"Effect","args":{"result":result,"operation":{"kind":"Move","args":{
            "objects":objects,"destination":super::super::movement::test_destination(destination),"qualifiers":[]}}}})
    }
    fn marker(result: u32) -> Value {
        json!({"$sym":"exile_followup","args":[{"kind":"One","args":{"kind":"Result","args":{"result":result,"phase":"After","index":0}}}],"at":30})
    }
    fn spell(targets: Vec<Value>, instructions: Vec<Value>) -> Value {
        json!({"kind":"Spell","args":{"announcement":{"targets":targets,"modes":null},"body":{"instructions":instructions}}})
    }
    fn group(result: u32) -> Value {
        json!({"kind":"InZone","args":{"objects":{"kind":"Transition","args":{"result":result,"phase":"After"}},"zone":"Exile"}})
    }
    #[test]
    fn only_literal_zero_or_one_object_bounds_supply_scoped_proof() {
        for count in [
            json!({"Exactly":{"kind":"Constant","args":1}}),
            json!({"UpTo":{"kind":"Constant","args":1}}),
        ] {
            let mut value = spec(2, count);
            // A pending filter must not make a valid independent count proof fail.
            value["filter"] = json!({"$sym":"self","args":[],"at":1});
            assert!(single_object(&value).unwrap());
            assert_eq!(
                slots(&spell(vec![value], vec![])).unwrap(),
                BTreeSet::from([2])
            );
        }
        for count in [
            json!({"UpTo":{"kind":"Constant","args":2}}),
            json!({"Exactly":{"kind":"Constant","args":2}}),
            json!({"AsManyAsCan":{"kind":"Constant","args":1}}),
            json!({"UpTo":{"kind":"Variable","args":"X"}}),
        ] {
            assert!(!single_object(&spec(0, count)).unwrap());
        }
        let mut player = spec(0, json!({"UpTo":{"kind":"Constant","args":1}}));
        player["kind"] = json!("Player");
        assert!(!single_object(&player).unwrap());
    }
    #[test]
    fn pending_target_identity_uses_the_owning_introduction_and_prefix_namespace() {
        let declared = spec(0, json!({"UpTo":{"kind":"Constant","args":1}}));
        let symbol = json!({"$sym":"target","args":[declared],"at":17});
        let objects = json!({"kind":"Targets","args":symbol});
        let keys = vec![((17, declared.to_string()), 0)];
        let own = Introductions {
            slots: &[],
            keys: &keys,
            base: 2,
        };
        assert_eq!(
            reference(&Value::Null, &objects, &own).unwrap(),
            Some(json!({"kind":"Target","args":{"slot":2,"index":0}}))
        );
        let foreign = Introductions {
            slots: &[],
            keys: &[],
            base: 2,
        };
        assert!(
            reference(&Value::Null, &objects, &foreign)
                .unwrap()
                .is_none()
        );
        assert_eq!(objects["kind"], "Targets");
        let mut deep = Value::Null;
        for _ in 0..130 {
            deep = json!([deep]);
        }
        let over = json!({"kind":"Targets","args":deep});
        assert!(reference(&Value::Null, &over, &own).is_err());
    }
    #[test]
    fn exact_actual_targets_receipt_is_preserved_without_coercing_the_producer() {
        for count in [
            json!({"UpTo":{"kind":"Constant","args":1}}),
            json!({"Exactly":{"kind":"Constant","args":1}}),
        ] {
            let producer = movement(7, json!({"kind":"Targets","args":2}), json!("Exile"));
            let mut value = spell(
                vec![
                    spec(0, json!({"Exactly":{"kind":"Constant","args":1}})),
                    spec(1, json!({"Exactly":{"kind":"Constant","args":1}})),
                    spec(2, count),
                ],
                vec![
                    producer.clone(),
                    movement(8, marker(7), json!("OwnersBattlefield")),
                ],
            );
            super::super::exile_followups::lower(&mut value).unwrap();
            assert_eq!(value["args"]["body"]["instructions"][0], producer);
            assert_eq!(
                value["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["objects"],
                group(7)
            );
        }
        // A plural or unknown slot cannot borrow a neighbouring singular spec.
        for slot in [0, 3] {
            let mut value = spell(
                vec![
                    spec(0, json!({"UpTo":{"kind":"Constant","args":2}})),
                    spec(2, json!({"UpTo":{"kind":"Constant","args":1}})),
                ],
                vec![
                    movement(7, json!({"kind":"Targets","args":slot}), json!("Exile")),
                    movement(8, marker(7), json!("OwnersBattlefield")),
                ],
            );
            super::super::exile_followups::lower(&mut value).unwrap();
            assert_eq!(
                value["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["objects"]["kind"],
                "One"
            );
        }
    }
    #[test]
    fn explicit_indexed_selected_and_ordinal_targets_are_not_implicit_action_proof() {
        let announcement = spec(0, json!({"UpTo":{"kind":"Constant","args":1}}));
        for reference in [
            json!({"kind":"Target","args":{"slot":0,"index":0}}),
            json!({"kind":"Target","args":{"slot":0,"index":1}}),
            json!({"kind":"Query","args":{"objects":{"kind":"Targets","args":0},"index":0}}),
            json!({"kind":"Query","args":{"objects":{"kind":"Selected","args":0},"index":0}}),
        ] {
            let objects = json!({"kind":"One","args":reference});
            let mut root = spell(
                vec![announcement.clone()],
                vec![
                    movement(7, json!({"kind":"Targets","args":0}), json!("Exile")),
                    movement(8, objects.clone(), json!("OwnersBattlefield")),
                ],
            );
            let path = vec![
                Step::Key("args".into()),
                Step::Key("body".into()),
                Step::Key("instructions".into()),
                Step::Index(1),
                Step::Key("args".into()),
                Step::Key("operation".into()),
                Step::Key("args".into()),
                Step::Key("objects".into()),
                Step::Key("args".into()),
            ];
            assert!(!implicit_action(&root, &path).unwrap());
            let no_keys = Introductions {
                slots: &[],
                keys: &[],
                base: 0,
            };
            let actual = super::super::transitions::reference(
                "current_named_object",
                &root,
                &path,
                &no_keys,
                &super::super::events::Frame::default(),
                reference.clone(),
                None,
            )
            .unwrap();
            assert_eq!(actual, reference);
            // Even a hand-authored enclosing action annotation cannot convert
            // an explicitly indexed spelling into an ordinary pronoun.
            root["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["objects"] =
                json!({"$sym":"exile_followup","args":[objects],"at":30});
            let mut nested = path[..path.len() - 1].to_vec();
            nested.extend([
                Step::Key("args".into()),
                Step::Index(0),
                Step::Key("args".into()),
            ]);
            assert!(!implicit_action(&root, &nested).unwrap());
        }
    }

    #[test]
    fn delayed_capture_keeps_receipt_but_fresh_holder_and_mode_do_not_inherit_it() {
        let target = spec(0, json!({"UpTo":{"kind":"Constant","args":1}}));
        let returned = movement(8, marker(7), json!("OwnersBattlefield"));
        let mut delayed = spell(
            vec![target.clone()],
            vec![
                movement(7, json!({"kind":"Targets","args":0}), json!("Exile")),
                json!({"kind":"Effect","args":{"result":9,"operation":{"kind":"CreateDelayedTrigger","args":{"body":{"instructions":[returned.clone()]}}}}}),
            ],
        );
        super::super::exile_followups::lower(&mut delayed).unwrap();
        assert_eq!(
            delayed["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["body"]["instructions"]
                [0]["args"]["operation"]["args"]["objects"],
            group(7)
        );
        for inner in [
            spell(vec![target.clone()], vec![returned.clone()]),
            json!({"announcement":{"targets":[target],"modes":null},"weight":1,"body":{"instructions":[returned]}}),
        ] {
            let mut outer = spell(
                vec![spec(0, json!({"UpTo":{"kind":"Constant","args":1}}))],
                vec![
                    movement(7, json!({"kind":"Targets","args":0}), json!("Exile")),
                    json!({"kind":"Effect","args":{"result":9,"operation":{"kind":"CreatePerpetualEffect","args":{
                        "objects":{"kind":"One","args":{"kind":"Source"}},
                        "changes":[{"kind":"GrantAbility","args":inner}]
                    }}}}),
                ],
            );
            super::super::exile_followups::lower(&mut outer).unwrap();
            let nested = &outer["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["changes"]
                [0]["args"];
            let body = if nested["kind"] == "Spell" {
                &nested["args"]["body"]
            } else {
                &nested["body"]
            };
            assert_eq!(
                body["instructions"][0]["args"]["operation"]["args"]["objects"]["kind"],
                "One"
            );
        }
    }
}

#[cfg(test)]
mod native_preflight {
    use super::*;
    use crate::engine::{Engine, canonical_abilities};

    fn canonical_set(values: Vec<Value>) -> Vec<Value> {
        let mut values: Vec<_> = values
            .iter()
            .map(|v| canonical_abilities(v).unwrap())
            .collect();
        values.sort_by_key(Value::to_string);
        values.dedup();
        values
    }
    fn with_target_set(value: &mut Value, changed: &mut bool) {
        if value["kind"] == "Move"
            && crate::bind::movement::zone(&value["args"]["destination"]) == Some("Exile")
            && value["args"]["objects"]["kind"] == "One"
            && value["args"]["objects"]["args"]["kind"] == "Target"
        {
            let r = &value["args"]["objects"]["args"]["args"];
            if r["index"] == 0 {
                value["args"]["objects"] = json!({"kind":"Targets","args":r["slot"]});
                *changed = true;
            }
            return;
        }
        crate::traverse::children(value, |v| with_target_set(v, changed));
    }
    #[test]
    fn twenty_target_phrases_and_six_existing_set_controls_preserve_all_reviewed_meanings() {
        let engine = Engine::load().unwrap();
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/optional-exile-targets-native.json"
        ))
        .unwrap();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let text = case["text"].as_str().unwrap();
            let mut expected = case["reviewed_old_readings"].as_array().unwrap().clone();
            // Each selected source frame has one reviewed graveyard-card exile
            // operation; only its zero-or-one target view is an added meaning.
            if case["add_zero_one_view"] == true {
                for old in expected.clone() {
                    let mut view = old;
                    let mut changed = false;
                    with_target_set(&mut view, &mut changed);
                    if changed {
                        expected.push(view);
                    }
                }
            }
            expected.push(case["native"].clone());
            let actual = engine
                .parse(name, text)
                .unwrap_or_else(|why| panic!("{name}: {why:?}"))
                .readings;
            assert!(
                actual.contains(&canonical_abilities(&case["native"]).unwrap()),
                "{name}: whole native frame"
            );
            assert_eq!(
                canonical_set(actual),
                canonical_set(expected),
                "{name}: all-reading review"
            );
        }
    }
    #[test]
    fn single_slot_exile_followup_uses_actual_after_receipt_in_both_input_views() {
        let engine = Engine::load().unwrap();
        let case: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/optional-exile-target-return.json"
        ))
        .unwrap();
        let expected = case["expected"].clone();
        let mut scalar_input = expected.clone();
        scalar_input[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"] =
            json!({"kind":"One","args":{"kind":"Target","args":{"slot":0,"index":0}}});
        let actual = engine
            .parse(
                case["name"].as_str().unwrap(),
                case["text"].as_str().unwrap(),
            )
            .unwrap()
            .readings;
        assert_eq!(
            canonical_set(actual),
            canonical_set(vec![expected, scalar_input])
        );
    }
    #[test]
    fn zero_one_illegal_and_prefixed_slot_reads_obey_the_published_resolution_equations() {
        // eacc cantrip/read.rs:810-820 uses raw.get(0).filter(legal.contains)
        // versus legal.iter(); effects.rs:674 partitions both by exact spec id.
        for raw in [vec![], vec![(42u32, true)], vec![(42u32, false)]] {
            let announced = vec![(0, 11, true), (1, 22, true)]
                .into_iter()
                .chain(raw.iter().map(|(id, ok)| (2, *id, *ok)))
                .collect::<Vec<_>>();
            let raw_slot: Vec<_> = announced.iter().filter(|t| t.0 == 2).map(|t| t.1).collect();
            let legal: Vec<_> = announced
                .iter()
                .filter(|t| t.0 == 2 && t.2)
                .map(|t| t.1)
                .collect();
            let one: Vec<_> = raw_slot
                .first()
                .copied()
                .filter(|o| legal.contains(o))
                .into_iter()
                .collect();
            assert_eq!(one, legal);
            assert!(!one.contains(&11) && !one.contains(&22));
        }
    }

    #[test]
    fn registered_optional_target_family_has_only_the_two_reviewed_views_and_verified_prints() {
        let engine = Engine::load().unwrap();
        let family: Value =
            serde_json::from_str(include_str!("../../families/optional-exile-targets.json"))
                .unwrap();
        let off = engine.rule_set(|n| n.starts_with("m_") || n.starts_with("t_"));
        for case in family["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let text = case["text"].as_str().unwrap();
            let expected = canonical_abilities(&case["expected"]).unwrap();
            let producer = if name.contains("prefix two") { 2 } else { 0 };
            let slot =
                expected[0]["args"]["body"]["instructions"][producer]["args"]["operation"]["args"]
                    ["objects"]["args"]
                    .clone();
            let mut indexed = expected.clone();
            indexed[0]["args"]["body"]["instructions"][producer]["args"]["operation"]["args"]["objects"] =
                json!({"kind":"One","args":{"kind":"Target","args":{"slot":slot,"index":0}}});
            let actual = engine.parse(name, text).unwrap().readings;
            assert_eq!(
                canonical_set(actual.clone()),
                canonical_set(vec![expected.clone(), indexed]),
                "{name}"
            );
            assert_eq!(
                engine.parse_without(name, text, &off).unwrap().readings,
                vec![expected],
                "{name}: registered production"
            );
            for reading in actual {
                let trail = engine.trail_of(name, &reading, text).unwrap().expect(name);
                assert_eq!(
                    engine
                        .print_trail(name, &reading, &trail)
                        .unwrap()
                        .as_deref(),
                    Some(text),
                    "{name}"
                );
                let printed = engine
                    .print_keyed(name, &reading, &[], &[])
                    .unwrap()
                    .expect(name);
                assert!(
                    engine
                        .parse(name, &printed)
                        .unwrap()
                        .readings
                        .contains(&reading),
                    "{name}"
                );
            }
        }
    }
    #[test]
    fn edited_optional_return_controller_reads_current_operand_and_rejects_old_trail() {
        let engine = Engine::load().unwrap();
        let case: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/optional-exile-target-return.json"
        ))
        .unwrap();
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let old = canonical_abilities(&case["expected"]).unwrap();
        let trail = engine.trail_of(name, &old, text).unwrap().unwrap();
        let mut edited = old.clone();
        edited[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["destination"]
            ["Battlefield"]["controller"] = json!({"Player":{"kind":"AbilityController"}});
        let edited = canonical_abilities(&edited).unwrap();
        assert!(!engine.parse(name, text).unwrap().readings.contains(&edited));
        assert!(engine.print_trail(name, &edited, &trail).unwrap().is_none());
        let printed = engine
            .print_keyed(name, &edited, &[], &[])
            .unwrap()
            .expect("edited current controller");
        assert_ne!(printed, text);
        assert!(
            engine
                .parse(name, &printed)
                .unwrap()
                .readings
                .contains(&edited)
        );
    }
}
