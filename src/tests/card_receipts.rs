//! "That card" names the one card the instruction before revealed, discarded, moved
//! or paid, in that producer's own receipt, and nothing when no producer comes first.
use super::*;

/// The objects whose `property` the reading measures.
fn measured(reading: &Value, property: &str) -> Vec<Value> {
    let mut found = Vec::new();
    visit(reading, "ObjectProperty", &mut found);
    found
        .into_iter()
        .filter(|args| args["property"] == property)
        .map(|args| args["object"].clone())
        .collect()
}

/// The result address of the first effect performing `operation`.
fn result_of(reading: &Value, operation: &str) -> Value {
    let mut found = Vec::new();
    visit(reading, "Effect", &mut found);
    found
        .into_iter()
        .find(|args| args["operation"]["kind"] == operation)
        .map(|args| args["result"].clone())
        .unwrap_or_else(|| panic!("no {operation} in {reading:#}"))
}

fn query(objects: Value) -> Value {
    json!({"kind":"Query","args":{"objects":objects,"index":0}})
}

fn refused(text: &str) {
    let readings = engine()
        .parse("Context example", text)
        .map(|parsed| parsed.readings)
        .unwrap_or_default();
    assert!(readings.is_empty(), "{text}: {readings:#?}");
}

#[test]
fn a_revealed_card_is_the_observed_card() {
    let text = "When this creature enters, scry 3, then reveal the top card of your library. \
                You gain life equal to that card's mana value.";
    let reading = compositional(text);
    let reveal = result_of(&reading, "Reveal");
    assert_eq!(
        measured(&reading, "ManaValue"),
        [query(json!({"kind":"Observed","args":reveal}))]
    );
    printed(&reading);
}

#[test]
fn a_random_hand_reveal_and_its_typed_noun_are_the_observed_card() {
    for text in [
        "{3}{W}: Target opponent reveals a card at random from their hand. You gain life \
         equal to that card's mana value.",
        "{3}{G}: Target opponent reveals a card at random from their hand. Target creature \
         gets +X/+X until end of turn, where X is the revealed card's mana value.",
    ] {
        let reading = compositional(text);
        let reveal = query(json!({"kind":"Observed","args":result_of(&reading, "RevealFromHand")}));
        // "+X/+X" measures the same card once for each characteristic.
        let measures = measured(&reading, "ManaValue");
        assert!(
            !measures.is_empty() && measures.iter().all(|object| *object == reveal),
            "{text}: {measures:#?}"
        );
        printed(&reading);
    }
}

#[test]
fn a_discarded_card_is_its_information_before_the_discard() {
    for text in [
        "When this creature enters, target player discards a card. This creature deals \
         damage to that player equal to that card's mana value.",
        "Whenever this creature attacks, draw a card, then discard a card. This creature \
         gets +X/+0 until end of turn, where X is the discarded card's mana value.",
    ] {
        let reading = compositional(text);
        let discard = result_of(&reading, "Discard");
        assert_eq!(
            measured(&reading, "ManaValue"),
            [query(
                json!({"kind":"Transition","args":{"result":discard,"phase":"Before"}})
            )],
            "{text}"
        );
        printed(&reading);
    }
}

#[test]
fn a_moved_target_stays_the_target() {
    for (text, property) in [
        (
            "Return target card from your graveyard to your hand. You gain life equal to \
             that card's mana value.",
            "ManaValue",
        ),
        (
            "When this creature enters, return target creature card from your graveyard to \
             the battlefield. You lose life equal to that card's mana value.",
            "ManaValue",
        ),
        (
            "When this creature enters, exile target creature card from a graveyard. You \
             gain life equal to that card's toughness.",
            "Toughness",
        ),
    ] {
        let reading = compositional(text);
        assert_eq!(
            measured(&reading, property),
            [json!({"kind":"Target","args":{"slot":0,"index":0}})],
            "{text}"
        );
        printed(&reading);
    }
}

#[test]
fn a_paid_card_is_the_payment_input() {
    let text = "When this creature enters, you may discard a card. If you do, seek a card \
                with mana value equal to that card's mana value.";
    let reading = compositional(text);
    let payment = result_of(&reading, "PayCost");
    assert_eq!(
        measured(&reading, "ManaValue"),
        [query(json!({"kind":"CostObjects","args":{
            "payment":{"Resolution":payment},"component":[0],"view":"Inputs","phase":"Before"
        }}))]
    );
    printed(&reading);
}

#[test]
fn that_card_needs_a_producer_and_a_typed_noun_needs_its_own() {
    // No antecedent: neither this card, the drawn cards nor an event object.
    refused(
        "When this creature enters, draw a card. You gain life equal to that card's mana value.",
    );
    refused("You gain life equal to that card's mana value.");
    // A revealed card is not "the discarded card".
    refused(
        "When this creature enters, reveal the top card of your library. You gain life \
         equal to the discarded card's mana value.",
    );
}
