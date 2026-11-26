use super::*;

const NAMES: [&str; 9] = [
    "Ancestral Recall",
    "Black Lotus",
    "Mox Emerald",
    "Mox Jet",
    "Mox Pearl",
    "Mox Ruby",
    "Mox Sapphire",
    "Time Walk",
    "Timetwister",
];
fn conjure(names: &[&str]) -> String {
    let phrases: Vec<_> = names.iter().map(|n| format!("a card named {n}")).collect();
    let (last, before) = phrases.split_last().unwrap();
    let list = if before.len() == 1 {
        format!("{} and {last}", before[0])
    } else {
        format!("{}, and {last}", before.join(", "))
    };
    format!("Conjure {list} into your library, then shuffle.")
}
fn operation<'a>(reading: &'a mut Value, kind: &str) -> &'a mut Value {
    reading[0]["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|i| i["args"]["operation"]["kind"] == kind)
        .unwrap()
        .get_mut("args")
        .unwrap()
        .get_mut("operation")
        .unwrap()
        .get_mut("args")
        .unwrap()
}
fn exact(reading: &Value, text: &str) -> Vec<String> {
    let types = vec!["Sorcery".to_string()];
    let key = engine()
        .trail_of_types("Context example", &types, reading, text)
        .unwrap()
        .expect("the recursive list retains exact wording");
    assert_eq!(
        engine()
            .print_trail_types("Context example", &types, reading, &key)
            .unwrap()
            .as_deref(),
        Some(text)
    );
    printed_types(reading, &["Sorcery"]);
    key
}
fn edited(reading: &Value, key: &[String]) {
    let normal = canonical_abilities(reading).unwrap();
    printed_types(&normal, &["Sorcery"]);
    if let Some(text) = engine()
        .print_trail_types("Context example", &["Sorcery".into()], &normal, key)
        .unwrap()
    {
        assert!(
            engine()
                .parse("Context example", &text)
                .unwrap()
                .readings
                .contains(&normal)
        );
    }
}

#[test]
fn named_conjure_lists_preserve_order_and_multiplicity_at_new_lengths() {
    for names in [
        NAMES.to_vec(),
        vec![NAMES[0], NAMES[1]],
        [NAMES.as_slice(), &NAMES[..3]].concat(),
    ] {
        let text = conjure(&names);
        let mut reading = compositional(&text);
        assert_eq!(
            operation(&mut reading, "Conjure")["card"]["Each"],
            json!(names)
        );
        let key = exact(&reading, &text);
        operation(&mut reading, "Conjure")["card"]["Each"] =
            json!(["Mox Ruby", "Ancestral Recall", "Mox Ruby"]);
        edited(&reading, &key);
    }
}

#[test]
fn creature_type_options_keep_each_operand_after_the_list_changes_length() {
    for text in [
        "Choose Angel, Demon, Devil, Dragon, Elf, Goblin, Soldier, or Zombie.",
        "Choose Elf, Goblin, or Merfolk.",
        "Choose Elf or Goblin.",
    ] {
        let mut reading = compositional(text);
        let key = exact(&reading, text);
        let args = operation(&mut reading, "ChooseValue");
        assert_eq!(args["kind"], "CreatureType");
        assert_eq!(
            args["terms"]["count"],
            json!({"Exactly":{"kind":"Constant","args":1}})
        );
        let options = args["terms"]["options"].as_array_mut().unwrap();
        assert_eq!(
            options.len(),
            if text.contains("Angel") {
                8
            } else if text.contains("Merfolk") {
                3
            } else {
                2
            }
        );
        options.truncate(2);
        options.swap(0, 1);
        options[0]["args"]["form"] = json!("Merfolk");
        edited(&reading, &key);
    }
}

#[test]
fn keyword_choice_counts_follow_the_list_and_keep_the_hexproof_family_slot() {
    let text = "Reveal the top seven cards of your library. Choose from among them a card with flying, a card with first strike, and so on for double strike, deathtouch, haste, hexproof, indestructible, lifelink, menace, reach, trample, and vigilance. Put one of the chosen cards onto the battlefield, the other chosen cards into your hand, and the rest into your graveyard.";
    let mut reading = compositional(text);
    let key = exact(&reading, text);
    let args = operation(&mut reading, "ChooseObjects");
    assert_eq!(args["count"], json!({"UpTo":{"kind":"Constant","args":12}}));
    let filters = args["constraints"][0]["OneForEach"].as_array_mut().unwrap();
    assert_eq!(filters.len(), 12);
    assert_eq!(
        filters[5],
        json!({"kind":"HasKeywordFamily","args":"Hexproof"})
    );
    assert!(
        filters
            .iter()
            .enumerate()
            .all(|(i, f)| i == 5 || f["kind"] == "HasKeyword")
    );
    filters.truncate(10);
    args["count"] = json!({"UpTo":{"kind":"Constant","args":10}});
    edited(&reading, &key);
}

#[test]
fn recursive_lists_consume_their_conjunction_and_the_entire_input() {
    for text in [
        "Choose Elf, Goblin, or.",
        "Choose Elf or Goblin. Incomplete list remainder.",
        "Conjure a card named Ancestral Recall and into your library, then shuffle.",
    ] {
        assert!(engine().parse("Context example", text).is_err(), "{text}");
    }
}
