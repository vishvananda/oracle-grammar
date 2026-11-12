//! A copy may take new targets, and a counter may send its spell somewhere other than
//! its owner's graveyard. Each later sentence completes the action before it.
use super::*;

fn operation(reading: &Value, index: usize) -> &Value {
    &reading[0]["args"]["body"]["instructions"][index]["args"]["operation"]
}

#[test]
fn a_copy_may_choose_new_targets() {
    let text = "Copy target instant or sorcery spell. You may choose new targets for the copy.";
    let reading = compositional(text);
    assert_eq!(operation(&reading, 0)["args"]["targets"], "May");
    assert_eq!(printed_types(&reading, &["Instant"]), text);
}

#[test]
fn a_countered_spell_goes_where_the_next_sentence_says() {
    for (text, destination) in [
        (
            "Counter target spell. If that spell is countered this way, exile it instead of \
             putting it into its owner's graveyard.",
            json!({"Exile":{"visibility":"FaceUp","arrival":{"counters":[],"states":[]}}}),
        ),
        (
            "Counter target spell. If that spell is countered this way, put it on top of its \
             owner's library instead of into that player's graveyard.",
            json!({"Library":{"position":"Top","order":"DestinationDefault",
                "arrival":{"counters":[],"states":[]}}}),
        ),
    ] {
        let reading = compositional(text);
        assert_eq!(
            operation(&reading, 0)["args"]["destination"],
            destination,
            "{text}"
        );
        assert_eq!(printed_types(&reading, &["Instant"]), text);
    }
    let unless = "Counter target spell unless its controller pays {3}. If that spell is \
                  countered this way, exile it instead of putting it into its owner's graveyard.";
    let reading = compositional(unless);
    let countered =
        &reading[0]["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0];
    assert_eq!(
        countered["args"]["operation"]["args"]["destination"]["Exile"]["visibility"],
        "FaceUp"
    );
}
