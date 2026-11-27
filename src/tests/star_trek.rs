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
