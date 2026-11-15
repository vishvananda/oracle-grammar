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

#[test]
fn a_bare_pronoun_after_a_reveal_is_the_revealed_card_not_this_spell() {
    let text = "Reveal the top card of your library. You gain life equal to its mana value.";
    let reading = compositional(text);
    let reveal = result_of(&reading, "Reveal");
    assert_eq!(
        measured(&reading, "ManaValue"),
        [query(json!({"kind":"Observed","args":reveal}))]
    );
    printed_types(&reading, &["Sorcery"]);
}

#[test]
fn a_tested_and_measured_pronoun_reaches_past_a_draw_to_the_revealed_card() {
    // A seeded rule also reads this sentence with the reference's placeholders, so
    // check every reading: none measures this spell, and the written one is exact.
    let text = "Reveal the top card of your library. If it's a creature card, you draw cards \
                equal to its power and you gain life equal to its toughness.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    let source = json!({"kind":"Source"});
    let exact = readings.iter().find(|reading| {
        let observed = query(json!({"kind":"Observed","args":result_of(reading, "Reveal")}));
        let mut tested = Vec::new();
        visit(reading, "ObjectMatches", &mut tested);
        ["Power", "Toughness"]
            .iter()
            .all(|property| measured(reading, property) == [observed.clone()])
            && tested
                .first()
                .is_some_and(|args| args["object"] == observed)
    });
    for reading in &readings {
        for property in ["Power", "Toughness"] {
            assert!(
                !measured(reading, property).contains(&source),
                "{reading:#}"
            );
        }
    }
    printed_types(exact.expect("the revealed card is measured"), &["Sorcery"]);
}

#[test]
fn a_reflexive_or_copy_pronoun_after_a_reveal_keeps_its_own_subject() {
    for text in [
        "When this creature enters, reveal the top card of your library and put it into \
         your hand. This creature deals damage to itself equal to that card's mana value.",
        "At the beginning of each combat, you may reveal the top card of your library. If \
         you reveal a creature card this way, this creature becomes a copy of that card \
         until end of turn, except it has flying.",
    ] {
        let readings = engine()
            .parse("Context example", text)
            .map(|parsed| parsed.readings)
            .unwrap_or_default();
        for reading in readings {
            let mut damage = Vec::new();
            visit(&reading, "DealDamage", &mut damage);
            for args in damage {
                assert!(
                    !args["recipients"].to_string().contains("Observed"),
                    "{text}: {args:#}"
                );
            }
            let mut copies = Vec::new();
            visit(&reading, "BecomeCopy", &mut copies);
            for args in copies {
                assert!(!args["source"].to_string().contains("Observed"), "{text}");
            }
        }
    }
}
