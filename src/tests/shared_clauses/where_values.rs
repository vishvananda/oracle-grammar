//! A `where X is` clause names a counted or aggregate value; the counted
//! population keeps its own zone, owner and counter recipient.
use super::*;

fn amounts(reading: &Value, kind: &str) -> Vec<Value> {
    let mut found = Vec::new();
    visit(reading, kind, &mut found);
    found
}

#[test]
fn entering_counters_take_their_number_from_the_where_clause() {
    let text = "This creature enters with X +1/+1 counters on it, where X is the number of \
                creature cards in all graveyards.";
    let reading = compositional(text);
    let counted = amounts(&reading, "CountObjects");
    assert_eq!(
        counted[0]["args"]["scope"],
        json!({"Players":{"zone":"Graveyard","players":{"kind":"All"}}})
    );
    assert!(amounts(&reading, "AnnouncedX").is_empty());
    assert_eq!(printed(&reading), text);
}

#[test]
fn named_card_zones_keep_their_owner() {
    let text = "Whenever this creature attacks, it gets +X/+0 until end of turn, where X is the \
                number of creature cards in defending player's graveyard.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert!(!readings.is_empty());
    for reading in &readings {
        let counted = amounts(reading, "CountObjects");
        assert_eq!(
            counted[0]["args"]["scope"]["Players"]["players"]["args"]["kind"],
            "DefendingPlayerOf"
        );
    }
}

#[test]
fn counts_of_players_names_counters_and_totals_are_values() {
    for (text, kind) in [
        (
            "Draw X cards, where X is the number of opponents you have.",
            "CountPlayers",
        ),
        (
            "Draw X cards, where X is the number of differently named lands you control.",
            "CountDistinct",
        ),
        (
            "Draw X cards, where X is the total power of creatures you control.",
            "Aggregate",
        ),
        (
            "Draw X cards, where X is the number of experience counters you have.",
            "CountCounters",
        ),
        (
            "Draw X cards, where X is the number of tokens you created this turn.",
            "EventHistory",
        ),
        (
            "Draw X cards, where X is the number of spells you've cast this turn.",
            "EventHistory",
        ),
    ] {
        let readings = engine().parse("Context example", text).unwrap().readings;
        assert!(!readings.is_empty(), "{text}");
        for reading in &readings {
            assert!(amounts(reading, "AnnouncedX").is_empty(), "{text}");
            assert_eq!(amounts(reading, kind).len(), 1, "{text}");
        }
    }
}
