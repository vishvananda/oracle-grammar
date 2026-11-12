//! Normal forms the reference settled on for one meaning: an optional target is its
//! population, a random selection is one move, a self-trigger's closure names the source,
//! a conniving object's controller connives, a free condition is not an object test, and
//! a card put into a hand is duplicated as it last existed.
use super::*;

fn operation(reading: &Value, at: usize) -> &Value {
    &reading[0]["args"]["body"]["instructions"][at]["args"]["operation"]
}

#[test]
fn an_optional_target_is_its_whole_population_in_any_operation() {
    for (text, field) in [
        ("Destroy up to one target creature.", "/args/objects"),
        ("Exile up to one target creature.", "/args/objects"),
        (
            "Up to one target creature can't block this turn.",
            "/args/affected/Objects",
        ),
    ] {
        let reading = compositional(text);
        let objects = operation(&reading, 0).pointer(field).unwrap();
        assert_eq!(objects, &json!({"kind":"Targets","args":0}), "{text}");
        assert_eq!(printed_types(&reading, &["Sorcery"]), text);
    }
    let mandatory = compositional("Destroy target creature.");
    let objects = &operation(&mandatory, 0)["args"]["objects"];
    assert_eq!(objects["kind"], "One");
}

#[test]
fn cards_at_random_are_one_move_of_a_chance_selection() {
    let text = "Return two cards at random from your graveyard to your hand.";
    let reading = compositional(text);
    let selection = operation(&reading, 0);
    assert_eq!(selection["kind"], "WithSelections");
    let step = &selection["args"]["steps"][0];
    assert_eq!(step["chooser"], "Chance");
    assert_eq!(
        step["count"],
        json!({"AsManyAsCan":{"kind":"Constant","args":2}})
    );
    assert_eq!(step["objects"]["args"]["filter"], json!({"kind":"Any"}));
    let action = &selection["args"]["action"]["args"];
    assert_eq!(action["objects"], json!({"kind":"Prepared","args":0}));
    let printed = printed_types(&reading, &["Sorcery"]);
    assert!(printed.ends_with("two cards at random from your graveyard to your hand."));
    let single = compositional("Return a card at random from your graveyard to your hand.");
    let pool = &operation(&single, 0)["args"]["steps"][0]["objects"]["args"];
    assert_eq!(pool["filter"]["kind"], "InZone");
}

#[test]
fn a_closure_under_the_sources_own_trigger_names_the_source() {
    let text = "Whenever this creature attacks, you may sacrifice an artifact. When you do, \
                target creature defending player controls gets -2/-2 until end of turn.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert!(!readings.is_empty());
    for reading in readings {
        let printed = reading.to_string();
        assert!(
            printed.contains(r#""DefendingPlayerOf","args":{"kind":"Source"}"#),
            "{printed}"
        );
        assert!(!printed.contains("EventSubject"), "{printed}");
    }
}

#[test]
fn a_conniving_object_is_the_one_its_controller_connives_with() {
    let reading = compositional("When this creature enters, it connives.");
    let keyword = &operation(&reading, 0)["args"];
    assert_eq!(
        keyword["actor"],
        json!({"kind":"One","args":{"kind":"ControllerOf","args":{"kind":"Source"}}})
    );
    assert_eq!(
        keyword["action"]["Connive"]["object"],
        json!({"kind":"Source"})
    );
}

#[test]
fn a_mana_spent_condition_is_not_an_object_test() {
    let text =
        "When this creature enters, if at least five mana was spent to cast it, draw a card.";
    let reading = compositional(text);
    let condition = &reading[0]["args"]["intervening_if"];
    assert_eq!(condition["kind"], "Compare");
    assert_eq!(condition["args"]["left"]["kind"], "ManaSpent");
    assert!(printed(&reading).contains("mana was spent to cast"));
}

#[test]
fn a_card_put_into_a_hand_is_duplicated_as_it_last_existed() {
    let text = "Whenever a card in your graveyard is put into your hand, \
                conjure a duplicate of that card into your hand.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{readings:#?}");
    let card = &operation(&readings[0], 0)["args"]["card"];
    assert_eq!(card, &json!({"DuplicateLastKnown":{"kind":"EventSubject"}}));
    // The printer's tree for "that card" here does not bind, though the text reparses;
    // printing this reading is a separate gap.
}
