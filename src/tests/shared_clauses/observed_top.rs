//! One inspected library top never names the source that did the looking.
use super::*;

#[test]
fn optional_top_moves_keep_the_inspected_card_and_its_library_owner() {
    for text in [
        "Look at the top card of target player's library. You may put that card into their graveyard.",
        "When this creature enters, look at the top card of target player's library. You may put that card into their graveyard.",
        "{2}, {T}: Look at the top card of target player's library. You may put that card into that player's graveyard.",
        "Whenever this creature attacks and isn't blocked, look at the top card of defending player's library. You may put that card on the bottom of that player's library.",
    ] {
        let frame = compositional(text);
        let body = &frame[0]["args"]["body"]["instructions"];
        assert_eq!(body.as_array().unwrap().len(), 2);
        let inspected = &body[0]["args"]["operation"]["args"]["objects"];
        let moved =
            &body[1]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"];
        assert_eq!(inspected, moved, "{text}");
        assert_eq!(moved["kind"], "Take");
        assert_eq!(moved["args"]["count"], json!({"kind":"Constant","args":1}));
        assert_eq!(
            moved["args"]["objects"]["args"]["scope"]["Players"]["zone"],
            "Library"
        );
    }
}

#[test]
fn inspected_top_move_prints_edited_owner_eligibility_and_destination() {
    let mut frame = compositional(
        "Look at the top card of target player's library. You may put that card into their graveyard.",
    );
    frame[0]["args"]["announcement"]["targets"][0]["players"] =
        json!({"kind":"OpponentsOf","args":{"kind":"AbilityController"}});
    frame[0]["args"]["body"]["instructions"][1]["args"]["body"]["instructions"][0]["args"]["operation"]
        ["args"]["destination"] = json!({"Library":{
        "position":"Bottom","order":"DestinationDefault","arrival":{"counters":[],"states":[]}
    }});
    let text = printed_types(&frame, &["Sorcery"]).to_lowercase();
    assert!(
        text.contains("opponent") && text.contains("bottom"),
        "{text}"
    );
}

#[test]
fn an_explicit_source_after_inspection_keeps_its_own_identity() {
    let frame = compositional(
        "Look at the top card of target player's library. You may put this card into its owner's graveyard.",
    );
    let mut moves = Vec::new();
    visit(&frame, "Move", &mut moves);
    assert_eq!(moves.len(), 1);
    assert_eq!(
        moves[0]["objects"],
        json!({"kind":"One","args":{"kind":"Source"}})
    );
}
