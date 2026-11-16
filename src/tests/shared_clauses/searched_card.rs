//! Search choices and conditional payment scopes retain every actual effect.
use super::*;

#[test]
fn a_searched_card_never_falls_back_to_the_source() {
    let frame = compositional(
        "Search your library for a card, put that card into your hand, then shuffle.",
    );
    let body = &frame[0]["args"]["body"]["instructions"];
    assert_eq!(body.as_array().unwrap().len(), 3);
    assert_eq!(body[0]["args"]["operation"]["kind"], "Search");
    assert_eq!(
        body[1]["args"]["operation"]["args"]["objects"],
        json!({"kind":"Selected","args":body[0]["args"]["result"]})
    );
    assert_eq!(body[2]["args"]["operation"]["kind"], "Shuffle");
}

#[test]
fn declining_all_mana_payments_keeps_the_complete_search_branch() {
    let frame = compositional(
        "Unless any player pays {2}, search your library for a card, put that card into your hand, then shuffle.",
    );
    let body = &frame[0]["args"]["body"]["instructions"];
    assert_eq!(body.as_array().unwrap().len(), 2);
    assert_eq!(
        body[0]["args"]["operation"]["args"]["player"],
        json!({"kind":"FirstWilling","args":{"kind":"All"}})
    );
    assert_eq!(
        body[1]["args"]["condition"],
        json!({"kind":"Not","args":{
        "kind":"PaymentChosen","args":body[0]["args"]["result"]}})
    );
    let branch = &body[1]["args"]["then"]["instructions"];
    let operations: Vec<_> = branch
        .as_array()
        .unwrap()
        .iter()
        .map(|step| step["args"]["operation"]["kind"].as_str().unwrap())
        .collect();
    assert_eq!(operations, ["Search", "Move", "Shuffle"]);
    assert_eq!(
        branch[1]["args"]["operation"]["args"]["objects"],
        json!({"kind":"Selected","args":branch[0]["args"]["result"]})
    );
}

#[test]
fn complete_search_continuations_keep_only_the_actual_native_card() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/searched-card-continuations.json"
    ))
    .unwrap();
    for case in data["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let parsed = engine().parse(name, text).unwrap();
        assert_eq!(
            parsed.readings,
            vec![canonical_abilities(&case["expected"]).unwrap()],
            "{name}"
        );
    }
}

#[test]
fn subtype_search_lists_print_their_edited_members() {
    let mut frame = compositional(
        "Search your library for a Plains, Island, Swamp, Mountain, or Forest card and put that card onto the battlefield. Then shuffle.",
    );
    frame[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["choices"][0]["filter"] = json!({"kind":"AnyOf","args":[
        {"kind":"Subtype","args":"Elf"},
        {"kind":"Subtype","args":"Goblin"},
        {"kind":"Subtype","args":"Merfolk"}
    ]});
    let text = printed_types(&frame, &["Sorcery"]);
    for word in ["Elf", "Goblin", "Merfolk"] {
        assert!(text.contains(word), "{text}");
    }
    assert!(
        !text.contains("Forest") && !text.contains("Island"),
        "{text}"
    );
}
