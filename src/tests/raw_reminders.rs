use crate::engine::{CardNote, CardReading, empty_card_characteristics};
use crate::model::ir::{Characteristics, ManaColor, ManaSymbol};
use crate::model::mtg_types::{CardType, SuperType};

const NAME: &str = "Synthetic";

fn exact_roundtrip(text: &str, context: &Characteristics) -> CardReading {
    let engine = super::engine();
    let parsed = engine
        .parse_card(NAME, text, context)
        .unwrap_or_else(|error| panic!("{text}: {error:?}"));
    for reading in parsed.readings {
        let key = engine
            .card_key_of(NAME, &reading, context, text, true)
            .unwrap();
        if let Some(key) = key
            && engine
                .print_card_trail(NAME, &reading, context, &key)
                .unwrap()
                .as_deref()
                == Some(text)
        {
            return reading;
        }
    }
    panic!("no exact print for {text}");
}

#[test]
fn native_and_bracketed_loyalty_headings_preserve_the_same_activations() {
    let context = empty_card_characteristics();
    let raw = "+2: Draw a card.\n−1: You gain 2 life.\n0: Scry 1.";
    let prepared = "[+2]: Draw a card.\n[−1]: You gain 2 life.\n[0]: Scry 1.";
    let raw = exact_roundtrip(raw, &context);
    let prepared = exact_roundtrip(prepared, &context);
    assert_eq!(raw, prepared);
    assert!(
        super::engine()
            .parse_card(NAME, "[+2: Draw a card.", &context)
            .is_err()
    );
}

#[test]
fn named_battlefield_entry_preserves_a_required_target() {
    let context = empty_card_characteristics();
    let text = "When this creature enters the battlefield, exile target creature. Its controller seeks a creature card.";
    let long = exact_roundtrip(text, &context);
    let short = exact_roundtrip(&text.replace(" the battlefield", ""), &context);
    assert_eq!(long, short);
    assert_eq!(
        long.abilities[0]["args"]["announcement"]["targets"][0]["count"],
        serde_json::json!({"Exactly":{"kind":"Constant","args":1}})
    );
}

#[test]
fn loyalty_discount_prints_the_draws_target_as_the_caster() {
    let context = empty_card_characteristics();
    let text = "−2: Target player draws two cards. Until your next turn, instant, sorcery, and planeswalker spells that player casts cost {2} less to cast.";
    exact_roundtrip(text, &context);
}

#[test]
fn mana_explanation_checks_the_activation_cost() {
    let context = empty_card_characteristics();
    let text = "{5}{U/P}: Transform this creature. Activate only as a sorcery. ({U/P} can be paid with either {U} or 2 life.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace(
        "({U/P} can be paid with either {U}",
        "({B/P} can be paid with either {B}",
    );
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn assist_limit_prints_from_the_current_mana_cost_slot() {
    let mut context = empty_card_characteristics();
    context.mana_cost = Some(vec![
        ManaSymbol::Generic(5),
        ManaSymbol::Color(ManaColor::Blue),
    ]);
    let text = "Assist (Another player can pay up to {5} of this spell's cost.)\nDraw a card.";
    let reading = exact_roundtrip(text, &context);
    assert_eq!(reading.notes, vec![CardNote::AssistLimit { slot: 0 }]);
    context.mana_cost.as_mut().unwrap()[0] = ManaSymbol::Generic(3);
    assert!(super::engine().parse_card(NAME, text, &context).is_err());
    assert_eq!(
        super::engine()
            .print_card_keyed(NAME, &reading, &context, &[])
            .unwrap()
            .unwrap(),
        text.replace("{5}", "{3}")
    );
}

#[test]
fn legendary_sorcery_explanation_requires_both_printed_types() {
    let mut context = empty_card_characteristics();
    context.card_types = vec![CardType::Sorcery];
    context.supertypes = vec![SuperType::Legendary];
    let text = "(You may cast a legendary sorcery only if you control a legendary creature or planeswalker.)\nDraw a card.";
    exact_roundtrip(text, &context);
    context.supertypes.clear();
    assert!(super::engine().parse_card(NAME, text, &context).is_err());
}

#[test]
fn awaken_explanation_checks_both_payment_and_counter_amount() {
    let context = empty_card_characteristics();
    let text = "Draw a card.\nAwaken 4—{4}{W} (If you cast this spell for {4}{W}, also put four +1/+1 counters on target land you control and it becomes a 0/0 Elemental creature with haste. It's still a land.)";
    exact_roundtrip(text, &context);
    for wrong in [
        text.replace("put four", "put three"),
        text.replace("for {4}{W}", "for {5}{W}"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn ward_explanation_repeats_the_granted_payment() {
    let context = empty_card_characteristics();
    let text = "Enchanted creature has ward {2}. (Whenever it becomes the target of a spell or ability an opponent controls, counter it unless that player pays {2}.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace("pays {2}", "pays {3}");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}

#[test]
fn optional_mill_explanation_keeps_its_amount_and_optionality() {
    let context = empty_card_characteristics();
    let text = "When this creature enters, you may mill two cards. (You may put the top two cards of your library into your graveyard.)";
    exact_roundtrip(text, &context);
    for wrong in [
        text.replace("may mill two", "may mill three"),
        text.replace("you may mill", "you mill"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn a_reminder_preserves_the_combined_entry_or_death_trigger() {
    let context = empty_card_characteristics();
    let text = "When this creature enters or dies, surveil 1. (Look at the top card of your library. You may put it into your graveyard.)";
    let parsed = super::engine().parse_card(NAME, text, &context).unwrap();
    let [reading] = parsed.readings.as_slice() else {
        panic!("expected one combined trigger");
    };
    assert_eq!(reading.abilities.as_array().unwrap().len(), 1);
    exact_roundtrip(text, &context);
}

#[test]
fn devour_explanation_checks_the_filter_and_multiplier() {
    let context = empty_card_characteristics();
    let text = "Devour artifact 2 (As this creature enters, you may sacrifice any number of artifacts. It enters with twice that many +1/+1 counters on it.)";
    exact_roundtrip(text, &context);
    for wrong in [
        text.replace("of artifacts.", "of creatures."),
        text.replace("twice", "three times"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn reinforce_explanation_checks_the_payment_and_amount() {
    let context = empty_card_characteristics();
    let text = "Reinforce 2—{2}{W} ({2}{W}, Discard this card: Put two +1/+1 counters on target creature.)";
    exact_roundtrip(text, &context);
    for wrong in [
        text.replace("({2}{W}", "({2}{U}"),
        text.replace("Put two", "Put three"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn a_fixed_airbend_explanation_keeps_the_action_and_payment() {
    let context = empty_card_characteristics();
    let text = "Airbend target nonland permanent. (Exile it. While it's exiled, its owner may cast it for {2} rather than its mana cost.)";
    exact_roundtrip(text, &context);
    for wrong in [
        text.replace("for {2}", "for {3}"),
        text.replace("Airbend", "Exile"),
    ] {
        assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
    }
}

#[test]
fn copy_explanation_distinguishes_permanent_spells_from_abilities() {
    let context = empty_card_characteristics();
    let text = "{4}, {T}: Copy target permanent spell you control. (The copy becomes a token.)";
    exact_roundtrip(text, &context);
    let wrong = text.replace("permanent spell", "activated ability");
    assert!(super::engine().parse_card(NAME, &wrong, &context).is_err());
}
