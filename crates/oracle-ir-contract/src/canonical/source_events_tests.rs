use super::*;
use crate::{ir::*, semantics, validate};

fn source() -> Value {
    json!({"kind":"SameObject","args":{"kind":"Source"}})
}
fn damage(subject: &str, grouping: &str) -> Value {
    json!({"Damage":{"source_kind":"Permanent","sources":source(),
        "recipients":{"Objects":source()},"kind":"Any","grouping":grouping,"subject":subject}})
}
fn ability(event: Value, object: Value) -> Value {
    json!({"kind":"Triggered","args":{"event":event,"intervening_if":null,
        "limits":{"per_turn":null,"total":null},"announcement":{"targets":[],"modes":null},
        "body":{"instructions":[{"kind":"Effect","args":{"result":7,"operation":{
            "kind":"GainLife","args":{"players":{"kind":"One","args":{"kind":"ControllerOf","args":object}},
                "amount":{"kind":"Constant","args":1}}}}}]}}})
}
fn compared(value: Value) -> Value {
    let mut value = json!([value]);
    semantic_abilities(&mut value).unwrap();
    value
}
fn event_subject() -> Value {
    json!({"kind":"EventSubject"})
}

#[test]
fn damage_source_proofs_agree_with_native_distinguished_object_availability() {
    for subject in ["Source", "Recipient"] {
        for grouping in ["Source", "Recipient", "SourceRecipient"] {
            let event = damage(subject, grouping);
            let typed: EventSpec = serde_json::from_value(event.clone()).unwrap();
            let available = semantics::event_bindings(&typed).object;
            assert_eq!(
                source_events::names_source(&event),
                available,
                "{subject}/{grouping}"
            );
            if available {
                let a = ability(event.clone(), event_subject());
                validate::validate_ability(&serde_json::from_value(a.clone()).unwrap()).unwrap();
                assert_eq!(
                    compared(a),
                    compared(ability(event, json!({"kind":"Source"})))
                );
            }
        }
    }
}

#[test]
fn attack_spell_counter_batches_and_wrappers_keep_the_same_source_identity() {
    let events = [
        json!({"Attack":{"attackers":source(),"by":null,"defenders":{"Players":{"kind":"All"}}}}),
        json!({"Spell":{"event":"Cast","players":{"kind":"All"},"spells":source()}}),
        json!({"Batch":{"event":{"Counters":{"change":"Put","kind":{"PowerToughness":[1,1]},
            "objects":source(),"by":null}},"per":"Subject","count":null,"including":null}}),
        json!({"FirstEachTurn":damage("Recipient","Recipient")}),
        json!({"Filtered":{"event":damage("Source","Source"),"condition":{"kind":"Compare","args":{"left":{"kind":"Constant","args":1},"test":{"kind":"Compare","args":{"comparison":"Equal","value":{"kind":"Constant","args":1}}}}}}}),
    ];
    for event in events {
        assert!(source_events::names_source(&event), "{event}");
        let a = ability(event.clone(), event_subject());
        validate::validate_ability(&serde_json::from_value(a.clone()).unwrap()).unwrap();
        assert_eq!(
            compared(a),
            compared(ability(event, json!({"kind":"Source"})))
        );
    }
}

#[test]
fn source_filters_require_the_correct_damage_role_and_every_alternative() {
    let mut event = damage("Recipient", "SourceRecipient");
    event["Damage"]["recipients"] = json!({"Objects":{"kind":"Any"}});
    assert!(!source_events::names_source(&event));
    let mut event = damage("Source", "SourceRecipient");
    event["Damage"]["sources"] = json!({"kind":"Any"});
    assert!(!source_events::names_source(&event));
    let mut attached = damage("Source", "SourceRecipient");
    attached["Damage"]["sources"] =
        json!({"kind":"SameObject","args":{"kind":"AttachedTo","args":{"kind":"Source"}}});
    assert!(!source_events::names_source(&attached));
    assert!(!source_events::names_source(
        &json!({"AnyOf":[damage("Source","Source"),attached]})
    ));
    assert!(!source_events::names_source(&json!({"AnyOf":[]})));
    let mut other_spell = json!({"Spell":{"event":"Cast","players":{"kind":"All"},"spells":{"kind":"CardType","args":"Creature"}}});
    assert!(!source_events::names_source(&other_spell));
    other_spell["Spell"]["spells"] = json!({"kind":"AnyOf","args":[source(),{"kind":"Any"}]});
    assert!(!source_events::names_source(&other_spell));
    let mut batch = json!({"Batch":{"event":{"Object":{"event":"Attacks","objects":source()}},"per":"Subject","count":null,"including":null}});
    batch["Batch"]["per"] = json!("Whole");
    assert!(!source_events::names_source(&batch));
}

#[test]
fn conjunction_proof_keeps_identifier_origins_and_has_explicit_work_depth_bounds() {
    let target = json!({"kind":"SameObject","args":{"kind":"Target","args":{
        "slot":{"$mtgish_id":11,"$site_origins":[5]},"index":0}}});
    let event =
        json!({"Object":{"event":"Attacks","objects":{"kind":"AllOf","args":[target,source()]}}});
    let before = event.clone();
    assert!(source_events::names_source(&event));
    assert_eq!(event, before);
    let wide =
        json!({"Object":{"event":"Attacks","objects":{"kind":"AllOf","args":vec![source();65]}}});
    assert!(!source_events::names_source(&wide));
    let mut deep = damage("Source", "Source");
    for _ in 0..40 {
        deep = json!({"FirstEachTurn":deep});
    }
    assert!(!source_events::names_source(&deep));
    let expensive = json!({"AnyOf":vec![damage("Source","Source");64]});
    assert!(!source_events::names_source(&expensive));
    let mut unknown = source();
    unknown["unknown"] = json!(true);
    assert!(!source_events::names_source(
        &json!({"Object":{"event":"Attacks","objects":unknown}})
    ));
}

#[test]
fn current_event_proofs_do_not_rewrite_history_headers_or_malformed_references() {
    let mut a = ability(
        json!({"Object":{"event":"Attacks","objects":source()}}),
        event_subject(),
    );
    a["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"] = json!({
        "kind":"EventHistory","args":{"event":{"Pair":{"event":"BecomesBlockedBy",
            "objects":{"kind":"Any"},"others":{"kind":"SameObject","args":event_subject()}}},
            "window":"ThisTurn","measure":"Occurrences"}});
    let typed: Ability = serde_json::from_value(a.clone()).unwrap();
    validate::validate_ability(&typed).unwrap();
    let value = compared(a);
    assert_eq!(
        value[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"]["args"]
            ["event"]["Pair"]["others"]["args"],
        event_subject()
    );
    let malformed = json!({"kind":"EventSubject","unknown":true});
    let value = compared(ability(damage("Source", "Source"), malformed.clone()));
    assert_eq!(
        value[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["players"]["args"]
            ["args"],
        malformed
    );
}

#[test]
fn explicit_damage_roles_normalize_in_one_pass_and_opposite_roles_remain_distinct() {
    let mut event = damage("Source", "SourceRecipient");
    event["Damage"]["recipients"] = json!({"Objects":{"kind":"Any"}});
    let original = ability(event.clone(), json!({"kind":"Damage","args":"Source"}));
    let canonical = compared(original);
    assert_eq!(
        canonical,
        compared(ability(event.clone(), json!({"kind":"Source"})))
    );
    assert_eq!(canonical, compared(canonical[0].clone()));
    assert_ne!(
        canonical,
        compared(ability(event, json!({"kind":"Damage","args":"Recipient"})))
    );
}

#[test]
fn descendants_and_created_triggers_use_their_own_event_and_restore_the_outer_frame() {
    let mut other_event = damage("Source", "SourceRecipient");
    other_event["Damage"]["sources"] = json!({"kind":"Any"});
    for inner_event in [damage("Recipient", "Recipient"), other_event] {
        let mut outer = ability(damage("Source", "Source"), event_subject());
        let inner = ability(inner_event, event_subject());
        let body = inner["args"]["body"].clone();
        for operation in [
            json!({"kind":"CreateEmblem","args":{"actor":{"kind":"One","args":{"kind":"AbilityController"}},"abilities":[inner.clone()]}}),
            json!({"kind":"CreateTrigger","args":{"event":inner["args"]["event"],"duration":"EndOfTurn",
                "announcement":{"targets":[],"modes":null},"once":false,"intervening_if":null,"body":body}}),
        ] {
            let mut instruction = outer["args"]["body"]["instructions"][0].clone();
            instruction["args"]["result"] = json!(8);
            instruction["args"]["operation"] = operation;
            outer["args"]["body"]["instructions"]
                .as_array_mut()
                .unwrap()
                .insert(0, instruction);
            validate::validate_ability(&serde_json::from_value(outer.clone()).unwrap()).unwrap();
            let value = compared(outer.clone());
            let operation = &value[0]["args"]["body"]["instructions"][0]["args"]["operation"];
            let actual_body = if operation["kind"] == "CreateEmblem" {
                &operation["args"]["abilities"][0]["args"]["body"]
            } else {
                &operation["args"]["body"]
            };
            assert_eq!(*actual_body, compared(inner.clone())[0]["args"]["body"]);
            assert_eq!(
                value[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["players"]
                    ["args"]["args"],
                json!({"kind":"Source"})
            );
            outer["args"]["body"]["instructions"]
                .as_array_mut()
                .unwrap()
                .remove(0);
        }
    }
}

#[test]
fn mapped_event_source_proofs_preserve_all_target_and_result_origins() {
    let packet: Value =
        serde_json::from_str(include_str!("../fixtures/extraction-190.json")).unwrap();
    let mut card = crate::migrate::read_json(&packet["records"][0]["card"].to_string()).unwrap();
    let mut value = ability(damage("Recipient", "Recipient"), event_subject());
    value["args"]["announcement"]["targets"] = json!([{
        "slot":11,"kind":"DamageRecipient","count":{"Exactly":{"kind":"Constant","args":1}},
        "filter":{"kind":"Any"},"players":null,"distribute":null,"constraints":[],"chooser":null}]);
    value["args"]["body"]["instructions"][0]["args"]["operation"] = json!({"kind":"DealDamage","args":{
        "source":event_subject(),"recipients":{"kind":"Target","args":{"slot":11,"index":0}},
        "amount":{"kind":"Constant","args":1},"preventable":true}});
    card.faces[0].abilities = vec![serde_json::from_value(value.clone()).unwrap()];
    validate::unified(&card).unwrap();
    let mapped = mapped_unified(&card).unwrap();
    value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["source"] =
        json!({"kind":"Source"});
    card.faces[0].abilities = vec![serde_json::from_value(value).unwrap()];
    let direct = mapped_unified(&card).unwrap();
    assert_eq!(
        serde_json::to_value(mapped.card).unwrap(),
        serde_json::to_value(direct.card).unwrap()
    );
    assert_eq!(
        serde_json::to_value(mapped.map).unwrap(),
        serde_json::to_value(direct.map).unwrap()
    );
}

#[test]
fn shared_trigger_branches_use_the_same_frame_after_source_specialization() {
    let self_event = json!({"ZoneChange":{"objects":{"kind":"AllOf","args":[
        source(),{"kind":"OwnedBy","args":{"kind":"All"}}]},"from":"Graveyard","to":"Battlefield"}});
    let other_event = json!({"ZoneChange":{"objects":{"kind":"Not","args":source()},
        "from":"Graveyard","to":"Battlefield"}});
    let mut split = json!([
        ability(self_event.clone(), event_subject()),
        ability(other_event.clone(), event_subject())
    ]);
    validate::abilities(&serde_json::from_value::<Vec<Ability>>(split.clone()).unwrap()).unwrap();
    let authored = split.clone();
    semantic_abilities(&mut split).unwrap();
    let joined = compared(ability(
        json!({"AnyOf":[self_event,other_event]}),
        event_subject(),
    ));
    assert_eq!(split, joined);
    let once = split.clone();
    semantic_abilities(&mut split).unwrap();
    assert_eq!(split, once);
    let packet: Value =
        serde_json::from_str(include_str!("../fixtures/extraction-190.json")).unwrap();
    let mut card = crate::migrate::read_json(&packet["records"][0]["card"].to_string()).unwrap();
    card.faces[0].abilities = serde_json::from_value(authored).unwrap();
    mapped_unified(&card).unwrap();
}

#[test]
fn branch_comparison_keeps_independent_abilities_future_bodies_and_history_headers() {
    let outer_self = damage("Source", "Source");
    let mut other = damage("Source", "SourceRecipient");
    other["Damage"]["sources"] = json!({"kind":"Any"});
    for variant in 0..3 {
        let mut a = ability(outer_self.clone(), json!({"kind":"Source"}));
        let mut b = ability(other.clone(), event_subject());
        let nested_a = ability(other.clone(), json!({"kind":"Source"}));
        let nested_b = ability(other.clone(), event_subject());
        for (value, inner) in [(&mut a, nested_a), (&mut b, nested_b)] {
            let operation = match variant {
                0 => {
                    json!({"kind":"CreateEmblem","args":{"actor":{"kind":"One","args":{"kind":"AbilityController"}},"abilities":[inner]}})
                }
                1 => json!({"kind":"CreateDelayedTrigger","args":{"event":other,
                    "start":"Immediately","expires":null,"intervening_if":null,"body":inner["args"]["body"]}}),
                _ => {
                    let object = inner["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["players"]["args"]["args"].clone();
                    let mut operation =
                        inner["args"]["body"]["instructions"][0]["args"]["operation"].clone();
                    operation["args"]["players"] =
                        json!({"kind":"One","args":{"kind":"AbilityController"}});
                    operation["args"]["amount"] = json!({"kind":"EventHistory","args":{
                        "event":{"Object":{"event":"Attacks","objects":{"kind":"SameObject","args":object}}},
                        "window":"ThisTurn","measure":"Occurrences"}});
                    operation
                }
            };
            value["args"]["body"]["instructions"][0]["args"]["operation"] = operation;
        }
        let mut split = json!([a, b]);
        validate::abilities(&serde_json::from_value::<Vec<Ability>>(split.clone()).unwrap())
            .unwrap();
        semantic_abilities(&mut split).unwrap();
        assert_eq!(split.as_array().unwrap().len(), 2, "variant {variant}");
    }
}

#[test]
fn equal_explicit_source_bodies_join_without_changing_them_to_event_subjects() {
    let mut other = damage("Source", "SourceRecipient");
    other["Damage"]["sources"] = json!({"kind":"Any"});
    let mut split = json!([
        ability(damage("Source", "Source"), json!({"kind":"Source"})),
        ability(other, json!({"kind":"Source"}))
    ]);
    semantic_abilities(&mut split).unwrap();
    assert_eq!(split.as_array().unwrap().len(), 1);
    assert_eq!(
        split[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["players"]["args"]
            ["args"],
        json!({"kind":"Source"})
    );
}

#[test]
fn oversized_branch_comparison_retains_both_valid_branches() {
    let mut a = ability(damage("Source", "Source"), json!({"kind":"Source"}));
    let mut event = damage("Source", "SourceRecipient");
    event["Damage"]["sources"] = json!({"kind":"Any"});
    let mut b = ability(event, event_subject());
    for value in [&mut a, &mut b] {
        let sample = value["args"]["body"]["instructions"][0].clone();
        value["args"]["body"]["instructions"] = Value::Array(
            (0..1500)
                .map(|id| {
                    let mut instruction = sample.clone();
                    instruction["args"]["result"] = json!(id);
                    instruction
                })
                .collect(),
        );
    }
    let mut split = json!([a, b]);
    validate::abilities(&serde_json::from_value::<Vec<Ability>>(split.clone()).unwrap()).unwrap();
    semantic_abilities(&mut split).unwrap();
    assert_eq!(split.as_array().unwrap().len(), 2);
    let once = split.clone();
    semantic_abilities(&mut split).unwrap();
    assert_eq!(split, once);
}

#[test]
fn a_long_branch_list_cannot_make_progress_again_on_a_second_normalization() {
    let mut other = damage("Source", "SourceRecipient");
    other["Damage"]["sources"] = json!({"kind":"Any"});
    let mut source = ability(damage("Source", "Source"), event_subject());
    let sample = source["args"]["body"]["instructions"][0].clone();
    source["args"]["body"]["instructions"] = Value::Array(
        (0..32)
            .map(|id| {
                let mut effect = sample.clone();
                effect["args"]["result"] = json!(id);
                effect
            })
            .collect(),
    );
    let mut branches = Value::Array(
        (0..64)
            .map(|index| {
                let mut branch = source.clone();
                if index % 2 == 1 {
                    branch["args"]["event"] = other.clone();
                }
                branch
            })
            .collect(),
    );
    validate::abilities(&serde_json::from_value::<Vec<Ability>>(branches.clone()).unwrap())
        .unwrap();
    semantic_abilities(&mut branches).unwrap();
    assert_eq!(branches.as_array().unwrap().len(), 1);
    let once = branches.clone();
    semantic_abilities(&mut branches).unwrap();
    assert_eq!(branches, once);
}

#[test]
fn printing_a_source_subject_view_uses_current_events_and_keeps_identifiers() {
    let original = json!([ability(
        damage("Recipient", "Recipient"),
        json!({"kind":"Source"})
    )]);
    let view = event_subject_print_views(&original).unwrap().remove(0);
    assert_eq!(compared(view[0].clone()), compared(original[0].clone()));
    assert_eq!(
        view[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["players"]["args"]
            ["args"],
        event_subject()
    );
    assert_eq!(view[0]["args"]["event"], original[0]["args"]["event"]);
    assert_eq!(
        view[0]["args"]["body"]["instructions"][0]["args"]["result"],
        json!(7)
    );
    let mut edited = original.clone();
    edited[0]["args"]["event"]["Damage"]["recipients"] = json!({"Objects":{"kind":"Any"}});
    assert!(event_subject_print_views(&edited).unwrap().is_empty());
    assert_ne!(
        compared(edited[0].clone()),
        compared(ability(edited[0]["args"]["event"].clone(), event_subject()))
    );
}

#[test]
fn print_views_can_keep_literal_creator_and_announcement_source_roles() {
    let mut a = ability(damage("Recipient", "Recipient"), json!({"kind":"Source"}));
    a["args"]["announcement"]["targets"] = json!([{
        "slot":11,"kind":"DamageRecipient","count":{"Exactly":{"kind":"Constant","args":1}},
        "filter":{"kind":"Not","args":{"kind":"SameObject","args":{"kind":"Source"}}},
        "players":null,"distribute":null,"constraints":[],"chooser":null}]);
    a["args"]["body"]["instructions"][0]["args"]["operation"] = json!({"kind":"DealDamage","args":{
        "source":{"kind":"Source"},"recipients":{"kind":"Target","args":{"slot":11,"index":0}},
        "amount":{"kind":"Constant","args":1},"preventable":true}});
    a["args"]["body"]["instructions"].as_array_mut().unwrap().push(json!({"kind":"Effect","args":{
        "result":8,"operation":{"kind":"CreateContinuousEffect","args":{"source":{"kind":"Source"},
            "affected":{"Objects":{"kind":"One","args":{"kind":"Source"}}},
            "changes":[{"kind":"Pump","args":{"power":{"kind":"Constant","args":1},"toughness":{"kind":"Constant","args":1}}}],
            "duration":"EndOfTurn","evaluation":"Snapshot"}}}}));
    let original = json!([a]);
    let views = event_subject_print_views(&original).unwrap();
    assert_eq!(views.len(), 4);
    assert_eq!(
        views[0][0]["args"]["announcement"],
        original[0]["args"]["announcement"]
    );
    assert_eq!(
        views[0][0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["source"],
        json!({"kind":"Source"})
    );
    assert_eq!(
        views[0][0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["source"],
        event_subject()
    );
    for view in views {
        validate::abilities(&serde_json::from_value::<Vec<Ability>>(view.clone()).unwrap())
            .unwrap();
        assert_eq!(compared(view[0].clone()), compared(original[0].clone()));
    }
}
