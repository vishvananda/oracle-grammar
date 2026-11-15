//! "If it's a land card" right after one library card is revealed or looked at tests
//! that card, never the creature that entered, the creature that attacked or the card
//! itself. A later move of "it" moves that library card.
use super::*;

/// Every object an `If` tests or a `Move` moves in `reading`.
fn tested_and_moved(reading: &Value) -> (Vec<Value>, Vec<Value>) {
    let (mut matched, mut moved) = (Vec::new(), Vec::new());
    visit(reading, "ObjectMatches", &mut matched);
    visit(reading, "Move", &mut moved);
    (
        matched,
        moved.iter().map(|m| m["objects"].clone()).collect(),
    )
}

#[test]
fn the_card_tested_after_a_reveal_or_look_is_that_library_card() {
    for text in [
        "When this creature enters, reveal the top card of your library. If it's a land card, \
         put it onto the battlefield. Otherwise, put that card into your hand.",
        "Whenever this creature attacks, look at the top card of your library. If it's a land \
         card, you may put it onto the battlefield tapped.",
        "Whenever this creature attacks, look at the top card of your library. If it's a land \
         card, you may put it onto the battlefield. If you don't put the card onto the \
         battlefield, put it into your hand.",
    ] {
        let reading = compositional(text);
        let (matched, moved) = tested_and_moved(&reading);
        assert!(matched.is_empty(), "{text}: {reading:#}");
        assert!(
            moved.iter().all(|objects| objects["kind"] == "Take"),
            "{text}: {reading:#}"
        );
        printed(&reading);
    }
}
