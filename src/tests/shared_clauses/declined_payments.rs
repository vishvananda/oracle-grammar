//! "If you don't, …" tests the offered payment's own result. A top-card reveal is such
//! a payment only when a later sentence asks whether the card went into your hand.
use super::*;

fn instructions(reading: &Value) -> &Value {
    &reading[0]["args"]["body"]["instructions"]
}

#[test]
fn a_declined_payment_guards_what_follows() {
    let text = "You may pay {2}. If you don't, you lose 3 life.";
    let reading = compositional(text);
    let items = instructions(&reading);
    assert_eq!(items[0]["args"]["operation"]["kind"], "PayCost");
    assert_eq!(items[1]["args"]["condition"]["kind"], "Not");
    // `You lose 3 life unless you pay {2}` is the same choice.
    printed_types(&reading, &["Sorcery"]);
    let both = "You may pay {2}. If you do, draw a card. If you don't, you lose 3 life.";
    let reading = compositional(both);
    let branch = &instructions(&reading)[1]["args"];
    assert_eq!(branch["condition"]["kind"], "PaymentChosen");
    assert_eq!(
        branch["otherwise"]["instructions"][0]["args"]["operation"]["kind"],
        "LoseLife"
    );
    printed_types(&reading, &["Sorcery"]);
}

#[test]
fn a_top_card_reveal_is_optional_unless_a_later_sentence_tests_it() {
    let plain = "Look at the top card of your library. If it's a land card, you may reveal it \
                 and put it into your hand.";
    let reading = compositional(plain);
    assert_eq!(instructions(&reading)[1]["kind"], "Optional");
    assert_eq!(printed_types(&reading, &["Sorcery"]), plain);
    let taken = "Look at the top card of your library. If it's a creature card, you may reveal \
                 it and put it into your hand. If you put a card into your hand this way, you \
                 gain 2 life.";
    let reading = compositional(taken);
    let items = instructions(&reading);
    assert_eq!(items[1]["args"]["operation"]["kind"], "PayCost");
    assert_eq!(items[2]["args"]["condition"]["kind"], "PaymentChosen");
    printed_types(&reading, &["Sorcery"]);
}
