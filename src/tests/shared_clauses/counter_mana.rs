//! Counter quantities follow actual payment components and current operands.
use super::*;

fn receipt<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    if value["kind"] == "PaidCountersRemoved" {
        out.push(&value["args"]);
    } else if let Some(fields) = value.as_object() {
        fields.values().for_each(|v| receipt(v, out));
    } else if let Some(items) = value.as_array() {
        items.iter().for_each(|v| receipt(v, out));
    }
}

#[test]
fn counter_mana_receipts_follow_the_current_activation_cost() {
    for (text, index) in [
        (
            "Remove any number of storage counters from this land: Add {U} for each storage counter removed this way.",
            0,
        ),
        (
            "{T}, Remove any number of storage counters from this land: Add {U} for each storage counter removed this way.",
            1,
        ),
        (
            "{2}, {T}, Remove any number of storage counters from this land: Add {U} for each storage counter removed this way.",
            2,
        ),
        (
            "{T}, Remove any number of charge counters from this artifact: Add {B}, then add an additional {B} for each charge counter removed this way.",
            1,
        ),
    ] {
        let reading = compositional(text);
        let mut found = Vec::new();
        receipt(&reading, &mut found);
        assert_eq!(found.len(), 1, "{text}");
        assert_eq!(
            found[0],
            &json!({"payment":"Activation","component":[index]})
        );
        printed_types(&reading, &["Artifact", "Land"]);
    }
}

#[test]
fn edited_counter_kind_cost_order_and_mana_print_from_operands() {
    let mut reading = compositional(
        "{T}, Remove any number of storage counters from this land: Add {U} for each storage counter removed this way.",
    );
    let ability = &mut reading[0]["args"];
    ability["cost"]["parts"][1]["RemoveCounters"]["removals"][0]["kind"] =
        json!({"Named":"ChargeCounter"});
    ability["cost"]["parts"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"Mana":[{"Generic":2}]}));
    let mana =
        &mut ability["body"]["instructions"][0]["args"]["operation"]["args"]["mana"]["Repeat"];
    mana["amount"]["args"]["component"] = json!([2]);
    mana["production"]["Amount"]["color"] = json!("Red");
    let text = printed_types(&reading, &["Land"]);
    assert!(text.contains("charge counter removed this way"), "{text}");
    assert!(text.contains("{R}"), "{text}");
    assert!(!text.contains("storage"), "{text}");
}

#[test]
fn counter_mana_keeps_independent_color_choices_per_removed_counter() {
    let reading = compositional(
        "{T}, Remove any number of lore counters from this artifact: Add {W} or {B} for each lore counter removed this way.",
    );
    let mana = &reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["mana"];
    assert_eq!(mana["Repeat"]["choice"], "EachUnit");
    assert!(mana["Repeat"]["production"].get("ChooseOne").is_some());
    printed_types(&reading, &["Artifact"]);
}

#[test]
fn additional_mana_keeps_the_named_recipient_and_complete_bundle() {
    let reading = compositional(
        "Whenever a Forest is tapped for mana, its controller adds an additional {G}{G}.",
    );
    let operation = &reading[0]["args"]["body"]["instructions"][0]["args"]["operation"];
    assert_eq!(
        operation["args"]["players"],
        json!({"kind":"One","args":{
        "kind":"ControllerOf","args":{"kind":"EventSubject"}}})
    );
    assert_eq!(
        operation["args"]["mana"],
        json!({"Amount":{
        "color":"Green","amount":{"kind":"Constant","args":2}}})
    );
    let mut edited = reading.clone();
    edited[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["players"] =
        json!({"kind":"One","args":{"kind":"AbilityController"}});
    let text = printed(&edited);
    assert!(
        !text.to_lowercase().contains("its controller adds"),
        "{text}"
    );
}

#[test]
fn removed_counter_mana_refuses_unproved_payment_contexts() {
    for text in [
        "Add {U} for each storage counter removed this way.",
        "{T}: Add {U} for each storage counter removed this way.",
        "Remove any number of charge counters from this artifact: Add {U} for each storage counter removed this way.",
        "Remove any number of lore counters from this artifact, Remove any number of lore counters from this artifact: Add {U} for each lore counter removed this way.",
        "Remove any number of lore counters from this artifact: Remove a lore counter from this artifact. Add {U} for each lore counter removed this way.",
    ] {
        match engine().parse("Context example", text) {
            Ok(parsed) => assert!(parsed.readings.is_empty(), "{text}"),
            Err(Refusal::Binding(_)) => {}
            Err(other) => panic!("unexpected refusal for {text}: {other:?}"),
        }
    }
}

#[test]
fn delayed_mana_keeps_the_player_who_tapped_the_land() {
    let reading = compositional(
        "Until end of turn, whenever a player taps a Forest for mana, that player adds an additional {G}.",
    );
    let trigger = &reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"];
    assert_eq!(trigger["duration"], "EndOfTurn");
    let player = &trigger["body"]["instructions"][0]["args"]["operation"]["args"]["players"];
    assert_eq!(
        *player,
        json!({"kind":"One","args":{"kind":"EventSubject"}})
    );
    let mut wrong = reading.clone();
    wrong[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["body"]["instructions"]
        [0]["args"]["operation"]["args"]["players"] =
        json!({"kind":"One","args":{"kind":"AbilityController"}});
    assert_ne!(canonical_abilities(&wrong).unwrap(), reading);
    printed(&reading);
    let text = printed(&wrong);
    assert!(!text.to_lowercase().contains("that player adds"), "{text}");
}
