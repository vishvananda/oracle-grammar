//! Whole-card semantics, edited operands and references across shared clauses.
use crate::engine::{Engine, Refusal, canonical_abilities, validate_abilities};
use serde_json::{Value, json};

fn engine() -> &'static Engine {
    crate::test_support::engine()
}

fn cases() -> Vec<Value> {
    [
        include_str!("../../tests/fixtures/shared-clauses-0.json"),
        include_str!("../../tests/fixtures/shared-clauses-1.json"),
        include_str!("../../tests/fixtures/shared-clauses-2.json"),
        include_str!("../../tests/fixtures/shared-clauses-3.json"),
        include_str!("../../tests/fixtures/shared-clauses-4.json"),
        include_str!("../../tests/fixtures/shared-clauses-5.json"),
        include_str!("../../tests/fixtures/shared-clauses-6.json"),
        include_str!("../../tests/fixtures/shared-clauses-7.json"),
        include_str!("../../tests/fixtures/shared-clauses-8.json"),
        include_str!("../../tests/fixtures/shared-clauses-9.json"),
        include_str!("../../tests/fixtures/shared-clauses-10.json"),
        include_str!("../../tests/fixtures/shared-clauses-11.json"),
        include_str!("../../tests/fixtures/shared-clauses-12.json"),
        include_str!("../../tests/fixtures/shared-clauses-13.json"),
        include_str!("../../tests/fixtures/shared-clauses-14.json"),
        include_str!("../../tests/fixtures/shared-clauses-15.json"),
        include_str!("../../tests/fixtures/shared-clauses-16.json"),
        include_str!("../../tests/fixtures/shared-clauses-17.json"),
        include_str!("../../tests/fixtures/shared-clauses-18.json"),
        include_str!("../../tests/fixtures/shared-clauses-19.json"),
        include_str!("../../tests/fixtures/shared-clauses-20.json"),
        include_str!("../../tests/fixtures/shared-clauses-21.json"),
        include_str!("../../tests/fixtures/shared-clauses-22.json"),
        include_str!("../../tests/fixtures/shared-clauses-23.json"),
        include_str!("../../tests/fixtures/shared-clauses-24.json"),
        include_str!("../../tests/fixtures/shared-clauses-25.json"),
        include_str!("../../tests/fixtures/shared-clauses-26.json"),
        include_str!("../../tests/fixtures/shared-clauses-27.json"),
        include_str!("../../tests/fixtures/shared-clauses-28.json"),
        include_str!("../../tests/fixtures/shared-clauses-29.json"),
        include_str!("../../tests/fixtures/shared-clauses-30.json"),
        include_str!("../../tests/fixtures/shared-clauses-31.json"),
        include_str!("../../tests/fixtures/shared-clauses-32.json"),
    ]
    .into_iter()
    .flat_map(|s| serde_json::from_str::<Vec<Value>>(s).unwrap())
    .collect()
}

fn expected(name: &str) -> Value {
    crate::bind::lowered(&cases().into_iter().find(|c| c["name"] == name).unwrap()["expected"])
}

fn printed(frame: &Value) -> String {
    printed_types(frame, &["Creature"])
}

fn printed_types(frame: &Value, types: &[&str]) -> String {
    let frame = canonical_abilities(frame).unwrap();
    validate_abilities(&frame).unwrap();
    let types: Vec<_> = types.iter().map(|ty| (*ty).to_string()).collect();
    let text = engine()
        .print_keyed("Context example", &frame, &types, &[])
        .unwrap()
        .expect("edited operands must have a verified print");
    assert!(
        engine()
            .parse("Context example", &text)
            .unwrap()
            .readings
            .contains(&frame),
        "{text}"
    );
    text
}

/// The one reading of `text`, which ordinary rules reach with every seeded rule off.
fn compositional(text: &str) -> Value {
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{text}: {readings:#?}");
    let audited = engine()
        .audited("Context example", text, &crate::parse::RuleSet::default())
        .unwrap();
    assert!(
        audited.iter().any(|(value, rules)| {
            canonical_abilities(value).unwrap() == readings[0] && rules.iter().all(Vec::is_empty)
        }),
        "{text} needs a seeded rule: {audited:?}"
    );
    readings[0].clone()
}

fn visit(value: &Value, kind: &str, found: &mut Vec<Value>) {
    if value["kind"] == kind {
        found.push(value["args"].clone());
    }
    match value {
        Value::Object(fields) => fields.values().for_each(|v| visit(v, kind, found)),
        Value::Array(items) => items.iter().for_each(|v| visit(v, kind, found)),
        _ => {}
    }
}

#[test]
fn complete_cards_include_native_or_independently_reviewed_corrected_ir() {
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let actual = engine()
            .parse(name, case["text"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            actual.readings.len(),
            case["readings"].as_u64().unwrap() as usize,
            "{name}"
        );
        assert!(
            actual
                .readings
                .contains(&crate::bind::normal_form(&case["expected"])),
            "{name}"
        );
        if let Some(forbidden) = case.get("forbidden") {
            assert!(
                !actual
                    .readings
                    .contains(&canonical_abilities(forbidden).unwrap()),
                "{name}: the former wrong recipient must not remain an alternative"
            );
        }
        for forbidden in case["forbidden_readings"].as_array().into_iter().flatten() {
            assert!(
                !actual
                    .readings
                    .contains(&canonical_abilities(forbidden).unwrap()),
                "{name}: a return must retain the actual exiled-card receipt"
            );
        }
        for reading in actual.readings {
            validate_abilities(&reading).unwrap();
        }
    }
}

#[test]
fn edited_die_face_is_an_expiry_event_amount() {
    let mut frame = expected("Drop Tower");
    let expiry = &mut frame[0]["args"]["body"]["args"]["body"]["instructions"][0]["args"]["operation"]
        ["args"]["duration"]["Either"];
    assert_eq!(expiry[0], "EndOfTurn");
    expiry[1]["UntilEvent"]["Filtered"]["condition"]["args"]["test"]["args"]["value"]["args"] =
        json!(3);
    let text = printed(&frame);
    assert!(text.contains("rolls"), "{text}");
    assert!(text.contains('3') || text.contains("three"), "{text}");
}

#[test]
fn cleanup_sacrifice_retains_the_named_creator_inside_a_holder_trigger() {
    let text = "Target creature gains \"When this creature attacks or blocks, sacrifice Context example at the beginning of the next cleanup step.\" until end of turn.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1);
    let mut delayed = Vec::new();
    visit(&readings[0], "CreateDelayedTrigger", &mut delayed);
    assert_eq!(delayed.len(), 1);
    assert_eq!(delayed[0]["event"]["Turn"]["point"], "CleanupStepBegins");
    assert_eq!(
        delayed[0]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"],
        json!({"kind":"One","args":{"kind":"GrantingObject"}})
    );
}

#[test]
fn fellowship_choices_are_collected_before_transfer_and_aid_counters_use_new_operands() {
    let mut frame = expected("Elrond of the White Council");
    let ins = &mut frame[0]["args"]["body"]["instructions"];
    assert_eq!(
        ins[2]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["player"],
        json!({"kind":"Bound","args":0})
    );
    assert_eq!(
        ins[3]["args"]["operation"]["args"]["affected"]["Objects"],
        json!({"kind":"Selected","args":0})
    );
    assert_eq!(
        ins[4]["args"]["operation"]["args"]["affected"]["Objects"],
        json!({"kind":"Affected","args":1})
    );
    let placement = &mut ins[5]["args"]["operation"]["args"]["placements"][0];
    placement["kind"] = json!({"Named":"ChargeCounter"});
    placement["amount"] = json!({"kind":"Multiply","args":[{"kind":"Constant","args":2},{"kind":"Votes","args":{"Word":"Aid"}}]});
    let text = printed(&frame);
    assert!(text.contains("charge"), "{text}");
}

#[test]
fn powerstone_quantity_does_not_scale_each_tokens_mana_ability() {
    let text = "Create two tapped Powerstone tokens.";
    let parsed = engine().parse("Context example", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let mut creation = Vec::new();
    let mut mana = Vec::new();
    visit(&parsed.readings[0], "CreateTokens", &mut creation);
    visit(&parsed.readings[0], "AddMana", &mut mana);
    assert_eq!(
        creation[0]["tokens"][0]["amount"],
        json!({"kind":"Constant","args":2})
    );
    assert_eq!(
        mana[0]["mana"]["Amount"]["amount"],
        json!({"kind":"Constant","args":1})
    );
    assert_eq!(
        mana[0]["players"],
        json!({"kind":"One","args":{"kind":"AbilityController"}})
    );
    printed(&parsed.readings[0]);
}

#[test]
fn granted_owner_prohibition_distinguishes_holder_and_named_creator() {
    for (subject, anchor) in [
        ("This creature", "Source"),
        ("Context example", "GrantingObject"),
    ] {
        let text = format!(
            "Target creature gains \"{subject} can't attack its owner.\" until end of turn."
        );
        let readings = engine().parse("Context example", &text).unwrap().readings;
        assert_eq!(readings.len(), 1, "{text}");
        let mut owners = Vec::new();
        visit(&readings[0], "OwnerOf", &mut owners);
        assert_eq!(owners, vec![json!({"kind":anchor})]);
    }
}

#[test]
fn untap_prohibition_names_one_players_step_but_affects_every_creature() {
    let frame = expected("Blinding Beam");
    let mut effects = Vec::new();
    visit(&frame, "CreateContinuousEffect", &mut effects);
    assert_eq!(effects.len(), 2);
    for effect in effects {
        assert_eq!(
            effect["affected"]["Objects"]["args"]["filter"],
            json!({"kind":"CardType","args":"Creature"})
        );
        let target = json!({"kind":"Target","args":{"slot":0,"index":0}});
        assert_eq!(effect["duration"], json!({"DuringNextUntapStep":target}));
        assert_eq!(
            effect["changes"][0]["args"]["UntapStep"]["players"],
            json!({"kind":"One","args":target})
        );
    }
}

#[test]
fn edited_base_stats_keep_separate_live_and_snapshot_effects() {
    let frame = expected("Atomic Microsizer");
    let mut ability = json!([frame[1].clone()]);
    let ins = &mut ability[0]["args"]["body"]["instructions"];
    assert_eq!(
        ins[0]["args"]["operation"]["args"]["evaluation"],
        "LiveObjects"
    );
    assert_eq!(
        ins[1]["args"]["operation"]["args"]["evaluation"],
        "Snapshot"
    );
    ins[1]["args"]["operation"]["args"]["changes"][0]["args"]["power"]["args"] = json!(2);
    ins[1]["args"]["operation"]["args"]["changes"][0]["args"]["toughness"]["args"] = json!(5);
    assert!(printed(&ability).contains("2/5"));
}

#[test]
fn deictic_history_and_vote_outcomes_require_their_producing_clause() {
    for text in [
        "If you've done all four this turn, transform Context example.",
        "If no creature got votes, each player draws a card.",
    ] {
        let result = engine().parse("Context example", text);
        assert!(!matches!(result, Err(Refusal::Limit(_))), "{text}");
        assert!(result.is_err(), "{text}");
    }
}

#[test]
fn all_angel_readings_boost_the_chosen_hand_card() {
    let case = cases()
        .into_iter()
        .find(|c| c["name"] == "Angel of Unity")
        .unwrap();
    let readings = engine()
        .parse("Angel of Unity", case["text"].as_str().unwrap())
        .unwrap()
        .readings;
    for reading in readings {
        let mut effects = Vec::new();
        visit(&reading, "Effect", &mut effects);
        let mut choice = None;
        for effect in effects {
            let operation = &effect["operation"];
            if operation["kind"] == "ChooseObjects" {
                choice = effect["result"].as_u64();
            }
            if operation["kind"] == "CreatePerpetualEffect" {
                assert_eq!(
                    operation["args"]["objects"],
                    json!({"kind":"One","args":{
                        "kind":"Result","args":{"result":choice.expect("prior choice"),"phase":"After","index":0}
                    }})
                );
            }
        }
        assert!(choice.is_some());
    }
}

#[test]
fn seek_discard_captures_the_moved_card_after_an_independent_prefix() {
    let text = "Draw a card. Seek a non-Shark card. Discard that card at the beginning of the next end step.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1);
    let mut effects = Vec::new();
    visit(&readings[0], "Effect", &mut effects);
    let movement = effects
        .iter()
        .find(|e| e["operation"]["kind"] == "Move")
        .unwrap();
    let discard = effects
        .iter()
        .find(|e| e["operation"]["kind"] == "DiscardObjects")
        .unwrap();
    assert_eq!(
        discard["operation"]["args"]["objects"],
        json!({"kind":"One","args":{
            "kind":"Result","args":{"result":movement["result"],"phase":"After","index":0}
        }})
    );
    assert!(
        engine()
            .parse(
                "Context example",
                "Discard that card at the beginning of the next end step."
            )
            .is_err()
    );
    let mut frame = expected("Arius, Flyby Trawler");
    frame[2]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"]["args"]["filter"]
        ["args"]["args"] = json!("Dragon");
    assert!(printed(&frame).to_lowercase().contains("dragon"));
}

#[test]
fn sacrifice_history_damage_prints_the_edited_filter_in_both_reads() {
    let mut frame = expected("Sawblade Skinripper");
    let trigger = &mut frame[2]["args"];
    trigger["intervening_if"]["args"]["left"]["args"]["event"]["Player"]["objects"] =
        json!({"kind":"CardType","args":"Artifact"});
    trigger["body"]["instructions"][0]["args"]["operation"]["args"]["amount"]["args"]["event"]["Player"]
        ["objects"] = json!({"kind":"CardType","args":"Artifact"});
    assert!(printed(&frame).to_lowercase().contains("artifacts"));
}

#[test]
fn optional_reveal_followup_prints_new_life_loss_instead_of_old_mana_value() {
    let full = expected("Stronghold Arena");
    let mut frame = json!([{"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},
        "body":full.as_array().unwrap().last().unwrap()["args"]["body"]}}]);
    frame[0]["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][1]["args"]["operation"]
        ["args"]["amount"] = json!({"kind":"Constant","args":3});
    let text = printed(&frame).to_lowercase();
    assert!(
        text.contains("3 life") || text.contains("three life"),
        "{text}"
    );
}

#[test]
fn nested_paid_future_trigger_keeps_creator_distinct_from_granted_holder() {
    let text = r#"Target creature gains "Whenever this creature attacks, you may pay {R}. If you do, until end of turn, whenever Context example deals combat damage to an opponent, Context example deals that much damage to each other opponent." until end of turn."#;
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert!(!readings.is_empty());
    for frame in readings {
        let mut created = Vec::new();
        visit(&frame, "CreateTrigger", &mut created);
        assert_eq!(created.len(), 1);
        assert_eq!(
            created[0]["event"]["Damage"]["sources"],
            json!({"kind":"SameObject","args":{"kind":"GrantingObject"}})
        );
        let source = &created[0]["body"]["instructions"][0]["args"]["operation"]["args"]["source"];
        assert!(matches!(
            source["kind"].as_str(),
            Some("GrantingObject" | "EventSubject")
        ));
    }
}

#[test]
fn every_optional_exile_return_uses_the_actual_paid_after_objects() {
    let case = cases()
        .into_iter()
        .find(|c| c["name"] == "Long River Lurker")
        .unwrap();
    let readings = engine()
        .parse("Long River Lurker", case["text"].as_str().unwrap())
        .unwrap()
        .readings;
    assert_eq!(readings.len(), 2);
    for frame in readings {
        let mut triggers = Vec::new();
        visit(&frame, "CreateTrigger", &mut triggers);
        assert_eq!(triggers.len(), 1);
        let instructions = &triggers[0]["body"]["instructions"];
        let payment = &instructions[0]["args"];
        let branch = &instructions[1]["args"];
        assert_eq!(
            branch["condition"],
            json!({"kind":"PaymentChosen","args":payment["result"]})
        );
        let part = &payment["operation"]["args"]["cost"]["parts"][0];
        let after = if let Some(action) = part.get("PlayerAction") {
            let movement = &action["body"]["instructions"][0]["args"];
            json!({"kind":"Transition","args":{"result":movement["result"],"phase":"After"}})
        } else {
            assert!(part.get("Move").is_some());
            json!({"kind":"CostObjects","args":{"payment":{"Resolution":payment["result"]},
                "component":[0],"view":"Performed","phase":"After"}})
        };
        assert_eq!(
            branch["then"]["instructions"][0]["args"]["operation"]["args"]["objects"],
            json!({"kind":"InZone","args":{"objects":after,"zone":"Exile"}})
        );
    }
    let frame = expected("Long River Lurker");
    let trigger = &frame[2]["args"];
    let synthesized = json!([{"kind":"Spell","args":{
        "announcement":trigger["announcement"],"body":trigger["body"]
    }}]);
    printed(&synthesized);
}

#[test]
fn ordinary_destroy_success_prints_the_edited_damage_and_real_mana_payment() {
    let mut frame = expected("Cosmic Horror");
    let instructions = &mut frame[1]["args"]["body"]["instructions"];
    instructions[0]["args"]["operation"]["args"]["cost"]["parts"][0]["Mana"] =
        json!([{"Color":"Red"}]);
    instructions[1]["args"]["then"]["instructions"][1]["args"]["then"]["instructions"][0]["args"]
        ["operation"]["args"]["amount"] = json!({"kind":"Constant","args":4});
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("{r}"), "{text}");
    assert!(
        text.contains("4 damage") || text.contains("four damage"),
        "{text}"
    );
}

#[test]
fn selected_counter_grant_prints_the_edited_holders_life_loss() {
    let mut frame = expected("Olinda the Oblivious");
    frame[0]["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["changes"][0]["args"]
        ["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"] =
        json!({"kind":"Constant","args":5});
    let text = printed(&frame).to_lowercase();
    assert!(
        text.contains("5 life") || text.contains("five life"),
        "{text}"
    );
}

#[test]
fn granted_destroy_success_uses_the_holder_and_its_controller() {
    let text = r#"Target creature gains "At the beginning of your upkeep, destroy this creature unless you pay {R}. If this creature is destroyed this way, it deals 4 damage to you." until end of turn."#;
    let frames = engine().parse("Context example", text).unwrap().readings;
    assert!(!frames.is_empty());
    for frame in frames {
        let mut destroyed = Vec::new();
        let mut damage = Vec::new();
        visit(&frame, "Destroy", &mut destroyed);
        visit(&frame, "DealDamage", &mut damage);
        assert_eq!(destroyed.len(), 1);
        assert_eq!(
            destroyed[0]["objects"],
            json!({"kind":"One","args":{"kind":"Source"}})
        );
        assert_eq!(damage[0]["source"], json!({"kind":"Source"}));
        assert_eq!(
            damage[0]["recipients"],
            json!({"kind":"Players","args":{"kind":"One","args":{"kind":"AbilityController"}}})
        );
    }
}

#[test]
fn later_target_prints_its_edited_kind_and_damage_without_moving_the_capture() {
    let mut frame = expected("Swooping Pteranodon");
    let future = &mut frame[2]["args"]["body"]["instructions"][3]["args"]["operation"]["args"];
    assert_eq!(future["announcement"]["targets"][0]["slot"], 1);
    future["announcement"]["targets"][0]["filter"] = json!({"kind":"CardType","args":"Artifact"});
    future["body"]["instructions"][0]["args"]["operation"]["args"]["amount"] =
        json!({"kind":"Constant","args":5});
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("target artifact"), "{text}");
    assert!(
        text.contains("5 damage") || text.contains("five damage"),
        "{text}"
    );
    let early = engine()
        .parse(
            "Context example",
            "Target land deals 3 damage to target creature at the beginning of the next end step.",
        )
        .unwrap();
    for frame in early.readings {
        assert_eq!(
            frame[0]["args"]["announcement"]["targets"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let mut delayed = Vec::new();
        visit(&frame, "CreateDelayedTrigger", &mut delayed);
        assert_eq!(delayed.len(), 1);
    }
}

#[test]
fn sacrifice_count_prints_changed_food_filter_keywords_and_receipt_stats() {
    for counted in [false, true] {
        let mut frame = expected("Campsite Cuisine");
        let instructions =
            &mut frame[1]["args"]["body"]["instructions"][0]["args"]["body"]["instructions"];
        instructions[0]["args"]["operation"]["args"]["objects"]["args"]["filter"] =
            json!({"kind":"Subtype","args":"Treasure"});
        let receipt = json!({"kind":"CountObjects","args":{"kind":"Transition","args":{
            "result":instructions[0]["args"]["result"],"phase":"Before"
        }}});
        let changes = &mut instructions[1]["args"]["operation"]["args"]["body"]["instructions"][0]
            ["args"]["operation"]["args"]["changes"];
        changes[0]["args"]["power"] = if counted {
            receipt.clone()
        } else {
            json!({"kind":"Constant","args":2})
        };
        changes[0]["args"]["toughness"] = if counted {
            receipt
        } else {
            json!({"kind":"Constant","args":1})
        };
        changes[1]["args"] = json!({"kind":"Haste"});
        changes[2]["args"] = json!({"kind":"Vigilance"});
        let text = printed_types(&frame, &["Enchantment"]).to_lowercase();
        assert!(
            text.contains("treasures") && text.contains("haste") && text.contains("vigilance"),
            "{text}"
        );
        assert!(
            text.contains(if counted { "+x/+x" } else { "+2/+1" }),
            "{text}"
        );
    }
}

#[test]
fn sacrifice_receipt_after_a_draw_never_binds_the_callback_to_the_draw() {
    let mut frame = expected("Campsite Cuisine");
    frame[1]["args"]["body"]["instructions"].as_array_mut().unwrap().insert(0,
        json!({"kind":"Effect","args":{"result":999u32,"operation":{"kind":"Draw","args":{
            "players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}
        }}}}));
    let case = cases()
        .into_iter()
        .find(|c| c["name"] == "Campsite Cuisine")
        .unwrap();
    let text = case["text"].as_str().unwrap().replace(
        "Whenever you attack, you may",
        "Whenever you attack, draw a card. You may",
    );
    let attempt = engine().inspect_with_candidate_ir("Campsite Cuisine", &text);
    assert!(
        attempt.result.is_ok(),
        "{:?}: {}",
        attempt.result.as_ref().err(),
        serde_json::to_string(&attempt.evidence).unwrap()
    );
    let parsed = attempt.result.unwrap();
    assert_eq!(parsed.readings, vec![canonical_abilities(&frame).unwrap()]);
}

#[test]
fn later_target_in_a_granted_trigger_keeps_the_named_creator() {
    let text = r#"Target creature gains "Whenever this creature attacks, at the beginning of the next end step, target land deals 3 damage to Context example." until end of turn."#;
    let frames = engine().parse("Context example", text).unwrap().readings;
    assert!(!frames.is_empty());
    for frame in frames {
        let mut damage = Vec::new();
        visit(&frame, "DealDamage", &mut damage);
        assert_eq!(damage.len(), 1);
        assert_eq!(
            damage[0]["recipients"],
            json!({"kind":"Objects","args":{"kind":"One","args":{"kind":"GrantingObject"}}})
        );
        let mut future = Vec::new();
        visit(&frame, "CreateTrigger", &mut future);
        assert_eq!(
            future[0]["announcement"]["targets"][0]["filter"],
            json!({"kind":"CardType","args":"Land"})
        );
    }
}

#[test]
fn paid_copy_return_prints_changed_target_duration_and_controller() {
    let mut frame = expected("Identity Thief");
    frame[0]["args"]["announcement"]["targets"][0]["filter"]["args"][0]["args"] = json!("Artifact");
    let branch = &mut frame[0]["args"]["body"]["instructions"][1]["args"]["then"]["instructions"];
    branch[0]["args"]["operation"]["args"]["duration"] = json!("EndOfCombat");
    branch[1]["args"]["operation"]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]
        ["destination"]["Battlefield"]["controller"] =
        json!({"Player":{"kind":"AbilityController"}});
    let text = printed(&frame).to_lowercase();
    assert!(
        text.contains("artifact")
            && (text.contains("until end of combat") || text.contains("this combat")),
        "{text}"
    );
    assert!(text.contains("under your control"), "{text}");
    assert!(
        text.contains("copy of it") || text.contains("copy of that artifact"),
        "{text}"
    );
}

#[test]
fn paid_copy_return_after_a_draw_or_payment_retains_its_own_receipt() {
    let case = cases()
        .into_iter()
        .find(|c| c["name"] == "Identity Thief")
        .unwrap();
    for payment in [false, true] {
        let draw = json!({"kind":"Effect","args":{"result":999u32,"operation":{"kind":"Draw","args":{
            "players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}
        }}}});
        let prefix = if payment {
            vec![
                json!({"kind":"Effect","args":{"result":777u32,"operation":{"kind":"PayCost","args":{
                    "player":{"kind":"AbilityController"},"cost":{"parts":[{"Mana":[{"Generic":1}]}]},"optional":true
                }}}}),
                json!({"kind":"If","args":{"condition":{"kind":"PaymentChosen","args":777u32},
                "then":{"instructions":[draw]},"otherwise":{"instructions":[]}}}),
            ]
        } else {
            vec![draw]
        };
        let mut frame = expected("Identity Thief");
        frame[0]["args"]["body"]["instructions"]
            .as_array_mut()
            .unwrap()
            .splice(0..0, prefix);
        let source = if payment {
            "attacks, you may pay {1}. If you do, draw a card. You may"
        } else {
            "attacks, draw a card. You may"
        };
        let text = case["text"]
            .as_str()
            .unwrap()
            .replace("attacks, you may", source);
        let parsed = engine().parse("Identity Thief", &text).unwrap();
        assert_eq!(parsed.readings, vec![canonical_abilities(&frame).unwrap()]);
    }
}

#[test]
fn cycling_and_per_opponent_targets_print_the_edited_filters() {
    let mut wave = expected("Dismantling Wave");
    wave[0]["args"]["announcement"]["targets"][0]["filter"] =
        json!({"kind":"CardType","args":"Creature"});
    let text = printed_types(&wave, &["Sorcery"]).to_lowercase();
    assert!(text.contains("target creature"), "{text}");

    let mut drift = expected("Astral Drift");
    drift[0]["args"]["ability"]["args"]["announcement"]["targets"][0]["filter"] =
        json!({"kind":"CardType","args":"Artifact"});
    drift[1]["args"]["announcement"]["targets"][0]["filter"] =
        json!({"kind":"CardType","args":"Artifact"});
    let text = printed_types(&drift, &["Enchantment"]).to_lowercase();
    assert!(text.contains("target artifact"), "{text}");
}

#[test]
fn granted_per_opponent_targets_use_the_holders_controller_and_own_announcement() {
    let text = r#"Target creature gains "Whenever this creature attacks, for each opponent, destroy up to one target artifact that player controls." until end of turn."#;
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert!(!readings.is_empty());
    for frame in readings {
        let mut granted = Vec::new();
        visit(&frame, "Triggered", &mut granted);
        assert_eq!(granted.len(), 1);
        let spec = &granted[0]["announcement"]["targets"][0];
        assert_eq!(
            spec["count"],
            json!({"PerPlayer":{
                "players":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}},
                "each":{"UpTo":{"kind":"Constant","args":1}}
            }})
        );
        assert_eq!(spec["slot"], 0);
        assert_eq!(spec["filter"], json!({"kind":"CardType","args":"Artifact"}));
        assert_eq!(
            frame[0]["args"]["announcement"]["targets"][0]["filter"],
            json!({"kind":"CardType","args":"Creature"})
        );
        assert_eq!(
            granted[0]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"],
            json!({"kind":"Targets","args":0})
        );
    }
}

#[test]
fn named_player_hand_entry_prints_the_edited_card_filter() {
    let mut frame = expected("Braids, Conjurer Adept");
    frame[0]["args"]["body"]["instructions"][0]["args"]["body"]["instructions"][0]["args"]["operation"]
        ["args"]["objects"]["args"]["filter"] = json!({"kind":"CardType","args":"Enchantment"});
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("enchantment card"), "{text}");
}

#[test]
fn unsuspecting_prints_changed_grant_and_static_stats() {
    let mut frame = expected("Airtight Alibi");
    frame[2]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["changes"][0]["args"] =
        json!({"kind":"Vigilance"});
    frame[3]["args"]["changes"][0]["args"]["power"] = json!({"kind":"Constant","args":1});
    frame[3]["args"]["changes"][0]["args"]["toughness"] = json!({"kind":"Constant","args":3});
    let text = printed_types(&frame, &["Enchantment"]).to_lowercase();
    assert!(
        text.contains("vigilance") && text.contains("+1/+3"),
        "{text}"
    );
}

#[test]
fn named_player_choice_in_a_granted_trigger_uses_the_upkeep_player() {
    let text = r#"Target creature gains "At the beginning of each player's upkeep, that player may put an enchantment card from their hand onto the battlefield." until end of turn."#;
    let frames = engine().parse("Context example", text).unwrap().readings;
    let mut direct = false;
    for frame in frames {
        let mut triggers = Vec::new();
        visit(&frame, "Triggered", &mut triggers);
        assert_eq!(triggers.len(), 1);
        let first = &triggers[0]["body"]["instructions"][0];
        if first["kind"] != "Optional" {
            continue;
        }
        direct = true;
        assert_eq!(first["args"]["decider"], json!({"kind":"EventSubject"}));
        let body = &first["args"]["body"]["instructions"];
        let choice = &body[0]["args"]["operation"]["args"];
        assert_eq!(choice["player"], json!({"kind":"EventSubject"}));
        assert_eq!(
            choice["objects"]["args"]["scope"],
            json!({"Players":{
                "zone":"Hand","players":{"kind":"One","args":{"kind":"EventSubject"}}
            }})
        );
        assert_eq!(
            body[1]["args"]["operation"]["args"]["destination"]["Battlefield"]["controller"],
            json!({"Player":{"kind":"EventSubject"}})
        );
    }
    assert!(direct);
}

#[test]
fn granted_unsuspecting_keeps_the_named_creator_separate_from_the_holder() {
    let text = r#"Target creature gains "Whenever this creature attacks, untap Context example. If it's suspected, it's no longer suspected." until end of turn."#;
    let frames = engine().parse("Context example", text).unwrap().readings;
    assert!(!frames.is_empty());
    for frame in frames {
        let mut actions = Vec::new();
        visit(&frame, "KeywordEach", &mut actions);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["action"], "Unsuspect");
        assert_eq!(
            actions[0]["objects"],
            json!({"kind":"One","args":{"kind":"GrantingObject"}})
        );
        assert_eq!(
            actions[0]["actor"],
            json!({"kind":"One","args":{"kind":"AbilityController"}})
        );
    }
}

#[path = "shared_clauses/phrase_gaps.rs"]
mod phrase_gaps;

#[path = "shared_clauses/continuous_gaps.rs"]
mod continuous_gaps;

#[path = "shared_clauses/definitions.rs"]
mod definitions;

#[path = "shared_clauses/phrase_composition.rs"]
mod phrase_composition;
#[path = "shared_clauses/scoped_heads.rs"]
mod scoped_heads;

#[path = "shared_clauses/revealed_followups.rs"]
mod revealed_followups;

#[path = "shared_clauses/sacrificed_receipts.rs"]
mod sacrificed_receipts;

#[path = "shared_clauses/audit_derivations.rs"]
mod audit_derivations;

#[path = "shared_clauses/self_words.rs"]
mod self_words;

#[path = "shared_clauses/searches.rs"]
mod searches;

#[path = "shared_clauses/kinds.rs"]
mod kinds;

#[path = "shared_clauses/payments.rs"]
mod payments;

#[path = "shared_clauses/counts.rs"]
mod counts;

#[path = "shared_clauses/look_where.rs"]
mod look_where;

#[path = "shared_clauses/where_values.rs"]
mod where_values;

#[path = "shared_clauses/token_values.rs"]
mod token_values;

#[path = "shared_clauses/turn_history.rs"]
mod turn_history;

#[path = "shared_clauses/condition_values.rs"]
mod condition_values;

#[path = "shared_clauses/normal_forms.rs"]
mod normal_forms;

#[path = "shared_clauses/verb_agreement.rs"]
mod verb_agreement;

#[path = "shared_clauses/votes.rs"]
mod votes;

#[path = "shared_clauses/stack_actions.rs"]
mod stack_actions;

#[path = "shared_clauses/discard_causes.rs"]
mod discard_causes;

#[path = "shared_clauses/declined_payments.rs"]
mod declined_payments;

#[path = "shared_clauses/planeswalks.rs"]
mod planeswalks;

#[path = "shared_clauses/ordered_zones.rs"]
mod ordered_zones;

#[path = "shared_clauses/top_permissions.rs"]
mod top_permissions;
