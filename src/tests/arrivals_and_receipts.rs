//! Arrivals attached to a host, exiles that stop on one card, and sentences that
//! continue an `if you do` branch.
use super::*;

/// The effect operations directly in an instruction list.
fn kinds(instructions: &Value) -> Vec<String> {
    instructions
        .as_array()
        .unwrap()
        .iter()
        .map(|instruction| instruction["kind"].as_str().unwrap().to_string())
        .collect()
}

/// The one reading of `text` with every rule on: a guard refused each other reading.
fn only(text: &str) -> Value {
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{text}: {readings:#?}");
    readings[0].clone()
}

#[test]
fn an_aura_from_hand_enters_attached_to_this_creature() {
    let reading = compositional(
        "When this creature enters, you may put an Aura card from your hand onto the \
         battlefield attached to this creature.",
    );
    let text = reading.to_string();
    assert!(
        text.contains(r#""states":[{"AttachedTo":{"kind":"Source"}}]"#),
        "{text}"
    );
    printed(&reading);
}

#[test]
fn returned_auras_attach_to_the_returned_creature_not_themselves() {
    let reading = only(
        "Return target creature card from your graveyard to the battlefield, then return up \
         to two target Aura and/or Equipment cards from your graveyard to the battlefield \
         attached to that creature.",
    );
    let host = &reading[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["destination"]
        ["Battlefield"]["arrival"]["states"][0]["AttachedTo"];
    assert_eq!(host["kind"], "Result", "{host}");
    printed_types(&reading, &["Sorcery"]);
}

#[test]
fn a_sentence_about_paid_targets_continues_the_if_you_do_branch() {
    let reading = only(
        "Whenever this creature attacks, you may pay {8}. If you do, tap up to eight target \
         permanents. Those permanents don't untap during their controllers' next untap steps.",
    );
    let body = &reading[0]["args"]["body"]["instructions"];
    assert_eq!(kinds(body), ["Effect", "If"]);
    assert_eq!(
        kinds(&body[1]["args"]["then"]["instructions"]),
        ["Effect", "Effect"]
    );
    printed(&reading);
}

#[test]
fn an_exile_until_names_the_card_it_stopped_on() {
    let reading = compositional(
        "Whenever a land you control enters, exile cards from the top of your library until \
         you exile a nonland card. You may cast that card this turn.",
    );
    let body = &reading[0]["args"]["body"]["instructions"];
    // The canonical reading flattens the reveal-and-exile group.
    assert_eq!(kinds(body), ["Effect", "Effect", "Effect"]);
    let exile = body[1]["args"]["result"].clone();
    let text = body[2].to_string();
    let receipt = json!({"kind":"Query","args":{"objects":{"kind":"Matching","args":{
        "objects":{"kind":"Transition","args":{"result":exile,"phase":"After"}},
        "filter":{"kind":"Not","args":{"kind":"CardType","args":"Land"}}}},"index":0}});
    assert!(text.contains(&receipt.to_string()), "{text}");
    printed(&reading);
}

#[test]
fn the_card_an_exile_until_stopped_on_can_be_cast_free() {
    let reading = compositional(
        "Whenever you cast your second spell each turn, exile cards from the top of your \
         library until you exile a nonland card. You may cast that card without paying its \
         mana cost.",
    );
    let text = reading.to_string();
    assert!(text.contains(r#""cost":"Free""#), "{text}");
    assert!(text.contains(r#""kind":"Transition""#), "{text}");
    printed_types(&reading, &["Enchantment"]);
}

#[test]
fn any_number_of_cards_from_hand_enter_the_battlefield() {
    let reading = compositional(
        "When this creature enters, put any number of creature cards from your hand onto the \
         battlefield.",
    );
    let text = reading.to_string();
    assert!(text.contains(r#""count":"AnyNumber""#), "{text}");
    printed(&reading);
}

#[test]
fn all_cards_of_a_kind_return_from_your_graveyard() {
    let tapped =
        compositional("Return all land cards from your graveyard to the battlefield tapped.");
    assert!(tapped.to_string().contains(r#""tapped":true"#), "{tapped}");
    printed_types(&tapped, &["Sorcery"]);
    let hand = compositional(
        "{3}, {T}, Sacrifice this artifact: Return all enchantment cards from your graveyard \
         to your hand.",
    );
    assert!(hand.to_string().contains("OwnersHand"), "{hand}");
    printed_types(&hand, &["Artifact"]);
}

#[test]
fn mana_of_any_combination_can_be_restricted_to_a_kind_of_spell() {
    let reading = compositional(
        "{T}: Add two mana in any combination of colors. Spend this mana only to cast \
         Elemental spells or activate abilities of Elementals.",
    );
    let text = reading.to_string();
    assert!(text.contains("restriction"), "{text}");
    printed(&reading);
}

#[test]
fn spending_this_mana_on_a_spell_triggers() {
    let reading = compositional(
        "{T}: Add {U}. When you spend this mana to cast a Dragon creature spell, scry 2.",
    );
    let text = reading.to_string();
    assert!(text.contains("WhenSpent"), "{text}");
    printed_types(&reading, &["Artifact"]);
}

#[test]
fn a_spell_costs_less_for_each_card_paid_this_way() {
    let reading = compositional(
        "As an additional cost to cast this spell, you may exile any number of white cards \
         from your hand. This spell costs {2} less to cast for each card exiled this way.",
    );
    assert_eq!(reading[0]["kind"], "Casting", "{reading}");
    assert!(
        reading.to_string().contains("ReduceForEachPaid"),
        "{reading}"
    );
    printed_types(&reading, &["Instant"]);
}

#[test]
fn a_cost_you_cannot_pay_at_resolution_has_its_consequence() {
    let reading = compositional(
        "At the beginning of your upkeep, sacrifice a creature. If you can't, sacrifice this \
         artifact.",
    );
    let body = &reading[0]["args"]["body"]["instructions"];
    assert_eq!(body[0]["args"]["operation"]["kind"], "PayCost", "{body}");
    assert_eq!(
        body[0]["args"]["operation"]["args"]["optional"], false,
        "{body}"
    );
    assert_eq!(body[1]["args"]["condition"]["kind"], "Not", "{body}");
    printed_types(&reading, &["Artifact"]);
}
