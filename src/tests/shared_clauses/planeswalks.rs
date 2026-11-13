//! A plane names itself in its planeswalk triggers. "When you planeswalk to X and at the
//! beginning of your upkeep" is one trigger whose event is either moment. A card exiled
//! from another player's library is tested by counting the moved card.
use super::*;

#[test]
fn a_planeswalk_and_a_turn_moment_share_one_trigger() {
    let text = "When you planeswalk to Context example and at the beginning of your upkeep, \
                investigate.";
    let reading = compositional(text);
    let moments = reading[0]["args"]["event"]["AnyOf"].as_array().unwrap();
    assert_eq!(moments[0]["Player"]["event"], "Planeswalks");
    assert_eq!(moments[1]["Turn"]["point"], "UpkeepBegins");
    assert_eq!(printed_types(&reading, &["Plane"]), text);
    let main = "When you planeswalk to Context example and at the beginning of your first main \
                phase, add {R}.";
    let reading = compositional(main);
    assert_eq!(
        reading[0]["args"]["event"]["AnyOf"][1]["Turn"]["point"],
        "FirstMainPhaseBegins"
    );
    let away = "When you planeswalk away from Context example, draw a card.";
    let reading = compositional(away);
    assert_eq!(
        reading[0]["args"]["event"]["Player"]["event"],
        "PlaneswalksAway"
    );
    // `When` and `Whenever` name the same trigger; Oracle uses both before `you planeswalk`.
    printed_types(&reading, &["Plane"]);
}

#[test]
fn another_players_exiled_top_card_is_counted() {
    let text = "Target opponent exiles the top card of their library. If it's a land card, you \
                gain 3 life.";
    let reading = compositional(text);
    let condition = &reading[0]["args"]["body"]["instructions"][1]["args"]["condition"];
    assert_eq!(condition["kind"], "Compare");
    assert_eq!(condition["args"]["left"]["kind"], "CountObjects");
    assert_eq!(printed_types(&reading, &["Sorcery"]), text);
}
