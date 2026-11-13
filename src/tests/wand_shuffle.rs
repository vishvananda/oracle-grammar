//! Positive reveal branches and player-specific movement receipts.
use super::*;

fn cases() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!(
        "../../tests/fixtures/wand-shuffle-corrections.json"
    ))
    .unwrap()["cases"]
        .as_array()
        .unwrap()
        .clone()
}
fn frame(name: &str) -> Value {
    canonical_abilities(&cases().into_iter().find(|c| c["name"] == name).unwrap()["expected"])
        .unwrap()
}
fn body(value: &mut Value, ability: usize) -> &mut Vec<Value> {
    value[ability]["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
}

#[test]
fn reported_shuffle_and_wand_faces_and_controls_have_only_the_correct_reading() {
    for case in cases() {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        validate_abilities(&case["expected"]).unwrap();
        let expected = canonical_abilities(&case["expected"]).unwrap();
        assert_eq!(
            engine().parse(name, text).unwrap().readings,
            vec![expected.clone()],
            "{name}"
        );
        let trail = engine().trail_of(name, &expected, text).unwrap().unwrap();
        assert_eq!(
            engine().print_trail(name, &expected, &trail).unwrap(),
            Some(text.to_string()),
            "{name}"
        );
        let types: Vec<_> = case["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(
            engine()
                .print_keyed(name, &expected, &types, &[])
                .unwrap()
                .is_some(),
            "{name}"
        );
    }
}

#[test]
fn all_shuffles_precede_draws_and_each_draw_reads_its_own_players_receipt() {
    for (name, ability) in [
        ("Winds of Change", 0),
        ("Molten Psyche", 0),
        ("Whirlpool Warrior", 1),
    ] {
        let mut reading = frame(name);
        let instructions = body(&mut reading, ability);
        assert_eq!(instructions[0]["kind"], "ForEachPlayer");
        assert_eq!(instructions[1]["kind"], "ForEachPlayer");
        let shuffle = &instructions[0]["args"];
        let drawing = &instructions[1]["args"];
        assert_eq!(shuffle["players"], json!({"kind":"All"}));
        assert_eq!(drawing["players"], shuffle["players"]);
        let movement = &shuffle["body"]["instructions"][0]["args"];
        assert_eq!(movement["operation"]["kind"], "Move");
        assert_eq!(
            movement["operation"]["args"]["destination"]["Library"]["position"],
            "Shuffled"
        );
        assert_eq!(shuffle["body"]["instructions"].as_array().unwrap().len(), 1);
        let draw = &drawing["body"]["instructions"][0]["args"]["operation"];
        assert_eq!(draw["kind"], "Draw");
        let player = json!({"kind":"Bound","args":drawing["binding"]});
        assert_eq!(draw["args"]["players"], json!({"kind":"One","args":player}));
        assert_eq!(
            draw["args"]["amount"],
            json!({"kind":"CountObjects","args":{"kind":"MovedBy","args":{"result":movement["result"],"phase":"Before","player":player}}})
        );
    }
    let text = "Each player shuffles the cards from their hand into their library, then draws that many cards.";
    assert_eq!(compositional(text), frame("Winds of Change"));
}

#[test]
fn edited_shuffle_population_and_prefix_print_from_current_receipts() {
    let mut reading = frame("Winds of Change");
    let instructions = body(&mut reading, 0);
    for instruction in instructions.iter_mut() {
        instruction["args"]["players"] =
            json!({"kind":"OpponentsOf","args":{"kind":"AbilityController"}});
    }
    instructions.insert(0, json!({"kind":"Effect","args":{"result":19,"operation":{"kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":4}}}}}));
    let text = printed(&reading);
    assert!(text.contains("opponent"), "{text}");
    assert!(text.contains('4') || text.contains("four"), "{text}");
}

#[test]
fn wand_counts_observed_lands_and_nonlands_without_negating_existence() {
    let mut reading = frame("Wand of Ith");
    let instructions = body(&mut reading, 0);
    let reveal = instructions[0]["args"]["result"].clone();
    assert_eq!(
        instructions[0]["args"]["operation"]["args"]["at_random"],
        true
    );
    for (i, filter) in [
        (1, json!({"kind":"CardType","args":"Land"})),
        (
            2,
            json!({"kind":"Not","args":{"kind":"CardType","args":"Land"}}),
        ),
    ] {
        let condition = &instructions[i]["args"]["condition"];
        assert_eq!(condition["kind"], "Compare");
        assert_eq!(
            condition["args"]["test"],
            json!({"kind":"Compare","args":{"comparison":"GreaterOrEqual","value":{"kind":"Constant","args":1}}})
        );
        assert_eq!(
            condition["args"]["left"],
            json!({"kind":"CountObjects","args":{"kind":"Matching","args":{"objects":{"kind":"Observed","args":reveal},"filter":filter}}})
        );
        let payment = &instructions[i]["args"]["then"]["instructions"][0]["args"]["operation"];
        assert_eq!(payment["kind"], "PayCost");
        assert_eq!(payment["args"]["optional"], true);
        assert_eq!(payment["args"]["player"]["kind"], "Target");
    }
    reading[0]["args"]["cost"]["parts"][0]["Mana"][0]["Generic"] = json!(5);
    assert!(printed(&reading).contains("{5}"));
}

#[test]
fn ordinary_receipts_and_a_distinct_mill_recipient_keep_their_performer() {
    for name in ["Credit Voucher", "Whirlpool Rider"] {
        let mut draws = Vec::new();
        visit(&frame(name), "Draw", &mut draws);
        assert_eq!(draws.len(), 1);
        assert_eq!(draws[0]["amount"]["args"]["kind"], "Transition");
    }
    let reading = frame("Psychic Spiral");
    let mut mills = Vec::new();
    visit(&reading, "Mill", &mut mills);
    assert_eq!(mills.len(), 1);
    assert_eq!(mills[0]["players"]["args"]["kind"], "Target");
    assert_eq!(mills[0]["amount"]["args"]["kind"], "Transition");
    let mut per_player = Vec::new();
    visit(&reading, "MovedBy", &mut per_player);
    assert!(per_player.is_empty());
}
