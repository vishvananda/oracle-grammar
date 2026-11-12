//! A default print chooses a word for the card where it names the card, and only there.
use super::*;

fn reprinted(text: &str, types: &[&str]) -> String {
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{text}: {readings:#?}");
    printed_types(&readings[0], types)
}

#[test]
fn a_filter_excluding_the_card_passes_over_no_word_for_it() {
    for text in [
        "Whenever another creature you control enters, this creature deals 1 damage to each opponent.",
        "Other creatures you control get +1/+1.",
    ] {
        assert_eq!(reprinted(text, &["Creature"]), text);
    }
}

#[test]
fn it_names_the_card_only_after_the_card_is_named() {
    for text in [
        "Whenever this creature attacks, it gets +1/+0 until end of turn.",
        "{T}: Put a +1/+1 counter on this creature.",
        "When this creature enters, it deals 2 damage to any target.",
        "Whenever this creature attacks, create a tapped and attacking token that's a copy of it.",
    ] {
        assert_eq!(reprinted(text, &["Creature"]), text);
    }
}

#[test]
fn a_legendary_card_is_named_wherever_its_word_is_chosen() {
    let text = "Whenever Context example attacks, draw a card.";
    assert_eq!(reprinted(text, &["Legendary", "Creature"]), text);
}

#[test]
fn a_sentence_names_the_subject_of_its_verb() {
    let text = "{T}: Add {G} or {W}. This land deals 1 damage to you.";
    assert_eq!(reprinted(text, &["Land"]), text);
}
