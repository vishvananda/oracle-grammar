//! Defining words retain the holder, grant and conditional scope (CR604.3a).
use super::*;

fn parsed(text: &str) -> Value {
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{text}: {readings:#?}");
    readings[0].clone()
}

fn definition(frame: &Value) -> Value {
    let mut statics = Vec::new();
    visit(frame, "Static", &mut statics);
    assert_eq!(statics.len(), 1, "{frame:#?}");
    statics.remove(0)
}

#[test]
fn chosen_player_definitions_match_complete_native_cards() {
    for name in ["Entropic Specter", "Haunting Apparition", "Sewer Nemesis"] {
        let case = cases().into_iter().find(|c| c["name"] == name).unwrap();
        let expected = canonical_abilities(&case["expected"]).unwrap();
        let readings = engine()
            .parse(name, case["text"].as_str().unwrap())
            .unwrap()
            .readings;
        assert_eq!(
            readings.as_slice(),
            std::slice::from_ref(&expected),
            "{name}"
        );
        printed(&expected);
    }
}

#[test]
fn granted_definitions_keep_creator_and_holder_distinct() {
    for (subject, object) in [
        ("This creature's", "Source"),
        ("Context example's", "GrantingObject"),
    ] {
        let text = format!(
            "Target creature gains \"{subject} power is equal to seven.\" until end of turn."
        );
        let frame = parsed(&text);
        let static_ = definition(&frame);
        assert_eq!(static_["kind"], "Ordinary");
        assert_eq!(
            static_["affected"],
            json!({"Objects":{"kind":"One","args":{"kind":object}}})
        );
        assert_eq!(
            static_["changes"],
            json!([{"kind":"SetPower","args":{"kind":"Constant","args":7}}])
        );
        printed_types(&frame, &["Instant"]);
    }
}

#[test]
fn conditional_definition_is_ordinary_but_intrinsic_definition_is_cda() {
    let frame = parsed("This creature's power is equal to seven.");
    assert!(
        definition(&frame)["kind"]
            .get("CharacteristicDefining")
            .is_some()
    );
    let frame = parsed("During your turn, this creature's power is equal to seven.");
    assert_eq!(definition(&frame)["kind"], "Ordinary");
    printed(&frame);
}

#[test]
fn edited_definition_prints_new_amount_and_player() {
    let mut frame = parsed(
        "This creature's power is equal to the number of cards in the chosen player's hand.",
    );
    let count = &mut frame[0]["args"]["changes"][0]["args"];
    count["args"]["args"]["scope"]["Players"]["players"] =
        json!({"kind":"One","args":{"kind":"AbilityController"}});
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("your hand"), "{text}");
    frame[0]["args"]["changes"][0]["args"] = json!({"kind":"Constant","args":9});
    let text = printed(&frame).to_lowercase();
    assert!(text.contains("nine") || text.contains('9'), "{text}");
    assert!(!text.contains("chosen player"), "{text}");
}

#[test]
fn entry_subtypes_are_applied_on_arrival_and_becomes_is_later() {
    for (word, arrival, instructions) in [("is", true, 1), ("becomes", false, 2)] {
        let frame = parsed(&format!(
            "Put target artifact card from a graveyard onto the battlefield under your control. It {word} an Elf in addition to its other types."
        ));
        let body = frame[0]["args"]["body"]["instructions"].as_array().unwrap();
        assert_eq!(body.len(), instructions);
        let states = &body[0]["args"]["operation"]["args"]["destination"]["Battlefield"]["arrival"]
            ["states"];
        assert_eq!(!states.as_array().unwrap().is_empty(), arrival);
        if !arrival {
            assert_eq!(
                body[1]["args"]["operation"]["kind"],
                "CreateContinuousEffect"
            );
            assert_eq!(
                body[1]["args"]["operation"]["args"]["affected"]["Objects"]["args"]["kind"],
                "Result"
            );
        }
        printed_types(&frame, &["Sorcery"]);
    }
}

#[test]
fn colors_and_types_that_a_moved_creature_is_join_its_arrival() {
    let black = json!({"kind":"Colors","args":{"mode":"Add","colors":["Black"]}});
    let zombie =
        json!({"kind":"CreatureTypes","args":{"mode":"Add","types":{"Listed":["Zombie"]}}});
    let set_black = json!({"kind":"Colors","args":{"mode":"Set","colors":["Black"]}});
    let nightmare =
        json!({"kind":"CreatureTypes","args":{"mode":"Add","types":{"Listed":["Nightmare"]}}});
    for (text, changes) in [
        (
            "Return target creature card from your graveyard to the battlefield. That creature is a black Zombie in addition to its other colors and types.",
            json!([black, zombie]),
        ),
        (
            "Put target creature card from a graveyard onto the battlefield under your control. That creature is black and is a Nightmare in addition to its other creature types.",
            json!([set_black, nightmare]),
        ),
    ] {
        let frame = parsed(text);
        let body = frame[0]["args"]["body"]["instructions"].as_array().unwrap();
        assert_eq!(body.len(), 1, "{text}");
        let arrival =
            &body[0]["args"]["operation"]["args"]["destination"]["Battlefield"]["arrival"];
        assert_eq!(arrival["states"], json!([{"Changes": changes}]), "{text}");
        printed_types(&frame, &["Sorcery"]);
    }
    // Only a move onto the battlefield has an arrival these characteristics can join.
    let exiled = "Exile target creature card from a graveyard. That creature is a Zombie in addition to its other types.";
    for reading in engine()
        .parse("Context example", exiled)
        .map_or_else(|_| Vec::new(), |p| p.readings)
    {
        assert!(!reading.to_string().contains("\"Changes\""), "{reading}");
    }
}

#[test]
fn every_creature_type_defines_only_its_own_holder() {
    let frame = parsed("This creature is every creature type.");
    let static_ = definition(&frame);
    assert!(static_["kind"].get("CharacteristicDefining").is_some());
    assert_eq!(
        static_["changes"],
        json!([{"kind":"CreatureTypes","args":{"mode":"Set","types":"All"}}])
    );
    printed(&frame);
    let frame = parsed("Equipped creature is every creature type.");
    assert_eq!(definition(&frame)["kind"], "Ordinary");
    printed(&frame);
}
