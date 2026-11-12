//! Observations, draw payments and granted holders retain their own identities.
use super::*;

#[test]
fn eight_complete_cards_print_their_native_meanings() {
    for name in [
        "Aethermage's Touch",
        "Balloon Stand",
        "Gor Muldrak, Amphinologist",
        "Horrid Shadowspinner",
        "Kogla and Yidaro",
        "Nascent Metamorph",
        "On Wings of Gold",
        "Saheeli's Directive",
    ] {
        let case = cases().into_iter().find(|c| c["name"] == name).unwrap();
        let types: Vec<_> = case["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|ty| ty.as_str().unwrap())
            .collect();
        printed_types(&crate::bind::lowered(&case["expected"]), &types);
    }
}

#[test]
fn a_prefix_does_not_replace_the_reveal_or_copy_receipt() {
    let case = cases()
        .into_iter()
        .find(|c| c["name"] == "Nascent Metamorph")
        .unwrap();
    let text = case["text"].as_str().unwrap().replacen(
        "target opponent reveals",
        "draw a card. Target opponent reveals",
        1,
    );
    let mut wanted = case["expected"].clone();
    wanted[0]["args"]["body"]["instructions"].as_array_mut().unwrap().insert(0,
        json!({"kind":"Effect","args":{"result":12345,"operation":{
            "kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}}
        }}}));
    let wanted = canonical_abilities(&wanted).unwrap();
    let readings = engine().parse("Nascent Metamorph", &text).unwrap().readings;
    assert_eq!(readings, vec![wanted]);
}

#[test]
fn copying_and_reordering_print_new_filters_duration_and_order() {
    let mut frame = expected("Nascent Metamorph");
    let body = &mut frame[0]["args"]["body"]["instructions"];
    body[0]["args"]["operation"]["args"]["filter"] = json!({"kind":"CardType","args":"Artifact"});
    let copy = &mut body[1]["args"]["operation"]["args"];
    copy["changes"][0]["args"]["object"]["args"]["objects"]["args"]["filter"] =
        json!({"kind":"CardType","args":"Artifact"});
    copy["duration"] = json!({"UntilNextTurn":{"kind":"AbilityController"}});
    body[2]["args"]["operation"]["args"]["order"] = json!("Chosen");
    let text = printed(&frame).to_lowercase();
    assert!(
        text.contains("artifact") && text.contains("next turn") && text.contains("any order"),
        "{text}"
    );
    assert!(!text.contains("random order"), "{text}");
}

#[test]
fn the_moved_holder_and_the_creating_spell_print_distinct_references() {
    let mut frame = expected("Aethermage's Touch");
    let body = &mut frame[0]["args"]["body"]["instructions"];
    let grant = &mut body[3]["args"]["operation"]["args"];
    assert_eq!(
        grant["affected"]["Objects"],
        json!({"kind":"One","args":{"kind":"Result","args":{"result":2,"phase":"After","index":0}}})
    );
    let returned = &mut grant["changes"][0]["args"]["args"]["body"]["instructions"][0]["args"]["operation"]
        ["args"];
    assert_eq!(
        returned["objects"],
        json!({"kind":"One","args":{"kind":"Source"}})
    );
    returned["destination"] = json!({"OwnersGraveyard":{"counters":[],"states":[]}});
    let text = printed_types(&frame, &["Instant"]).to_lowercase();
    assert!(text.contains("graveyard"), "{text}");
    // The creator's printed name must not replace the granted ability holder.
    assert!(!text.contains("return context example"), "{text}");
    assert!(!text.contains("put context example"), "{text}");
    let wanted = canonical_abilities(&frame).unwrap();
    let mut creator = frame;
    creator[0]["args"]["body"]["instructions"][3]["args"]["operation"]["args"]["changes"][0]["args"]
        ["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"]["args"] =
        json!({"kind":"GrantingObject"});
    let creator = canonical_abilities(&creator).unwrap();
    assert_ne!(creator, wanted);
    let text = printed_types(&creator, &["Instant"]);
    assert!(
        engine()
            .parse("Context example", &text)
            .unwrap()
            .readings
            .contains(&creator)
    );
}

#[test]
fn revealed_selection_and_remainder_print_independent_numeric_operands() {
    let mut frame = expected("Saheeli's Directive");
    let body = &mut frame[1]["args"]["body"]["instructions"];
    body[0]["args"]["operation"]["args"]["objects"]["args"]["count"] =
        json!({"kind":"Constant","args":5});
    let filters = body[1]["args"]["operation"]["args"]["objects"]["args"]["filter"]["args"]
        .as_array_mut()
        .unwrap();
    for filter in filters {
        if filter["kind"] == "Property" {
            filter["args"]["test"]["args"]["value"] = json!({"kind":"Constant","args":3});
        }
    }
    assert_eq!(
        body[3]["args"]["operation"]["args"]["objects"]["args"]["objects"]["args"]["excluding"]["args"],
        json!([{"kind":"Transition","args":{"result":2,"phase":"Before"}}])
    );
    let text = printed_types(&frame, &["Sorcery"]).to_lowercase();
    assert!(text.contains("five") || text.contains('5'), "{text}");
    assert!(text.contains("three") || text.contains('3'), "{text}");
}

#[test]
fn an_optional_draw_names_its_requested_payment_amount() {
    let text = "Target creature gains flying until end of turn. You may draw cards equal to its power. If you do, discard that many cards.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1);
    let body = &readings[0][0]["args"]["body"]["instructions"];
    let pay = &body[1]["args"];
    assert_eq!(
        pay["operation"]["args"]["cost"]["parts"][0]["Draw"]["count"]["args"]["object"],
        json!({"kind":"Target","args":{"slot":0,"index":0}})
    );
    let amount = &body[2]["args"]["then"]["instructions"][0]["args"]["operation"]["args"]["amount"];
    assert_eq!(amount["kind"], "RequestedDrawCount");
    assert_eq!(amount["args"]["payment"]["Resolution"], pay["result"]);
    assert_eq!(amount["args"]["component"], json!([0]));
    let mut frame = expected("Horrid Shadowspinner");
    frame[1]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"][0]
        ["Draw"]["count"]["args"]["property"] = json!("Toughness");
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("toughness"), "{text}");
}

#[test]
fn required_sacrifices_keep_their_choice_count_and_payment_guard() {
    let mut frame = expected("Balloon Stand");
    let body = &mut frame[0]["args"]["body"]["args"]["announcement"]["modes"]["options"][1]["body"]
        ["instructions"];
    let pay_id = body[0]["args"]["result"].clone();
    let pay = &mut body[0]["args"]["operation"]["args"];
    assert_eq!(pay["optional"], false);
    pay["cost"]["parts"][0]["Sacrifice"]["choose"]["Exactly"]["args"] = json!(2);
    pay["cost"]["parts"][0]["Sacrifice"]["objects"]["args"]["filter"] =
        json!({"kind":"Subtype","args":"Food"});
    assert_eq!(
        body[1]["args"]["condition"],
        json!({"kind":"PaymentChosen","args":pay_id})
    );
    let text = printed_types(&frame, &["Artifact"]).to_lowercase();
    assert!(
        text.contains("food") && (text.contains("two") || text.contains('2')),
        "{text}"
    );
}

#[test]
fn tied_minimums_count_the_same_permanent_filter_for_each_player() {
    let mut frame = expected("Gor Muldrak, Amphinologist");
    let players = &mut frame[2]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["players"]
        ["args"];
    let condition = &mut players["condition"]["args"];
    assert_eq!(
        condition["left"]["args"]["player"],
        json!({"kind":"Bound","args":0})
    );
    assert_eq!(condition["test"]["args"]["comparison"], "LessOrEqual");
    assert_eq!(
        condition["test"]["args"]["value"]["args"]["aggregation"],
        "Minimum"
    );
    condition["left"]["args"]["number"]["Permanents"] =
        json!({"kind":"CardType","args":"Artifact"});
    condition["test"]["args"]["value"]["args"]["number"]["Permanents"] =
        json!({"kind":"CardType","args":"Artifact"});
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("fewest artifacts"), "{text}");
}

#[test]
fn inclusive_kind_qualification_does_not_exclude_creature_tokens() {
    let mut frame = expected("On Wings of Gold");
    let filters = frame[0]["args"]["affected"]["Objects"]["args"]["filter"]["args"]
        .as_array_mut()
        .unwrap();
    let union = filters.iter_mut().find(|f| f["kind"] == "AnyOf").unwrap();
    assert!(
        union["args"]
            .as_array()
            .unwrap()
            .contains(&json!({"kind":"Token"}))
    );
    for member in union["args"].as_array_mut().unwrap() {
        if member["kind"] == "Subtype" {
            member["args"] = json!("Goblin");
        }
    }
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("goblin"), "{text}");
}

#[test]
fn whole_input_and_inner_quotation_punctuation_are_required() {
    let case = cases()
        .into_iter()
        .find(|c| c["name"] == "Aethermage's Touch")
        .unwrap();
    let text = case["text"].as_str().unwrap();
    for malformed in [
        text.replace("hand.\"", "hand\""),
        text.replace("It gains \"At the beginning of your end step, return this creature to its owner's hand.\"", "It gains flying"),
        format!("{text} Unrecognized extra effect."),
    ] {
        let result = engine().parse("Aethermage's Touch", &malformed);
        assert!(!matches!(result, Err(Refusal::Limit(_))));
        assert!(result.is_err(), "{malformed}");
    }
}

#[test]
fn the_discarded_card_is_the_actual_post_cost_incarnation() {
    let mut frame = expected("Kogla and Yidaro");
    let activated = &mut frame[1]["args"]["ability"]["args"];
    assert_eq!(activated["cost"]["parts"].as_array().unwrap().len(), 2);
    let body = &mut activated["body"]["instructions"];
    let reference = json!({"kind":"Query","args":{"objects":{"kind":"CostObjects","args":{
        "payment":"Activation","component":[1],"view":"Performed","phase":"After"
    }},"index":0}});
    let branch = &body[1]["args"];
    assert_eq!(branch["condition"]["args"]["object"], reference);
    assert_eq!(
        branch["condition"]["args"]["filter"],
        json!({"kind":"InZone","args":{"Players":{
            "zone":"Graveyard","players":{"kind":"One","args":{"kind":"AbilityController"}}
        }}})
    );
    assert_eq!(
        branch["then"]["instructions"][0]["args"]["operation"]["args"]["objects"],
        json!({"kind":"One","args":reference})
    );
    assert_eq!(
        branch["otherwise"]["instructions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        branch["otherwise"]["instructions"][0]["args"]["operation"]["kind"],
        "Shuffle"
    );
    assert_eq!(body[2]["args"]["operation"]["kind"], "Draw");
    body[2]["args"]["operation"]["args"]["amount"]["args"] = json!(2);
    activated["cost"]["parts"][0]["Mana"][0]["Generic"] = json!(3);
    let text = printed(&frame).to_lowercase();
    assert!(
        text.contains("{3}") && (text.contains("two cards") || text.contains("2 cards")),
        "{text}"
    );
}
