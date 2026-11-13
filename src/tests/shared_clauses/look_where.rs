//! A look definition scopes later selections without losing observed identities.
use super::*;

#[test]
fn observed_where_values_compose_with_hand_selection_and_reveal() {
    for tail in [
        "Put one of those cards into your hand and the rest on the bottom of your library in a random order.",
        "You may reveal a creature card from among them and put it into your hand. Put the rest on the bottom of your library in any order.",
    ] {
        let text = format!(
            "Look at the top X cards of your library, where X is the number of artifacts you control. {tail}"
        );
        let reading = compositional(&text);
        assert!(!reading.to_string().contains("AnnouncedX"));
        assert!(!reading.to_string().contains("BindValue"));
        let body = &reading[0]["args"]["body"]["instructions"];
        let look = body[0]["args"]["result"].clone();
        assert_eq!(
            body[1]["args"]["operation"]["args"]["objects"]["kind"],
            if tail.starts_with("Put") {
                "InZone"
            } else {
                "Matching"
            }
        );
        let mut observed = Vec::new();
        visit(&reading, "Observed", &mut observed);
        assert!(!observed.is_empty());
        assert!(observed.iter().all(|id| *id == look));
        printed_types(&reading, &["Sorcery"]);
    }
}

#[test]
fn repeated_observed_x_is_captured_once_and_preserves_card_receipts() {
    for pronoun in ["them", "those cards"] {
        let text = format!(
            "Look at the top X cards of your library, where X is the number of cards in your hand. Put X of {pronoun} into your hand and the rest on the bottom of your library in a random order."
        );
        let mut reading = compositional(&text);
        let body = &reading[0]["args"]["body"]["instructions"];
        let kinds: Vec<_> = body
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["args"]["operation"]["kind"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            [
                "BindValue",
                "LookAt",
                "ChooseObjects",
                "Move",
                "ReorderLibrary"
            ]
        );
        let bound = json!({"kind":"BoundValue","args":body[0]["args"]["result"]});
        assert_eq!(
            body[1]["args"]["operation"]["args"]["objects"]["args"]["count"],
            bound
        );
        assert_eq!(
            body[2]["args"]["operation"]["args"]["count"]["AsManyAsCan"],
            bound
        );
        assert_eq!(
            body[2]["args"]["operation"]["args"]["objects"]["args"]["objects"],
            json!({"kind":"Observed","args":body[1]["args"]["result"]})
        );
        assert_eq!(
            body[3]["args"]["operation"]["args"]["objects"],
            json!({"kind":"Selected","args":body[2]["args"]["result"]})
        );
        let rest = &body[4]["args"]["operation"]["args"]["objects"]["args"]["objects"]["args"];
        assert_eq!(
            rest["objects"],
            json!({"kind":"Observed","args":body[1]["args"]["result"]})
        );
        assert_eq!(
            rest["excluding"]["args"][0]["args"]["result"],
            body[3]["args"]["result"]
        );
        printed_types(&reading, &["Sorcery"]);
        reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["value"]["args"]
            ["args"]["scope"]["Players"]["zone"] = json!("Graveyard");
        let edited = printed_types(&reading, &["Sorcery"]);
        assert!(edited.contains("graveyard"), "{edited}");
    }
}

#[test]
fn observed_shared_property_is_captured_before_looking_and_cannot_move() {
    use crate::pattern::{Bindings, Pattern};
    let text = "Whenever this creature attacks, look at the top X cards of your library, where X is this creature's power. Put X of them into your hand and the rest on the bottom of your library in a random order.";
    let reading = compositional(text);
    let body = &reading[0]["args"]["body"];
    assert_eq!(
        body["instructions"][0]["args"]["operation"]["args"]["value"],
        json!({"kind":"ObjectProperty","args":{"object":{"kind":"Source"},"property":"Power","information":"CurrentOrLastKnown"}})
    );
    printed_types(&reading, &["Creature"]);
    let holes = [
        Pattern::Hole("program".into(), crate::model::ir_schema::Ty::Opaque),
        Pattern::Hole("value".into(), crate::model::ir_schema::Ty::Opaque),
    ];
    let inverse =
        |body: &Value| crate::functions::value_snapshot::unify(&holes, body, Bindings::new()).len();
    assert_eq!(inverse(body), 1);
    let mut moved = body.clone();
    moved["instructions"].as_array_mut().unwrap().swap(0, 1);
    assert_eq!(inverse(&moved), 0);
}
