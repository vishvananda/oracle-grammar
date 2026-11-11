//! Compound noun heads, independent conditions and contextual result references.
use super::*;

#[test]
fn five_complete_cards_print_their_native_operands() {
    for name in [
        "Canoptek Spyder",
        "Oft-Nabbed Goat",
        "Scion of Draco",
        "Witness the Future",
        "Labyrinth Raptor",
    ] {
        let case = cases().into_iter().find(|c| c["name"] == name).unwrap();
        let types: Vec<_> = case["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|ty| ty.as_str().unwrap())
            .collect();
        printed_types(&case["expected"], &types);
    }
}

#[test]
fn a_vehicle_does_not_need_to_be_an_artifact_creature() {
    let filter = json!({"kind":"AnyOf","args":[
        {"kind":"AllOf","args":[
            {"kind":"CardType","args":"Artifact"},
            {"kind":"CardType","args":"Creature"}
        ]},
        {"kind":"Subtype","args":"Vehicle"}
    ]});
    for text in [
        "Destroy target artifact creature or Vehicle.",
        "Destroy all artifact creatures or Vehicles.",
    ] {
        let readings = engine().parse("Context example", text).unwrap().readings;
        assert_eq!(readings.len(), 1, "{text}");
        let frame = &readings[0];
        let actual = if text.contains("target") {
            &frame[0]["args"]["announcement"]["targets"][0]["filter"]
        } else {
            &frame[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"]["args"]
                ["filter"]
        };
        // Canonicalize the complete expected program, not just the union order.
        let mut wanted = frame.clone();
        if text.contains("target") {
            wanted[0]["args"]["announcement"]["targets"][0]["filter"] = filter.clone();
        } else {
            wanted[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"]
                ["args"]["filter"] = filter.clone();
        }
        assert!(!actual.is_null());
        assert_eq!(frame, &canonical_abilities(&wanted).unwrap(), "{text}");
    }
}

#[test]
fn outer_qualities_and_controller_apply_to_both_compound_heads() {
    let frame = expected("Canoptek Spyder");
    let filter = &frame[1]["args"]["event"]["Player"]["objects"];
    let clauses = filter["args"].as_array().unwrap();
    assert_eq!(filter["kind"], "AllOf");
    for clause in [
        json!({"kind":"Not","args":{"kind":"Token"}}),
        json!({"kind":"Not","args":{"kind":"SameObject","args":{"kind":"Source"}}}),
        json!({"kind":"ControlledBy","args":{"kind":"One","args":{"kind":"AbilityController"}}}),
    ] {
        assert!(clauses.contains(&clause));
    }
    assert_eq!(clauses.iter().filter(|c| c["kind"] == "AnyOf").count(), 1);
}

#[test]
fn each_color_grant_is_independent_and_prints_edited_operands() {
    let mut frame = expected("Scion of Draco");
    let white = &frame[2]["args"]["affected"]["Objects"]["args"]["filter"]["args"];
    let blue = &frame[3]["args"]["affected"]["Objects"]["args"]["filter"]["args"];
    assert!(
        white
            .as_array()
            .unwrap()
            .contains(&json!({"kind":"Color","args":"White"}))
    );
    assert!(
        blue.as_array()
            .unwrap()
            .contains(&json!({"kind":"Color","args":"Blue"}))
    );
    // Neither arm contains an else branch excluding creatures of another color.
    let mut exclusions = Vec::new();
    visit(
        &json!(&frame.as_array().unwrap()[2..]),
        "Not",
        &mut exclusions,
    );
    assert!(exclusions.is_empty());
    for clause in frame[2]["args"]["affected"]["Objects"]["args"]["filter"]["args"]
        .as_array_mut()
        .unwrap()
    {
        if clause["kind"] == "Color" {
            clause["args"] = json!("Green");
        }
    }
    frame[2]["args"]["changes"][0]["args"] = json!({"kind":"Reach"});
    let text = printed_types(&frame, &["Artifact", "Creature"]).to_lowercase();
    assert!(text.contains("reach") && text.contains("green"), "{text}");
    assert!(
        !engine()
            .parse(
                "Scion of Draco",
                cases()
                    .into_iter()
                    .find(|c| c["name"] == "Scion of Draco")
                    .unwrap()["text"]
                    .as_str()
                    .unwrap()
            )
            .unwrap()
            .readings
            .contains(&canonical_abilities(&frame).unwrap())
    );
}

#[test]
fn departed_counter_amount_and_other_players_follow_the_owner() {
    let mut frame = expected("Oft-Nabbed Goat");
    let trigger = &mut frame[1]["args"];
    let counter_filter = &mut trigger["intervening_if"]["args"]["filter"]["args"];
    counter_filter["kind"] = json!({"Named":"OilCounter"});
    counter_filter["value"]["args"] = json!(2);
    let instructions = trigger["body"]["instructions"].as_array_mut().unwrap();
    for instruction in instructions {
        let amount = &mut instruction["args"]["operation"]["args"]["amount"];
        assert_eq!(amount["kind"], "CountCounters");
        assert_eq!(amount["args"]["information"], "CurrentOrLastKnown");
        assert_eq!(
            amount["args"]["recipients"]["args"]["args"],
            json!({"kind":"Source"})
        );
        amount["args"]["kind"] = json!({"Named":"OilCounter"});
    }
    let text = printed(&frame).to_lowercase();
    assert!(
        text.contains("oil") && (text.contains("two") || text.contains('2')),
        "{text}"
    );
    let ops = &frame[1]["args"]["body"]["instructions"];
    assert_eq!(
        ops[0]["args"]["operation"]["args"]["players"],
        json!({"kind":"One","args":{"kind":"OwnerOf","args":{"kind":"Source"}}})
    );
    assert_eq!(
        ops[1]["args"]["operation"]["args"]["players"],
        json!({"kind":"OthersThan","args":{"kind":"OwnerOf","args":{"kind":"Source"}}})
    );
    assert_eq!(
        ops[0]["args"]["operation"]["args"]["amount"],
        ops[1]["args"]["operation"]["args"]["amount"]
    );
}

#[test]
fn a_granted_counter_echo_keeps_holder_and_named_creator_distinct() {
    for (subject, reference) in [
        ("this creature", "Source"),
        ("Context example", "GrantingObject"),
    ] {
        let text = format!(
            "Target creature gains \"When {subject} dies, if it had two or more oil counters on it, its owner draws that many cards and each other player loses that much life.\" until end of turn."
        );
        let frames = engine().parse("Context example", &text).unwrap().readings;
        assert!(!frames.is_empty(), "{text}");
        for frame in frames {
            let mut triggers = Vec::new();
            visit(&frame, "Triggered", &mut triggers);
            assert_eq!(triggers.len(), 1);
            let trigger = &triggers[0];
            assert_eq!(
                trigger["event"]["ZoneChange"]["objects"]["args"],
                json!({"kind":reference})
            );
            let mut owners = Vec::new();
            visit(trigger, "OwnerOf", &mut owners);
            assert_eq!(owners, vec![json!({"kind":reference}); 2]);
        }
    }
}

#[test]
fn explicit_you_restarts_the_actor_after_a_target_player() {
    let mut frame = expected("Witness the Future");
    let instructions = &mut frame[0]["args"]["body"]["instructions"];
    let look = &mut instructions[1]["args"]["operation"]["args"];
    assert_eq!(look["player"], json!({"kind":"AbilityController"}));
    look["objects"]["args"]["count"]["args"] = json!(5);
    instructions[2]["args"]["operation"]["args"]["count"]["AsManyAsCan"]["args"] = json!(2);
    assert_eq!(
        instructions[2]["args"]["operation"]["args"]["objects"]["args"]["objects"],
        json!({"kind":"Observed","args":0})
    );
    assert_eq!(
        instructions[4]["args"]["operation"]["args"]["objects"]["args"]["objects"]["args"]["excluding"]
            ["args"][0],
        json!({"kind":"Transition","args":{"result":2,"phase":"Before"}})
    );
    let text = printed_types(&frame, &["Sorcery"]).to_lowercase();
    assert!(text.contains("five") || text.contains('5'), "{text}");
    assert!(text.contains("two") || text.contains('2'), "{text}");
}

#[test]
fn the_defender_chooses_a_blocker_of_the_triggering_attacker() {
    let mut frame = expected("Labyrinth Raptor");
    let sacrifice = &mut frame[1]["args"]["body"]["instructions"][0]["args"]["operation"]["args"];
    assert_eq!(
        sacrifice["players"],
        json!({"kind":"One","args":{"kind":"DefendingPlayerOf","args":{"kind":"EventSubject"}}})
    );
    assert!(
        sacrifice["objects"]["args"]["filter"]["args"]
            .as_array()
            .unwrap()
            .contains(&json!({"kind":"BlockingObject","args":{"kind":"EventSubject"}}))
    );
    sacrifice["choose"]["AsManyAsCan"]["args"] = json!(2);
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("two") || text.contains('2'), "{text}");
    let result = engine().parse("Context example", "Whenever a creature you control with menace becomes blocked, defending player sacrifices a creature of their choice that's blocking it. Unrecognized trailing effect.");
    assert!(!matches!(result, Err(Refusal::Limit(_))));
    assert!(result.is_err());
}
