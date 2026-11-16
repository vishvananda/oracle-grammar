//! Library splits preserve the observation or search that supplied the cards.
use super::*;

#[test]
fn looked_at_and_searched_cards_keep_distinct_receipts() {
    let look = compositional(
        "Look at the top two cards of your library. Put one into your hand and the other into your graveyard.",
    );
    let body = &look[0]["args"]["body"]["instructions"];
    let observed = json!({"kind":"Observed","args":body[0]["args"]["result"]});
    assert_eq!(
        body[1]["args"]["operation"]["args"]["objects"],
        json!({"kind":"InZone","args":{"objects":observed,"zone":"Library"}})
    );
    let rest = &body[3]["args"]["operation"]["args"]["objects"]["args"]["objects"]["args"];
    assert_eq!(rest["objects"], observed);
    assert_eq!(
        rest["excluding"]["args"][0],
        json!({"kind":"Transition","args":{"result":body[2]["args"]["result"],"phase":"Before"}})
    );

    for text in [
        "Search your library for two cards. Put one into your hand and the other into your graveyard. Then shuffle.",
        "Search your library for up to two creature cards and reveal them. Put one into your hand and the other into your graveyard. Then shuffle.",
    ] {
        let search = compositional(text);
        let body = &search[0]["args"]["body"]["instructions"];
        let selected = json!({"kind":"Selected","args":body[0]["args"]["result"]});
        let mut choices = Vec::new();
        visit(&search, "ChooseObjects", &mut choices);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0]["objects"], selected);
        let mut moves = Vec::new();
        visit(&search, "Move", &mut moves);
        assert_eq!(moves.len(), 2);
        assert_eq!(moves[1]["objects"], selected);
        assert_ne!(moves[0]["objects"], selected);
        printed_types(&search, &["Sorcery"]);
    }
}

#[test]
fn look_split_prints_changed_observation_and_selection_counts() {
    let mut frame = compositional(
        "Look at the top two cards of your library. Put one into your hand and the other into your graveyard.",
    );
    let body = &mut frame[0]["args"]["body"]["instructions"];
    body[0]["args"]["operation"]["args"]["objects"]["args"]["count"] =
        json!({"kind":"Constant","args":3});
    body[1]["args"]["operation"]["args"]["count"] =
        json!({"AsManyAsCan":{"kind":"Constant","args":2}});
    let text = printed_types(&frame, &["Sorcery"]).to_lowercase();
    assert!(text.contains("three") || text.contains('3'), "{text}");
    assert!(text.contains("two") || text.contains('2'), "{text}");
}
