//! Shared phrase composition, complete edited printing and independent roles.
use super::*;

#[test]
fn either_action_prints_new_discard_payment_and_both_target_filters() {
    let mut frame = expected("Anthropede");
    let options = &mut frame[1]["args"]["body"]["instructions"][0]["args"]["options"];
    options[0]["instructions"][0]["args"]["operation"]["args"]["amount"] =
        json!({"kind":"Constant","args":2});
    options[1]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"] =
        json!([{"Mana":[{"Generic":3}]}]);
    for option in options.as_array_mut().unwrap().iter_mut().take(2) {
        option["instructions"][1]["args"]["operation"]["args"]["announcement"]["targets"][0]["filter"] =
            json!({"kind":"CardType","args":"Artifact"});
    }
    let text = printed(&frame).to_lowercase();
    assert!(
        text.contains("discard two cards") || text.contains("discard 2 cards"),
        "{text}"
    );
    assert!(
        text.contains("{3}") && text.contains("target artifact"),
        "{text}"
    );
}

#[test]
fn either_action_receipts_do_not_capture_an_unrelated_preceding_draw() {
    let mut frame = expected("Anthropede");
    frame[1]["args"]["body"]["instructions"].as_array_mut().unwrap().insert(0,
        json!({"kind":"Effect","args":{"result":999u32,"operation":{"kind":"Draw","args":{
            "players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}
        }}}}));
    let case = cases()
        .into_iter()
        .find(|c| c["name"] == "Anthropede")
        .unwrap();
    let text = case["text"]
        .as_str()
        .unwrap()
        .replace("enters, you may", "enters, draw a card. You may");
    let parsed = engine().parse("Anthropede", &text).unwrap();
    assert_eq!(parsed.readings, vec![canonical_abilities(&frame).unwrap()]);
}

#[test]
fn declined_payment_prints_the_new_payer_domain_and_draw_quantity() {
    let mut frame = expected("Gravestorm");
    frame[0]["args"]["announcement"]["targets"][0]["players"] = json!({"kind":"All"});
    frame[0]["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0]["args"]["body"]
        ["instructions"][0]["args"]["operation"]["args"]["amount"] =
        json!({"kind":"Constant","args":2});
    let text = printed_types(&frame, &["Enchantment"]).to_lowercase();
    assert!(text.contains("target player"), "{text}");
    assert!(
        text.contains("two cards") || text.contains("2 cards"),
        "{text}"
    );
}

#[test]
fn returning_an_artifact_still_boosts_the_equipped_attacker() {
    let mut frame = expected("Slab Hammer");
    frame[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"][0]
        ["Move"]["selections"][0]["objects"]["args"]["filter"]["args"][0]["args"] =
        json!("Artifact");
    let pump = &mut frame[0]["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0]
        ["args"]["operation"]["args"]["changes"][0]["args"];
    pump["power"] = json!({"kind":"Constant","args":3});
    pump["toughness"] = json!({"kind":"Constant","args":1});
    let text = printed_types(&frame, &["Artifact"]).to_lowercase();
    assert!(
        text.contains("artifact you control") && text.contains("+3/+1"),
        "{text}"
    );
}

#[test]
fn independent_offers_print_new_grant_and_counter_without_nesting() {
    let mut frame = expected("Talus Paladin");
    let body = &mut frame[0]["args"]["body"]["instructions"];
    assert_eq!(body.as_array().unwrap().len(), 2);
    body[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"][0]["args"] =
        json!({"kind":"Vigilance"});
    let placement = &mut body[1]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["placements"]
        [0];
    placement["kind"] = json!({"Named":"ChargeCounter"});
    placement["amount"] = json!({"kind":"Constant","args":2});
    let text = printed(&frame).to_lowercase();
    assert!(
        text.contains("you may have") && text.contains("vigilance") && text.contains("charge"),
        "{text}"
    );
}

#[test]
fn an_explicit_second_offer_is_independent_with_or_without_a_comma() {
    let case = cases()
        .into_iter()
        .find(|c| c["name"] == "Talus Paladin")
        .unwrap();
    for text in [
        case["text"].as_str().unwrap().to_string(),
        case["text"]
            .as_str()
            .unwrap()
            .replace(", and you may", " and you may"),
    ] {
        let readings = engine().parse("Talus Paladin", &text).unwrap().readings;
        assert_eq!(
            readings,
            vec![canonical_abilities(&case["expected"]).unwrap()]
        );
    }
}

#[test]
fn unattach_prints_the_edited_aura_instead_of_its_equipment_host() {
    let mut frame = expected("Tamiyo's Compleation");
    frame[2]["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0]["args"]["operation"]
        ["args"]["action"]["Unattach"]["object"] = json!({"kind":"Source"});
    let text = printed_types(&frame, &["Enchantment"]).to_lowercase();
    assert!(text.contains("unattach"), "{text}");
}

#[test]
fn granted_unattach_keeps_the_named_creator_distinct_from_the_holder() {
    let text = r#"Target creature gains "Whenever this creature attacks, unattach Context example." until end of turn."#;
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert!(!readings.is_empty());
    for frame in readings {
        let mut actions = Vec::new();
        visit(&frame, "Keyword", &mut actions);
        let unattach: Vec<_> = actions
            .iter()
            .filter(|a| a["action"].get("Unattach").is_some())
            .collect();
        assert_eq!(unattach.len(), 1);
        assert_eq!(
            unattach[0]["actor"],
            json!({"kind":"One","args":{"kind":"AbilityController"}})
        );
        assert_eq!(
            unattach[0]["action"]["Unattach"]["object"],
            json!({"kind":"GrantingObject"})
        );
    }
}

#[test]
fn copied_target_and_counter_source_remain_independent_when_printed() {
    let mut frame = expected("Furnace-Blessed Conqueror");
    frame[0]["args"]["announcement"]["targets"] = json!([{
        "slot":0,"kind":{"Object":{"kind":"Permanent","zones":["Battlefield"]}},
        "count":{"Exactly":{"kind":"Constant","args":1}},
        "filter":{"kind":"CardType","args":"Creature"},"players":null,
        "distribute":null,"constraints":[],"chooser":null
    }]);
    let instructions = &mut frame[0]["args"]["body"]["instructions"];
    instructions[0]["args"]["operation"]["args"]["tokens"][0]["recipe"]["Copy"]["object"] =
        json!({"kind":"Target","args":{"slot":0,"index":0}});
    let placement = &mut instructions[1]["args"]["operation"]["args"]["placements"][0];
    placement["kind"] = json!({"Named":"ChargeCounter"});
    placement["amount"]["args"]["kind"] = json!({"Named":"QuestCounter"});
    let text = printed(&frame).to_lowercase();
    assert!(
        text.contains("target creature") && text.contains("charge") && text.contains("quest"),
        "{text}"
    );
}

#[test]
fn a_wrapped_damage_event_names_its_source_as_the_event_subject() {
    for text in [
        "Whenever a creature deals damage to one or more players for the first time each turn, put a +1/+1 counter on it.",
        "Whenever a creature deals combat damage to a player for the first time each turn, put a +1/+1 counter on it.",
        "Whenever a creature deals combat damage to a player, put a +1/+1 counter on it.",
    ] {
        let parsed = engine().parse("Context example", text).unwrap();
        assert!(!parsed.readings.is_empty(), "{text}");
        for reading in &parsed.readings {
            let mut recipients = Vec::new();
            visit(reading, "Objects", &mut recipients);
            assert_eq!(
                recipients,
                vec![json!({"kind":"One","args":{"kind":"EventSubject"}})],
                "{text}"
            );
        }
    }
}

#[test]
fn a_declined_draw_replacement_keeps_the_draw() {
    let text = "If you would draw a card, you may put a study counter on this enchantment instead.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{readings:#?}");
    let mut optional = Vec::new();
    visit(&readings[0], "Optional", &mut optional);
    assert!(optional.is_empty(), "{readings:#?}");
    let mut choices = Vec::new();
    visit(&readings[0], "ChooseAction", &mut choices);
    let options = choices[0]["options"].as_array().unwrap();
    assert_eq!(options.len(), 2);
    let draw = &options[1]["args"]["instructions"][1]["args"]["operation"];
    assert_eq!(draw["kind"], "Draw");
    printed_types(&readings[0], &["Enchantment"]);
}

#[test]
fn a_return_from_two_zones_is_one_ability_per_zone() {
    let text =
        "{2}{G}: Return this card from your graveyard or from exile to the battlefield tapped.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{readings:#?}");
    let abilities = readings[0].as_array().unwrap();
    assert_eq!(abilities.len(), 2);
    assert_eq!(abilities[0]["args"]["zones"], json!(["Graveyard"]));
    assert_eq!(abilities[1]["args"]["zones"], json!(["Exile"]));
    let moved = &abilities[1]["args"]["ability"]["args"]["body"]["instructions"][0]["args"]["operation"]
        ["args"]["objects"];
    assert_eq!(moved["kind"], "InZone");
    printed(&readings[0]);
}

#[test]
fn an_infinity_ability_exists_only_while_harnessed() {
    let text = "∞ — At the beginning of your upkeep, return target creature card from your graveyard to the battlefield.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{readings:#?}");
    let ability = &readings[0][0];
    assert_eq!(ability["kind"], "Conditional");
    let then = &ability["args"]["then"][0]["args"]["event"];
    assert!(then.get("Turn").is_some(), "{then:#?}");
    printed_types(&readings[0], &["Artifact"]);
}

#[test]
fn an_optional_reveal_before_a_reflexive_trigger_is_an_action() {
    let text = "You may reveal a Giant card from your hand. When you do, this enchantment deals 2 damage to any target.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    let actions = readings
        .iter()
        .find(|reading| {
            let mut found = Vec::new();
            visit(reading, "Reveal", &mut found);
            found
                .iter()
                .any(|reveal| reveal["objects"]["kind"] == "Selected")
        })
        .expect("an optional reveal action");
    let mut optional = Vec::new();
    visit(actions, "Optional", &mut optional);
    assert_eq!(optional.len(), 1);
    printed_types(actions, &["Enchantment"]);
}

#[test]
fn an_activation_restriction_names_starting_life_before_its_margin() {
    for (text, first) in [
        (
            "{1}: Draw a card. Activate only if you have at least 10 life more than your starting life total.",
            "StartingLife",
        ),
        (
            "At the beginning of your upkeep, if you have at least 15 life more than your starting life total, draw a card.",
            "Constant",
        ),
    ] {
        let readings = engine().parse("Context example", text).unwrap().readings;
        assert_eq!(readings.len(), 1, "{text}: {readings:#?}");
        let mut sums = Vec::new();
        visit(&readings[0], "Add", &mut sums);
        assert_eq!(sums[0][0]["kind"], first, "{text}");
        printed(&readings[0]);
    }
}

#[test]
fn a_perpetual_grant_on_a_creature_that_died_names_its_graveyard_card() {
    let text = "Until end of turn, whenever a nontoken creature dies, it perpetually gains unearth {5}.\nConsumed by History deals 3 damage to each creature.";
    let readings = engine()
        .parse("Consumed by History", text)
        .unwrap()
        .readings;
    assert_eq!(readings.len(), 1, "{readings:#?}");
    let mut perpetual = Vec::new();
    visit(&readings[0], "CreatePerpetualEffect", &mut perpetual);
    assert_eq!(
        perpetual[0]["objects"]["args"]["args"]["objects"]["args"]["objects"],
        json!({"kind":"One","args":{"kind":"EventDestination"}})
    );
}

#[test]
fn it_after_a_created_trigger_still_names_its_event_object() {
    let text = "Whenever a creature you control attacks alone this turn, put three +1/+1 counters on it. It gains trample, lifelink, and indestructible until end of turn.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    let inner = readings
        .iter()
        .find(|reading| {
            let mut created = Vec::new();
            visit(reading, "CreateTrigger", &mut created);
            created.first().is_some_and(|created| {
                created["body"]["instructions"].as_array().unwrap().len() == 2
            })
        })
        .unwrap_or_else(|| panic!("{readings:#?}"));
    let mut effects = Vec::new();
    visit(inner, "CreateContinuousEffect", &mut effects);
    assert_eq!(
        effects[0]["affected"]["Objects"]["args"],
        json!({"kind":"EventSubject"})
    );
    printed_types(inner, &["Sorcery"]);
}

#[test]
fn the_chosen_type_is_the_kind_of_type_the_card_chose() {
    for (text, kind) in [
        (
            "As this artifact enters, choose a creature type.\nSpells you cast of the chosen type cost {1} less to cast.",
            "CreatureType",
        ),
        (
            "As this artifact enters, choose a card type.\nSpells you cast of the chosen type cost {1} less to cast.",
            "CardType",
        ),
        (
            "As this enchantment enters, choose a planeswalker type.\nPlaneswalkers you control of the chosen type have hexproof.",
            "PlaneswalkerType",
        ),
    ] {
        let readings = engine().parse("Context example", text).unwrap().readings;
        assert_eq!(readings.len(), 1, "{text}: {readings:#?}");
        let mut chosen = Vec::new();
        visit(&readings[0], "Chosen", &mut chosen);
        assert!(
            !chosen.is_empty() && chosen.iter().all(|used| used == kind),
            "{text}"
        );
    }
}
