//! A search names where it looks and what it finds; an `and/or` search shuffles only
//! if it looked through the library.
use super::*;
use crate::parse::RuleSet;

fn one_reading(text: &str) -> Value {
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{text}: {readings:#?}");
    let audited = engine()
        .audited("Context example", text, &RuleSet::default())
        .unwrap();
    assert!(
        audited.iter().any(|(value, rules)| {
            canonical_abilities(value).unwrap() == readings[0] && rules.iter().all(Vec::is_empty)
        }),
        "{text} needs a seeded rule: {audited:?}"
    );
    readings[0].clone()
}

fn search(reading: &Value) -> &Value {
    let first = &reading[0]["args"]["body"]["instructions"][0]["args"]["operation"];
    assert_eq!(first["kind"], "Search");
    &first["args"]
}

#[test]
fn an_either_search_shuffles_only_if_it_searched_the_library() {
    let text = "Search your library and/or graveyard for a card named Jiang Yanggu, reveal it, \
                and put it into your hand. If you search your library this way, shuffle.";
    let mut reading = one_reading(text);
    let found = search(&reading);
    assert_eq!(found["also"], json!(["Graveyard"]));
    assert_eq!(found["either"], json!(true));
    assert_eq!(
        found["choices"][0]["filter"],
        json!({"kind":"Name","args":"Jiang Yanggu"})
    );
    let last = &reading[0]["args"]["body"]["instructions"][3];
    assert_eq!(last["kind"], "If");
    assert_eq!(last["args"]["condition"]["kind"], "Searched");
    assert_eq!(printed_types(&reading, &["Sorcery"]), text);
    let choice = &mut reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["choices"]
        [0]["filter"]["args"];
    *choice = json!("Mu Yanling");
    let edited = printed_types(&reading, &["Sorcery"]);
    assert!(edited.contains("a card named Mu Yanling,"), "{edited}");
}

#[test]
fn a_search_of_library_and_graveyard_always_shuffles() {
    let text = "Search your library and graveyard for a card named Nissa, Nature's Artisan, \
                reveal it, put it into your hand, then shuffle.";
    let reading = one_reading(text);
    let found = search(&reading);
    assert_eq!(found["also"], json!(["Graveyard"]));
    assert_ne!(found["either"], json!(true));
    let instructions = reading[0]["args"]["body"]["instructions"]
        .as_array()
        .unwrap();
    assert_eq!(
        instructions.last().unwrap()["args"]["operation"]["kind"],
        "Shuffle"
    );
    assert_eq!(printed_types(&reading, &["Sorcery"]), text);
}
