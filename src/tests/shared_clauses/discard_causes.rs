//! A spell or ability an opponent controls makes you discard this card. The trigger
//! works from every zone, `it` follows the discarded card, and `that player` is the
//! one who controls the cause, never the player who discarded.
use super::*;

fn first_operation(reading: &Value) -> &Value {
    &reading[0]["args"]["ability"]["args"]["body"]["instructions"][0]["args"]["operation"]
}

#[test]
fn an_opponents_discard_names_the_card_and_the_player_who_caused_it() {
    let returned = "When a spell or ability an opponent controls causes you to discard this \
                    card, return it to your hand.";
    let reading = compositional(returned);
    assert_eq!(reading[0]["kind"], "Scoped");
    assert_eq!(
        first_operation(&reading)["args"]["objects"],
        json!({"kind":"One","args":{"kind":"Discarded"}})
    );
    printed(&reading);
    let punished = "When a spell or ability an opponent controls causes you to discard this \
                    card, that player loses 5 life.";
    let reading = compositional(punished);
    assert_eq!(
        first_operation(&reading)["args"]["players"],
        json!({"kind":"One","args":{"kind":"ControllerOf","args":{"kind":"EventStackObject"}}})
    );
    printed(&reading);
}

#[test]
fn an_opponents_discard_can_put_this_card_onto_the_battlefield() {
    let text = "If a spell or ability an opponent controls causes you to discard this card, \
                put it onto the battlefield with two +1/+1 counters on it instead of putting \
                it into your graveyard.";
    let reading = compositional(text);
    assert_eq!(reading[0]["kind"], "Replacement");
    assert_eq!(reading[0]["args"]["active"], "Everywhere");
    assert_eq!(printed(&reading), text);
}
