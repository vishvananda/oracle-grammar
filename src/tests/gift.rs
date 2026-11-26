//! Gift choices retain recipients, partial predicates and branch-local targets.
use super::*;

fn gift_frame(text: &str) -> Value {
    let value = compositional(text);
    assert_eq!(value.as_array().unwrap().len(), 1);
    assert_eq!(value[0]["kind"], "Gift");
    value
}
fn draws(ability: &Value) -> Vec<Value> {
    let mut found = Vec::new();
    visit(ability, "Draw", &mut found);
    found
}

#[test]
fn gift_allowed_faces_preserve_reviewed_native_meanings() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/gift.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let parsed = engine()
            .parse(name, case["text"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            parsed.readings,
            vec![crate::bind::normal_form(&case["expected"])],
            "{name}"
        );
    }
}

#[test]
fn then_conditions_keep_closed_receipts_and_every_distinct_where_reading() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/then-conditionals.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let parsed = engine()
            .parse(name, case["text"].as_str().unwrap())
            .unwrap();
        let mut expected: Vec<_> = case["normal_expected_readings"]
            .as_array()
            .unwrap()
            .iter()
            .map(crate::bind::normal_form)
            .collect();
        expected.sort_by_cached_key(Value::to_string);
        expected.dedup();
        assert_eq!(parsed.readings.len(), expected.len(), "{name}");
        for expected in expected {
            assert!(parsed.readings.contains(&expected), "{name}");
        }
        for forbidden in case["forbidden_readings"].as_array().into_iter().flatten() {
            assert!(
                !parsed
                    .readings
                    .contains(&crate::bind::normal_form(forbidden)),
                "{name}"
            );
        }
    }
}

#[test]
fn gift_keywords_share_the_defined_effect_and_keep_the_promised_opponent() {
    for text in [
        "Gift a card",
        "Gift a tapped Fish",
        "Gift an Octopus",
        "Gift a Food",
        "Gift a Treasure",
        "Gift an extra turn",
        "Gift a Rhystic Study",
    ] {
        let frame = gift_frame(text);
        validate_abilities(&frame).unwrap();
        assert!(frame[0]["args"]["promised"].is_null());
        let gift = &frame[0]["args"]["gift"][0]["instructions"][0]["args"]["operation"]["args"];
        let recipient = gift.get("players").or_else(|| gift.get("actor")).unwrap();
        assert_eq!(
            recipient,
            &json!({"kind":"One","args":{"kind":"GiftedPlayer"}})
        );
        assert!(!printed(&frame).is_empty());
    }
    let octopus = gift_frame("Gift an Octopus");
    let token =
        &octopus[0]["args"]["gift"][0]["instructions"][0]["args"]["operation"]["args"]["tokens"][0];
    assert_eq!(token["amount"], json!({"kind":"Constant","args":1}));
    assert_eq!(
        token["recipe"]["Defined"]["power_toughness"],
        json!([{"kind":"Constant","args":8},{"kind":"Constant","args":8}])
    );
}

#[test]
fn gift_spell_specializes_both_outcomes_without_a_separate_spell_reading() {
    let frame = gift_frame("Gift a card\nDraw a card. If the gift was promised, draw two cards.");
    let args = &frame[0]["args"];
    assert_eq!(draws(&args["promised"]).len(), 2);
    assert_eq!(draws(&args["unpromised"]).len(), 1);
    assert_eq!(
        draws(&args["gift"])[0]["players"]["args"]["kind"],
        "GiftedPlayer"
    );
    assert_eq!(
        draws(&args["promised"])[0]["players"]["args"]["kind"],
        "AbilityController"
    );
    assert!(!printed(&frame).is_empty());
    let kicker = engine()
        .parse(
            "Context example",
            "Kicker {1}\nDraw a card. If this spell was kicked, draw two cards.",
        )
        .unwrap()
        .readings
        .into_iter()
        .find(|v| v[0]["kind"] == "CostBranch")
        .unwrap();
    assert_eq!(kicker[0]["kind"], "CostBranch");
    assert_eq!(draws(&kicker[0]["args"]["paid"]).len(), 2);
    assert_eq!(draws(&kicker[0]["args"]["unpaid"]).len(), 1);
}

#[test]
fn gift_specialization_keeps_other_predicates_and_negative_promise_meaning() {
    let frame = gift_frame(
        "Gift a card\nIf the gift was promised and you control an artifact, draw two cards.",
    );
    let paid = &frame[0]["args"]["promised"];
    let unpaid = &frame[0]["args"]["unpromised"];
    assert!(paid.to_string().contains("Controls"));
    assert!(!paid.to_string().contains("GiftPromised"));
    assert!(
        unpaid["args"]["body"]["instructions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!printed(&frame).is_empty());
    let frame =
        gift_frame("Gift a card\nDraw a card. If the gift wasn't promised, draw two cards.");
    assert_eq!(draws(&frame[0]["args"]["promised"]).len(), 1);
    assert_eq!(draws(&frame[0]["args"]["unpromised"]).len(), 2);
    assert!(!printed(&frame).is_empty());
}

#[test]
fn gift_disjunction_keeps_the_other_test_only_when_the_gift_is_unpromised() {
    let frame = gift_frame(
        "Gift a card\nIf the gift was promised or you control an artifact, draw two cards.",
    );
    let args = &frame[0]["args"];
    assert_eq!(
        args["promised"]["args"]["body"]["instructions"][0]["kind"],
        "Effect"
    );
    assert_eq!(
        args["unpromised"]["args"]["body"]["instructions"][0]["kind"],
        "If"
    );
    assert!(args["unpromised"].to_string().contains("Controls"));
    assert!(!args["unpromised"].to_string().contains("GiftPromised"));
    validate_abilities(&frame).unwrap();
}

#[test]
fn gift_sentence_boundaries_keep_the_following_draw_outside_the_condition() {
    let frame = gift_frame(
        "Gift a card\nDraw a card. Then if the gift was promised, draw two cards. Draw three cards.",
    );
    let amounts = |ability: &Value| {
        draws(ability)
            .into_iter()
            .map(|draw| draw["amount"]["args"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        amounts(&frame[0]["args"]["promised"]),
        vec![json!(1), json!(2), json!(3)]
    );
    assert_eq!(
        amounts(&frame[0]["args"]["unpromised"]),
        vec![json!(1), json!(3)]
    );
    assert!(!printed(&frame).is_empty());
}

#[test]
fn edited_gift_branch_counts_and_card_restrictions_print_from_current_operands() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/gift.json")).unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Dewdrop Cure")
        .unwrap();
    let mut frame = engine()
        .parse("Dewdrop Cure", case["text"].as_str().unwrap())
        .unwrap()
        .readings
        .remove(0);
    frame[0]["args"]["promised"]["args"]["announcement"]["targets"][0]["count"]["UpTo"]["args"] =
        json!(5);
    let text = printed_types(&frame, &["Sorcery"]);
    assert!(text.contains("five"), "{text}");
    assert_eq!(
        frame[0]["args"]["unpromised"]["args"]["announcement"]["targets"][0]["count"]["UpTo"]["args"],
        2
    );
}

#[test]
fn plural_seek_moves_the_whole_random_selection_without_a_scalar_receipt() {
    let mut frame = compositional("Seek two nonland cards.");
    let steps = &mut frame[0]["args"]["body"]["instructions"];
    assert_eq!(
        steps[0]["args"]["operation"]["args"]["count"]["AsManyAsCan"]["args"],
        2
    );
    assert_eq!(
        steps[1]["args"]["operation"]["args"]["objects"],
        json!({"kind":"Selected","args":steps[0]["args"]["result"]})
    );
    assert!(!steps.to_string().contains("\"index\":0"));
    steps[0]["args"]["operation"]["args"]["count"]["AsManyAsCan"]["args"] = json!(3);
    let text = printed_types(&frame, &["Sorcery"]);
    assert!(text.contains("three"), "{text}");
    assert!(
        engine()
            .parse("Seek Study", "Seek one nonland cards.")
            .is_err()
    );
}

#[test]
fn gifted_targets_exist_only_in_the_resolving_branch_that_uses_them() {
    let frame =
        gift_frame("Gift a card\nDraw a card. If the gift was promised, destroy target creature.");
    assert_eq!(
        frame[0]["args"]["promised"]["args"]["announcement"]["targets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        frame[0]["args"]["unpromised"]["args"]["announcement"]["targets"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!printed(&frame).is_empty());
}

#[test]
fn gift_definition_printing_refuses_unrepresented_stats_and_counts() {
    let mut frame = gift_frame("Gift an Octopus");
    frame[0]["args"]["gift"][0]["instructions"][0]["args"]["operation"]["args"]["tokens"][0]["recipe"]
        ["Defined"]["power_toughness"][0]["args"] = json!(1);
    assert!(
        engine()
            .print_keyed("Context example", &frame, &["Creature".into()], &[])
            .unwrap()
            .is_none()
    );
    assert!(
        engine()
            .parse("Context example", "Gift two Octopus")
            .is_err()
    );
    assert!(engine().parse("Context example", "Gift two cards").is_err());
}
