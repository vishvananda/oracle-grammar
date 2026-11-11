//! Adjacent statements and local definitions keep their independent operands.
use super::*;

#[test]
fn complete_composed_cards_print_and_reparse_their_typed_meanings() {
    for name in [
        "Elenda, Saint of Dusk",
        "Filigree Racer",
        "Winged Temple of Orazca",
        "Hideous Taskmaster",
        "Mirror Gallery",
        "Retro-Mutation",
        "Tenza, Godo's Maul",
    ] {
        let case = cases()
            .into_iter()
            .find(|case| case["name"] == name)
            .unwrap();
        let types: Vec<_> = case["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|ty| ty.as_str().unwrap())
            .collect();
        printed_types(&case["expected"], &types);
    }
}

#[test]
fn explicit_local_x_sets_both_dimensions_and_prints_the_changed_property() {
    let case = cases()
        .into_iter()
        .find(|case| case["name"] == "Arahbo, Roar of the World")
        .unwrap();
    let readings = engine()
        .parse("Arahbo, Roar of the World", case["text"].as_str().unwrap())
        .unwrap()
        .readings;
    assert!(readings.contains(&canonical_abilities(&case["additional_expected"]).unwrap()));
    assert!(readings.contains(&canonical_abilities(&case["expected"]).unwrap()));
    let mut frame = expected("Winged Temple of Orazca");
    let changes =
        &mut frame[1]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"];
    changes[0]["args"] = json!({"kind":"Vigilance"});
    let pump = &mut changes[1]["args"];
    assert_eq!(pump["power"], pump["toughness"]);
    pump["power"]["args"]["property"] = json!("Toughness");
    pump["toughness"]["args"]["property"] = json!("Toughness");
    let text = printed_types(&frame, &["Land"]).to_lowercase();
    assert!(
        text.contains("vigilance") && text.contains("toughness"),
        "{text}"
    );
    assert!(!text.contains("its power"), "{text}");
}

#[test]
fn attachment_conditions_print_their_own_color_supertype_and_bonus() {
    let mut frame = expected("Tenza, Godo's Maul");
    frame[1]["args"]["condition"]["args"]["filter"] = json!({"kind":"Supertype","args":"Snow"});
    frame[2]["args"]["condition"]["args"]["filter"] = json!({"kind":"Color","args":"Blue"});
    let changes = &mut frame[1]["args"]["then"][0]["args"]["changes"][0]["args"];
    changes["power"] = json!({"kind":"Constant","args":3});
    changes["toughness"] = json!({"kind":"Constant","args":4});
    let text = printed_types(&frame, &["Artifact"]).to_lowercase();
    assert!(
        text.contains("snow") && text.contains("blue") && text.contains("+3/+4"),
        "{text}"
    );
    for index in [1, 2] {
        assert_eq!(
            frame[index]["args"]["condition"]["args"]["object"],
            json!({"kind":"AttachedTo","args":{"kind":"Source"}})
        );
    }
}

#[test]
fn host_characteristics_print_new_subtype_and_both_base_stats() {
    let mut frame = expected("Retro-Mutation");
    let changes = &mut frame[2]["args"]["changes"];
    changes[0]["args"]["types"]["Listed"] = json!(["Frog"]);
    changes[1]["args"]["power"] = json!({"kind":"Constant","args":2});
    changes[1]["args"]["toughness"] = json!({"kind":"Constant","args":3});
    let text = printed_types(&frame, &["Enchantment"]).to_lowercase();
    assert!(text.contains("frog") && text.contains("2/3"), "{text}");
    assert!(!text.contains("turtle"), "{text}");
}

#[test]
fn numbered_grants_print_changed_amounts_as_recipient_mechanics() {
    let text = "Target creature gains annihilator 2 until end of turn.";
    let mut readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1);
    let mut frame = readings.remove(0);
    let mechanic = &mut frame[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"]
        [0]["args"];
    assert_eq!(mechanic["kind"], "Mechanic");
    assert_eq!(mechanic["args"]["mechanic"], "Annihilator");
    mechanic["args"]["amount"] = json!({"kind":"Constant","args":3});
    let text = printed_types(&frame, &["Instant"]).to_lowercase();
    assert!(text.contains("annihilator 3"), "{text}");
}

#[test]
fn life_margin_and_the_first_bonus_print_independent_new_values() {
    let mut frame = expected("Elenda, Saint of Dusk");
    frame[2]["args"]["then"][0]["args"]["changes"][0]["args"]["power"] =
        json!({"kind":"Constant","args":2});
    frame[3]["args"]["condition"]["args"]["test"]["args"]["value"]["args"][0] =
        json!({"kind":"Constant","args":7});
    let text = printed(&frame).to_lowercase();
    assert!(
        text.contains("+2/+1") && (text.contains("seven") || text.contains('7')),
        "{text}"
    );
}
