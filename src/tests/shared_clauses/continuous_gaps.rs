//! Complete edited cards and independent reference/receipt populations.
use super::*;

fn edit_kind(value: &mut Value, kind: &str, edit: &mut dyn FnMut(&mut Value)) {
    if value["kind"] == kind {
        edit(value);
    }
    match value {
        Value::Object(fields) => {
            for child in fields.values_mut() {
                edit_kind(child, kind, edit);
            }
        }
        Value::Array(items) => {
            for child in items {
                edit_kind(child, kind, edit);
            }
        }
        _ => {}
    }
}

#[test]
fn snow_modes_print_changed_filters_and_deadlines() {
    let mut frame = expected("Thermal Flux");
    let first = &mut frame[0]["args"]["announcement"]["modes"]["options"][0];
    first["announcement"]["targets"][0]["filter"] =
        json!({"kind":"Not","args":{"kind":"Supertype","args":"Legendary"}});
    first["body"]["instructions"][0]["args"]["operation"]["args"]["duration"] =
        json!({"UntilNextTurn":{"kind":"AbilityController"}});
    let text = printed_types(&frame, &["Instant"]).to_lowercase();
    assert!(
        (text.contains("nonlegendary") || text.contains("other than a legendary"))
            && text.contains("snow"),
        "{text}"
    );
    assert!(text.contains("next turn"), "{text}");
}

#[test]
fn legendary_animation_prints_edited_stats_types_and_keywords() {
    let mut frame = expected("Cacophony Unleashed");
    let changes =
        &mut frame[1]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"];
    changes[1]["args"]["power"] = json!({"kind":"Constant","args":4});
    changes[1]["args"]["toughness"] = json!({"kind":"Constant","args":5});
    changes[2]["args"]["types"]["Listed"] = json!(["Dragon", "Spirit"]);
    changes[4]["args"] = json!({"kind":"Flying"});
    let text = printed_types(&frame, &["Enchantment"]).to_lowercase();
    assert!(text.contains("4/5") && text.contains("dragon"), "{text}");
    assert!(
        text.contains("in addition to its other types") || text.contains("still an enchantment"),
        "{text}"
    );
}

#[test]
fn retained_land_animation_and_later_keyword_keep_independent_operands() {
    let mut frame = expected("Rootwise Survivor");
    let body = &mut frame[1]["args"]["body"]["instructions"];
    body[0]["args"]["operation"]["args"]["placements"][0]["amount"] =
        json!({"kind":"Constant","args":2});
    let changes = &mut body[1]["args"]["operation"]["args"]["changes"];
    changes[0]["args"]["power"] = json!({"kind":"Constant","args":2});
    changes[0]["args"]["toughness"] = json!({"kind":"Constant","args":3});
    changes[1]["args"]["types"]["Listed"] = json!(["Zombie"]);
    body[2]["args"]["operation"]["args"]["changes"][0]["args"] = json!({"kind":"Trample"});
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("2/3") && text.contains("zombie"), "{text}");
    assert!(
        text.contains("trample") && text.contains("next turn"),
        "{text}"
    );
}

#[test]
fn grouped_targets_print_changed_domain_and_both_action_quantities() {
    let mut frame = expected("Tinybones Joins Up");
    frame[1]["args"]["announcement"]["targets"][0]["players"] =
        json!({"kind":"OpponentsOf","args":{"kind":"AbilityController"}});
    let instructions = &mut frame[1]["args"]["body"]["instructions"];
    instructions[0]["args"]["operation"]["args"]["amount"] = json!({"kind":"Constant","args":2});
    instructions[1]["args"]["operation"]["args"]["amount"] = json!({"kind":"Constant","args":3});
    let text = printed_types(&frame, &["Enchantment"]).to_lowercase();
    assert!(
        text.contains("target opponents") && text.contains("3 life"),
        "{text}"
    );
}

#[test]
fn grouped_players_after_an_object_target_keep_their_own_slot() {
    let text = "Destroy target artifact. Any number of target players each mill two cards and lose 3 life.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1);
    let frame = &readings[0];
    assert_eq!(frame[0]["args"]["announcement"]["targets"][1]["slot"], 1);
    let instructions = &frame[0]["args"]["body"]["instructions"];
    assert_eq!(instructions.as_array().unwrap().len(), 3);
    for action in [1, 2] {
        assert_eq!(
            instructions[action]["args"]["operation"]["args"]["players"],
            json!({"kind":"Targets","args":1})
        );
    }
    printed(frame);
}

#[test]
fn upkeep_payment_retains_event_player_when_cost_and_penalty_change() {
    let mut frame = expected("Pillar Tombs of Aku");
    let body = &mut frame[0]["args"]["body"]["instructions"];
    body[0]["args"]["operation"]["args"]["cost"]["parts"][0]["Sacrifice"]["objects"]["args"]["filter"] =
        json!({"kind":"CardType","args":"Artifact"});
    body[1]["args"]["then"]["instructions"][0]["args"]["operation"]["args"]["amount"] =
        json!({"kind":"Constant","args":2});
    let text = printed_types(&frame, &["Enchantment"]).to_lowercase();
    assert!(
        text.contains("artifact") && text.contains("2 life"),
        "{text}"
    );
}

#[test]
fn tap_then_counters_prints_new_controller_and_keeps_the_before_population() {
    let mut frame = expected("Monstrosity of the Lake");
    let actions = &mut frame[0]["args"]["body"]["instructions"][1]["args"]["then"]["instructions"];
    actions[0]["args"]["operation"]["args"]["objects"]["args"]["filter"]["args"][1]["args"] =
        json!({"kind":"One","args":{"kind":"AbilityController"}});
    let placement = &mut actions[1]["args"]["operation"]["args"]["placements"][0];
    assert_eq!(
        placement["recipients"],
        json!({"kind":"Objects","args":{
            "kind":"Transition","args":{"result":1,"phase":"Before"}
        }})
    );
    placement["kind"] = json!({"Named":"ChargeCounter"});
    placement["amount"] = json!({"kind":"Constant","args":3});
    let text = printed(&frame).to_lowercase();
    assert!(
        text.contains("creatures you control") && text.contains("charge"),
        "{text}"
    );
}

#[test]
fn cross_zone_grants_print_independent_filters_and_shared_new_keyword() {
    let mut frame = expected("Lo and Li, Twin Tutors");
    frame[1]["args"]["affected"]["Objects"]["args"]["filter"]["args"][1]["args"] = json!("Dragon");
    frame[2]["args"]["affected"]["Objects"]["args"]["filter"]["args"][0]["args"] = json!("Arcane");
    for i in [1, 2] {
        frame[i]["args"]["changes"][0]["args"] = json!({"kind":"Deathtouch"});
    }
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("dragon") && text.contains("arcane"), "{text}");
    assert!(text.contains("deathtouch"), "{text}");
}

#[test]
fn counted_keyword_disjunction_and_both_speed_grants_print_new_operands() {
    let mut frame = expected("Tsagan, Raider Warlord");
    edit_kind(&mut frame, "HasKeyword", &mut |v| {
        v["args"] = match v["args"]["kind"].as_str().unwrap() {
            "FirstStrike" => json!({"kind":"Flying"}),
            "DoubleStrike" => json!({"kind":"Reach"}),
            _ => panic!("unexpected counted keyword"),
        };
    });
    frame[3]["args"]["then"][0]["args"]["changes"][0]["args"] = json!({"kind":"Lifelink"});
    frame[4]["args"]["then"][0]["args"]["changes"][0]["args"] = json!({"kind":"Haste"});
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("flying") && text.contains("reach"), "{text}");
    assert!(
        text.contains("max speed") && text.contains("lifelink"),
        "{text}"
    );
}

#[test]
fn shared_counter_batch_prints_each_changed_amount_and_kind() {
    let mut frame = expected("Guide of Souls");
    let mut batches = Vec::new();
    visit(&frame, "PutCounters", &mut batches);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0]["placements"].as_array().unwrap().len(), 2);
    edit_kind(&mut frame, "PutCounters", &mut |v| {
        v["args"]["placements"][0]["amount"] = json!({"kind":"Constant","args":3});
        v["args"]["placements"][1]["amount"] = json!({"kind":"Constant","args":2});
        v["args"]["placements"][1]["kind"] = json!({"Named":"ReachCounter"});
    });
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("reach") && text.contains("+1/+1"), "{text}");
}

#[test]
fn shared_counters_do_not_capture_a_preceding_target() {
    let text =
        "Destroy target artifact. Put two +1/+1 counters and a flying counter on target creature.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1);
    let mut batches = Vec::new();
    visit(&readings[0], "PutCounters", &mut batches);
    assert_eq!(batches.len(), 1);
    for contribution in batches[0]["placements"].as_array().unwrap() {
        assert_eq!(
            contribution["recipients"],
            json!({"kind":"Objects","args":{
                "kind":"One","args":{"kind":"Target","args":{"slot":1,"index":0}}
            }})
        );
    }
    printed(&readings[0]);
}

#[test]
fn a_granted_counter_batch_keeps_the_named_creator_distinct() {
    let text = r#"Target creature gains "Whenever this creature attacks, put a +1/+1 counter and a flying counter on Context example." until end of turn."#;
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1);
    let mut batches = Vec::new();
    visit(&readings[0], "PutCounters", &mut batches);
    assert_eq!(batches.len(), 1);
    for contribution in batches[0]["placements"].as_array().unwrap() {
        assert_eq!(
            contribution["recipients"],
            json!({"kind":"Objects","args":{
                "kind":"One","args":{"kind":"GrantingObject"}
            }})
        );
    }
}

#[test]
fn perpetual_type_change_prints_an_edited_source_instead_of_the_drafted_card() {
    let mut frame = expected("Support Skyforge");
    edit_kind(&mut frame, "CreatePerpetualEffect", &mut |v| {
        v["args"]["objects"] = json!({"kind":"One","args":{"kind":"Source"}});
    });
    let frame = canonical_abilities(&frame).unwrap();
    let text = engine()
        .print_keyed("Support Skyforge", &frame, &["Artifact".into()], &[])
        .unwrap()
        .expect("edited perpetual subject must print");
    assert!(
        engine()
            .parse("Support Skyforge", &text)
            .unwrap()
            .readings
            .contains(&frame)
    );
    assert!(text.to_lowercase().contains("artifact creature"), "{text}");
}

#[test]
fn a_nonretained_legendary_creature_replaces_its_prior_card_types() {
    let mut frame = expected("Sarkhan, the Dragonspeaker");
    let changes =
        &mut frame[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"];
    assert_eq!(
        changes[4],
        json!({"kind":"SetCardTypes","args":["Creature"]})
    );
    assert_eq!(changes[3]["args"]["mode"], "Set");
    changes[1]["args"]["power"] = json!({"kind":"Constant","args":5});
    changes[1]["args"]["toughness"] = json!({"kind":"Constant","args":6});
    let text = printed_types(&frame, &["Planeswalker"]).to_lowercase();
    assert!(text.contains("5/6") && text.contains("dragon"), "{text}");
}

#[test]
fn searched_land_animates_after_entry_and_shuffle_with_edited_stats() {
    let mut frame = expected("Emergent Sequence");
    let actions = &mut frame[0]["args"]["body"]["instructions"];
    assert!(
        actions[1]["args"]["operation"]["args"]["destination"]["Battlefield"]["arrival"]["states"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(actions[2]["args"]["operation"]["kind"], "Shuffle");
    assert_eq!(
        actions[3]["args"]["operation"]["kind"],
        "CreateContinuousEffect"
    );
    let changes = &mut actions[3]["args"]["operation"]["args"]["changes"];
    changes[0]["args"]["power"] = json!({"kind":"Constant","args":2});
    changes[0]["args"]["toughness"] = json!({"kind":"Constant","args":3});
    let text = printed_types(&frame, &["Sorcery"]).to_lowercase();
    assert!(text.contains("2/3") && text.contains("shuffle"), "{text}");
}

#[test]
fn a_cast_spell_keeps_a_temporary_grant_only_as_a_spell_change() {
    let only = |text: &str| {
        let readings = engine().parse("Context example", text).unwrap().readings;
        assert_eq!(readings.len(), 1, "{text}: {readings:#?}");
        readings[0].clone()
    };
    let found = |frame: &Value, kind: &str| {
        let mut found = Vec::new();
        visit(frame, kind, &mut found);
        found
    };
    let frame = only("Whenever you cast a creature spell, it gains haste until end of turn.");
    let changes = found(&frame, "ChangeSpell");
    assert_eq!(changes.len(), 1, "{frame:#?}");
    assert_eq!(
        changes[0]["changes"],
        json!([{"kind":"Continuing","args":{"changes":[{"kind":"GrantKeyword","args":{"kind":"Haste"}}],"until":"EndOfTurn"}}])
    );
    assert!(found(&frame, "CreateContinuousEffect").is_empty());
    printed(&frame);
    let frame = only("Whenever a creature you control enters, it gains haste until end of turn.");
    assert!(found(&frame, "ChangeSpell").is_empty(), "{frame:#?}");
    assert_eq!(found(&frame, "CreateContinuousEffect").len(), 1);
    printed(&frame);
}

#[test]
fn a_granted_self_trigger_reads_it_as_its_event_subject() {
    for holder in ["Enchanted creature has", "Creatures you control have"] {
        let text = format!(
            "{holder} \"Whenever this creature becomes the target of a spell, put a +1/+1 counter on it.\""
        );
        let readings = engine().parse("Context example", &text).unwrap().readings;
        assert_eq!(readings.len(), 1, "{text}: {readings:#?}");
        let mut counters = Vec::new();
        visit(&readings[0], "PutCounters", &mut counters);
        assert_eq!(
            counters[0]["placements"][0]["recipients"],
            json!({"kind":"Objects","args":{"kind":"One","args":{"kind":"EventSubject"}}}),
            "{text}"
        );
        printed_types(&readings[0], &["Enchantment"]);
    }
}
