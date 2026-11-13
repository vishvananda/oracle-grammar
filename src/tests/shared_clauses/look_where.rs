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

#[test]
fn look_value_is_shared_with_a_battlefield_selection_filter() {
    let text = "Whenever this creature attacks, look at the top X cards of your library, where X is its power. You may put a permanent card with mana value X or less from among them onto the battlefield. Put the rest on the bottom of your library in a random order.";
    let reading = compositional(text);
    let instructions = &reading[0]["args"]["body"]["instructions"];
    let bound = json!({"kind":"BoundValue","args":instructions[0]["args"]["result"]});
    assert_eq!(
        instructions[1]["args"]["operation"]["args"]["objects"]["args"]["count"],
        bound
    );
    let mut quantities = Vec::new();
    visit(&instructions[2], "BoundValue", &mut quantities);
    assert_eq!(quantities, vec![instructions[0]["args"]["result"].clone()]);
    assert_eq!(
        instructions[3]["args"]["operation"]["args"]["destination"]["Battlefield"]["controller"],
        json!({"Player":{"kind":"AbilityController"}})
    );
    printed_types(&reading, &["Creature"]);
}

#[test]
fn look_value_stays_captured_through_reordering_and_a_later_comparison() {
    let text = "Look at the top X cards of your library, where X is the number of cards in your hand. Put up to one of them on top of your library and the rest on the bottom of your library in a random order. If X is greater than or equal to the number of cards in your library, you win the game.";
    let reading = compositional(text);
    let instructions = &reading[0]["args"]["body"]["instructions"];
    assert_eq!(instructions[0]["args"]["operation"]["kind"], "BindValue");
    assert_eq!(instructions[5]["kind"], "If");
    assert_eq!(
        instructions[5]["args"]["condition"]["args"]["left"],
        json!({"kind":"BoundValue","args":instructions[0]["args"]["result"]})
    );
    assert_eq!(
        instructions[5]["args"]["condition"]["args"]["test"]["args"]["value"]["kind"],
        "CountObjects"
    );
    printed_types(&reading, &["Sorcery"]);
}

#[test]
fn look_cast_filters_keep_the_casting_event_and_the_library_remainder() {
    let text = "Whenever you cast a Kraken, Leviathan, Octopus, or Serpent spell from your hand, look at the top X cards of your library, where X is that spell's mana value. You may cast a spell with mana value less than X from among them without paying its mana cost. Put the rest on the bottom of your library in a random order.";
    let reading = compositional(text);
    let body = &reading[0]["args"]["body"]["instructions"];
    assert_eq!(
        body[0]["args"]["operation"]["args"]["value"]["args"]["object"],
        json!({"kind":"EventSubject"})
    );
    assert_eq!(body[2]["args"]["operation"]["kind"], "PlayWith");
    assert_eq!(
        body[2]["args"]["operation"]["args"]["cards"]["args"]["filter"]["args"]["test"]["args"]["value"],
        json!({"kind":"BoundValue","args":body[0]["args"]["result"]})
    );
    assert_eq!(
        body[3]["args"]["operation"]["args"]["objects"],
        json!({"kind":"InZone","args":{"objects":{"kind":"Observed","args":body[1]["args"]["result"]},"zone":"Library"}})
    );
    printed_types(&reading, &["Creature"]);
}

#[test]
fn optional_look_declines_the_whole_top_bottom_selection() {
    let text = "Whenever a creature you control enters, you may look at the top X cards of your library, where X is that creature's power. If you do, put one of those cards on top of your library and the rest on the bottom of your library in any order.";
    let reading = compositional(text);
    let instructions = &reading[0]["args"]["body"]["instructions"];
    assert_eq!(instructions.as_array().unwrap().len(), 1);
    assert_eq!(instructions[0]["kind"], "Optional");
    assert_eq!(
        instructions[0]["args"]["body"]["instructions"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let printed = printed_types(&reading, &["Enchantment"]);
    assert!(printed.contains(". If you do,"), "{printed}");
    let unguarded = text.replace(". If you do, put", ". Put");
    if let Ok(parsed) = engine().parse("Context example", &unguarded) {
        assert!(
            !parsed.readings.contains(&reading),
            "a later mandatory sentence is not part of the option"
        );
    }
}

#[test]
fn look_exile_cast_preserves_the_exile_receipt_after_reordering() {
    let text = "Whenever this creature attacks, look at the top X cards of your library, where X is this creature's power. You may exile an instant or sorcery card with mana value X or less from among them. Put the rest on the bottom of your library in a random order. You may cast the exiled card without paying its mana cost.";
    let mut reading = compositional(text);
    let body = &reading[0]["args"]["body"]["instructions"];
    assert_eq!(body[0]["args"]["operation"]["kind"], "BindValue");
    assert_eq!(
        body[3]["args"]["operation"]["args"]["destination"]["Exile"]["visibility"],
        "FaceUp"
    );
    assert_eq!(body[4]["args"]["operation"]["kind"], "ReorderLibrary");
    assert_eq!(body[5]["kind"], "Optional");
    let mut results = Vec::new();
    visit(&body[5], "Result", &mut results);
    assert_eq!(
        results,
        vec![json!({"result":body[3]["args"]["result"],"phase":"After","index":0})]
    );
    printed_types(&reading, &["Creature"]);
    reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["value"]["args"]["property"] =
        json!("Toughness");
    assert!(printed_types(&reading, &["Creature"]).contains("toughness"));
}

#[test]
fn backup_look_quantity_is_captured_separately_in_each_holder() {
    let text = "Backup 3\nWhenever this creature attacks, look at the top X cards of your library, where X is its power. You may put a permanent card with mana value X or less from among them onto the battlefield. Put the rest on the bottom of your library in a random order.";
    let reading = compositional(text);
    for trigger in [&reading[0]["args"]["abilities"][0], &reading[1]] {
        let body = &trigger["args"]["body"]["instructions"];
        assert_eq!(body[0]["args"]["operation"]["kind"], "BindValue");
        assert_eq!(
            body[0]["args"]["operation"]["args"]["value"]["args"]["object"],
            json!({"kind":"Source"})
        );
        assert_eq!(
            body[1]["args"]["operation"]["args"]["objects"]["args"]["count"],
            json!({"kind":"BoundValue","args":body[0]["args"]["result"]})
        );
    }
    printed_types(&reading, &["Creature"]);
}

#[test]
fn one_sentence_optional_look_then_selection_keeps_its_explicit_scope() {
    for join in [", then", " and"] {
        let text = format!(
            "You may look at the top three cards of your library{join} put one of them into your hand and the rest on the bottom of your library in any order."
        );
        let reading = compositional(&text);
        let instructions = &reading[0]["args"]["body"]["instructions"];
        assert_eq!(instructions.as_array().unwrap().len(), 1);
        assert_eq!(instructions[0]["kind"], "Optional");
        assert_eq!(
            instructions[0]["args"]["body"]["instructions"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        printed_types(&reading, &["Sorcery"]);
    }
}
