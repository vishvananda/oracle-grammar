use crate::engine::PrintedPowerToughness;
use crate::model::ir::Characteristics;
use crate::model::mtg_types::{CardType, SubType};
use crate::test_support::engine;

#[test]
fn station_trail_keeps_printed_box_across_written_views() {
    let engine = engine();
    let name = "K't'inga-Class Battle Cruiser";
    let text = concat!(
        "Whenever this Spacecraft enters or attacks, one or two target Klingons and/or Warriors you control gain flying until end of turn.\n",
        "Station (Tap another creature you control: Put charge counters equal to its power on this Spacecraft. Station only as a sorcery. It's an artifact creature at 4+.)\n",
        "4+ | Flying"
    );
    let characteristics = Characteristics {
        card_types: vec![CardType::Artifact],
        subtypes: vec![SubType::Spacecraft],
        supertypes: Vec::new(),
        mana_cost: None,
        color_indicator: Vec::new(),
        power_toughness: None,
        loyalty: None,
        defense: None,
    };
    let source = engine
        .card_input_context(
            name,
            text,
            &characteristics,
            PrintedPowerToughness::Known {
                power: 5,
                toughness: 5,
            },
        )
        .unwrap();
    let parsed = engine.parse_card_with_source(name, text, &source).unwrap();
    let [reading] = parsed.readings.as_slice() else {
        panic!("expected one reading");
    };
    let key = engine
        .card_key_of_with_source(name, reading, &source, text, true)
        .unwrap()
        .expect("a derivation for the original Station source text");
    assert_eq!(
        engine
            .print_card_trail_with_source(name, reading, &source, &key)
            .unwrap()
            .as_deref(),
        Some(text)
    );
}

#[test]
fn battlefront_reads_attacked_opponents_from_history() {
    let engine = engine();
    let name = "Korok, Liberated Rage";
    let text = "Battlefront — Whenever Korok attacks, creatures you control get +X/+X until end of turn, where X is the number of opponents you attacked this turn.";
    let parsed = engine.parse(name, text).unwrap();
    let [reading] = parsed.readings.as_slice() else {
        panic!("expected one reading");
    };
    let pump = &reading[0]["args"]["ability"]["args"]["body"]["instructions"][0]["args"]["operation"]
        ["args"]["changes"][0]["args"];
    assert_eq!(pump["power"], pump["toughness"]);
    assert_eq!(pump["power"]["kind"], "CountPlayers");
    assert_eq!(pump["power"]["args"]["kind"], "OpponentsAttackedBy");
    assert_eq!(pump["power"]["args"]["args"]["window"], "ThisTurn");
}

#[test]
fn copy_exception_keeps_creature_subtype_alongside_artifact_type() {
    let engine = engine();
    let name = "Reg Barclay, Fidgety Fantasist";
    let text = concat!(
        "Seek out — Whenever you draw your second card each turn, ",
        "create a token that's a copy of another target creature you control, ",
        "except it isn't legendary, it's an Illusion artifact in addition to its other types, ",
        "and it has haste. Exile that token at the beginning of the next end step."
    );
    let parsed = engine.parse(name, text).unwrap();
    let [reading] = parsed.readings.as_slice() else {
        panic!("expected one reading");
    };
    let exceptions = &reading[0]["args"]["ability"]["args"]["body"]["instructions"][0]["args"]["operation"]
        ["args"]["tokens"][0]["recipe"]["Copy"]["exceptions"];
    assert!(exceptions.as_array().unwrap().contains(&serde_json::json!({
        "kind":"AddCreatureTypes",
        "args":["Illusion"]
    })));
    assert!(exceptions.as_array().unwrap().contains(&serde_json::json!({
        "kind":"AddCardTypes",
        "args":["Artifact"]
    })));
}

#[test]
fn proposed_spell_filter_is_separate_from_card_selection() {
    let parsed = engine()
        .parse(
            "Brazen Bluff",
            concat!(
                "Choose target noncreature spell an opponent controls. That player may exile it. ",
                "If they don't, you may cast a noncreature spell from your hand ",
                "without paying its mana cost."
            ),
        )
        .unwrap();
    let [reading] = parsed.readings.as_slice() else {
        panic!("expected one reading");
    };
    let mut landed = reading.clone();
    let play = landed
        .pointer_mut("/0/args/body/instructions/1/args/then/instructions/0/args/operation")
        .unwrap();
    assert_eq!(play["kind"], "PlayWith");
    assert_eq!(play["args"]["cards"]["args"]["filter"]["kind"], "Any");
    assert_eq!(
        play["args"]["terms"]["cast_filter"],
        serde_json::json!({"kind":"Not", "args":{"kind":"CardType","args":"Creature"}})
    );
    play["args"]["action"] = serde_json::json!("PlayLand");
    let error = crate::engine::validate_abilities(&landed).unwrap_err();
    assert!(error.contains("land-only play"));
}

#[test]
fn play_origins_share_one_limit_and_compare_as_a_set() {
    let parsed = engine()
        .parse(
            "The Holodeck",
            concat!(
                "Once during each of your turns, you may play a permanent card you own ",
                "from outside the game or from your graveyard."
            ),
        )
        .unwrap();
    let [reading] = parsed.readings.as_slice() else {
        panic!("expected one reading");
    };
    let extended = &reading[0]["args"]["changes"][0]["args"]["PlayPermissionWith"];
    assert_eq!(
        extended["terms"]["limit"],
        serde_json::json!({"EachTurn":1})
    );
    let location = concat!(
        "/0/args/changes/0/args/PlayPermissionWith",
        "/permission/PlayPermission/origins"
    );
    assert_eq!(
        reading.pointer(location).unwrap(),
        &serde_json::json!(["Graveyard", "Outside"])
    );

    let mut reordered = reading.clone();
    *reordered.pointer_mut(location).unwrap() = serde_json::json!(["Outside", "Graveyard"]);
    assert_eq!(
        crate::engine::canonical_abilities(&reordered).unwrap(),
        reading.clone()
    );
    for invalid in [
        serde_json::json!([]),
        serde_json::json!(["Graveyard", "Graveyard"]),
        serde_json::json!(["Graveyard"]),
        serde_json::json!(["Outside"]),
        serde_json::json!(["Graveyard", "Outside", "Battlefield"]),
        serde_json::json!(["Graveyard", "Outside", "Stack"]),
    ] {
        *reordered.pointer_mut(location).unwrap() = invalid;
        assert!(crate::engine::validate_abilities(&reordered).is_err());
    }

    let mut mixed = reading.clone();
    *mixed.pointer_mut(location).unwrap() = serde_json::json!(["Exile"]);
    let cards = concat!(
        "/0/args/changes/0/args/PlayPermissionWith",
        "/permission/PlayPermission/cards/args/0"
    );
    *mixed.pointer_mut(cards).unwrap() =
        serde_json::json!({"kind":"AnyZone","args":{"kind":"Any"}});
    assert!(crate::engine::validate_abilities(&mixed).is_err());
}
