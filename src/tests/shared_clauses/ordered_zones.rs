//! One end of one player's library or graveyard is one phrase: an ordered end, a count,
//! a player's possessive and a zone. As a set it is `Take(Zone, count, end)`; the top card
//! read in place is `Query(Zone, 0)`. Cards looked at go back into their own library; the
//! seeded "put them back" names the controller's library, so it cannot follow a look at
//! another player's.
use super::*;

fn zone(zone: &str, player: Value) -> Value {
    json!({"kind":"Zone","args":{"scope":{"Players":{"zone":zone,
        "players":{"kind":"One","args":player}}},"kind":"Card","filter":{"kind":"Any"}}})
}

fn target() -> Value {
    json!({"kind":"Target","args":{"slot":0,"index":0}})
}

#[test]
fn either_end_of_any_players_zone_is_one_slice() {
    let text = "Exile the bottom card of target player's graveyard.";
    let reading = compositional(text);
    let moved = &reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"];
    assert_eq!(
        moved["objects"],
        json!({"kind":"Take","args":{"objects":zone("Graveyard", target()),
            "count":{"kind":"Constant","args":1},"from":"Bottom"}})
    );
    assert_eq!(printed_types(&reading, &["Sorcery"]), text);
    let text = "Put the top card of your graveyard on the bottom of your library.";
    let reading = compositional(text);
    let moved = &reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"];
    let controller = json!({"kind":"AbilityController"});
    assert_eq!(
        moved["objects"]["args"]["objects"],
        zone("Graveyard", controller)
    );
    assert_eq!(moved["objects"]["args"]["from"], "Top");
    assert_eq!(printed_types(&reading, &["Sorcery"]), text);
}

#[test]
fn a_top_card_is_tested_where_it_is() {
    let text = "If the top card of target player's graveyard is a creature card, draw a card.";
    let reading = compositional(text);
    let condition = &reading[0]["args"]["body"]["instructions"][0]["args"]["condition"];
    assert_eq!(condition["kind"], "ObjectMatches");
    assert_eq!(
        condition["args"]["object"],
        json!({"kind":"Query","args":{"objects":zone("Graveyard", target()),"index":0}})
    );
    printed_types(&reading, &["Sorcery"]);
}

#[test]
fn a_written_kind_filters_the_graveyard_before_its_top_card_is_paid() {
    let text = "{1}, Exile the top creature card of your graveyard: Draw a card.";
    let reading = compositional(text);
    let part = &reading[0]["args"]["cost"]["parts"][1]["Move"];
    let taken = &part["selections"][0]["objects"]["args"];
    assert_eq!(
        taken["objects"]["args"]["filter"],
        json!({"kind":"CardType","args":"Creature"})
    );
    assert_eq!(taken["from"], "Top");
    assert_eq!(printed_types(&reading, &["Artifact"]), text);
}

#[test]
fn cards_looked_at_go_back_into_their_own_library() {
    let text = "When this creature enters, look at the top three cards of target player's \
                library, then put them back in any order.";
    let reading = compositional(text);
    let instructions = &reading[0]["args"]["body"]["instructions"];
    let reorder = instructions.to_string();
    assert!(
        reorder.contains(r#""library_owner":{"kind":"Target""#),
        "{reorder}"
    );
}
