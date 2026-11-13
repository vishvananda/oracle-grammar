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
    // The zone already bounds the pool; one random card repeats no zone filter.
    assert_eq!(pool["filter"], json!({"kind":"Any"}));
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

fn legacy_suspect_operation(operation: &mut Value) {
    assert_eq!(operation["kind"], "KeywordEach");
    let objects = &operation["args"]["objects"];
    let object = if objects["kind"] == "Targets" {
        json!({"kind":"Target","args":{"slot":objects["args"],"index":0}})
    } else {
        assert_eq!(objects["kind"], "One");
        objects["args"].clone()
    };
    *operation = json!({"kind":"Keyword","args":{
        "actor":operation["args"]["actor"],"action":{"Suspect":{"object":object}}
    }});
}

fn legacy_suspect(mut reading: Value, at: usize) -> Value {
    legacy_suspect_operation(
        &mut reading[0]["args"]["body"]["instructions"][at]["args"]["operation"],
    );
    reading
}

#[test]
fn suspect_has_one_shape_and_preserves_legacy_actor_and_object_operands() {
    let reading = compositional("Target player draws a card. Suspect target creature.");
    let mut legacy = legacy_suspect(reading.clone(), 1);
    validate_abilities(&legacy).unwrap();
    assert_eq!(canonical_abilities(&legacy).unwrap(), reading);
    printed_types(&legacy, &["Sorcery"]);
    let actor = json!({"kind":"One","args":{"kind":"Target","args":0}});
    legacy[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["actor"] =
        actor.clone();
    let normalized = canonical_abilities(&legacy).unwrap();
    assert_eq!(operation(&normalized, 1)["args"]["actor"], actor);
    assert_eq!(
        operation(&normalized, 1)["args"]["objects"],
        operation(&reading, 1)["args"]["objects"]
    );
    assert_eq!(canonical_abilities(&normalized).unwrap(), normalized);
    let singular = compositional("Suspect this creature.");
    let mut edited = legacy_suspect(singular, 0);
    edited[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["action"]["Suspect"]
        ["object"] = json!({"kind":"AttachedTo","args":{"kind":"Source"}});
    let text = printed_types(&edited, &["Enchantment", "Aura"]);
    assert!(text.to_lowercase().contains("enchanted"), "{text}");
}

#[test]
fn suspect_normalization_preserves_invalid_extra_operands_for_validation() {
    let reading = compositional("Suspect this creature.");
    for path in [
        "/0/args/body/instructions/0/args/operation",
        "/0/args/body/instructions/0/args/operation/args",
        "/0/args/body/instructions/0/args/operation/args/action",
        "/0/args/body/instructions/0/args/operation/args/action/Suspect",
    ] {
        let mut legacy = legacy_suspect(reading.clone(), 0);
        legacy.pointer_mut(path).unwrap()["extra"] = json!({"kind":"GrantingObject"});
        assert!(validate_abilities(&legacy).is_err());
        crate::model::canonical::abilities(&mut legacy).unwrap();
        assert_eq!(operation(&legacy, 0)["kind"], "Keyword");
        assert!(legacy.pointer(path).unwrap().get("extra").is_some());
        assert!(validate_abilities(&legacy).is_err());
    }
}

#[test]
fn suspect_normalization_maps_the_original_target_id_to_its_new_position() {
    use crate::model::{canonical, ir};
    for (text, destination) in [
        ("Suspect target creature.", "/objects/args/args/slot"),
        ("Suspect up to one target creature.", "/objects/args"),
    ] {
        let reading = compositional(text);
        let legacy = legacy_suspect(reading, 0);
        let card: ir::UnifiedCard = serde_json::from_value(json!({
            "schema_version":ir::SCHEMA_VERSION,"layout":"Card","data":[],"faces":[{
                "schema_version":ir::SCHEMA_VERSION,"name":"Context example",
                "characteristics":{"supertypes":[],"card_types":["Sorcery"],"subtypes":[],
                    "mana_cost":null,"color_indicator":[],"power_toughness":null,"loyalty":null,
                    "defense":null},"abilities":legacy
            }]
        }))
        .unwrap();
        let mapped = canonical::mapped_unified(&card).unwrap();
        let prefix = "/faces/0/abilities/0/args/body/instructions/0/args/operation/args";
        assert_eq!(
            mapped.map.sites[&format!("{prefix}/action/Suspect/object/args/slot")],
            [format!("{prefix}{destination}")]
        );
        assert!(mapped.map.introduced.is_empty());
        assert_eq!(
            serde_json::to_value(mapped.card).unwrap(),
            serde_json::to_value(canonical::unified(&card).unwrap()).unwrap()
        );
    }
}

#[test]
fn optional_suspect_targets_keep_their_entire_announced_population() {
    let reading = compositional("Suspect up to one target creature.");
    let legacy = legacy_suspect(reading.clone(), 0);
    assert_eq!(canonical_abilities(&legacy).unwrap(), reading);
    assert_eq!(
        printed_types(&legacy, &["Sorcery"]),
        "Suspect up to one target creature."
    );
}

#[test]
fn modal_suspect_actions_keep_optional_and_mandatory_target_scopes() {
    let reading = compositional(
        "Choose one —\n• Suspect up to one target creature.\n• Suspect target creature.",
    );
    let mut legacy = reading.clone();
    for option in legacy[0]["args"]["announcement"]["modes"]["options"]
        .as_array_mut()
        .unwrap()
    {
        legacy_suspect_operation(&mut option["body"]["instructions"][0]["args"]["operation"]);
    }
    assert_eq!(canonical_abilities(&legacy).unwrap(), reading);
    printed_types(&legacy, &["Sorcery"]);
}

#[test]
fn suspecting_a_created_token_uses_the_batch_without_aliasing_its_first_member() {
    let reading = compositional("Create a 2/1 black Skeleton creature token. Suspect it.");
    let produced = reading[0]["args"]["body"]["instructions"][0]["args"]["result"].clone();
    assert_eq!(
        operation(&reading, 1)["args"]["objects"],
        json!({"kind":"Created","args":produced})
    );
    printed_types(&reading, &["Sorcery"]);
    let mut first_only = reading.clone();
    first_only[0]["args"]["body"]["instructions"][1]["args"]["operation"] = json!({"kind":"Keyword","args":{
        "actor":{"kind":"One","args":{"kind":"AbilityController"}},
        "action":{"Suspect":{"object":{"kind":"Result","args":{
            "result":produced,"phase":"After","index":0
        }}}}
    }});
    validate_abilities(&first_only).unwrap();
    let normalized = canonical_abilities(&first_only).unwrap();
    assert_ne!(normalized, reading, "a batch cannot alias its first token");
    assert_eq!(operation(&normalized, 1)["args"]["objects"]["kind"], "One");
    assert!(
        engine()
            .print_keyed("Context example", &normalized, &["Sorcery".into()], &[])
            .unwrap()
            .is_none(),
        "the batch wording cannot claim to print a single produced object"
    );
}

#[test]
fn goad_duration_keeps_its_permanent_separate_from_its_recipients() {
    let mut reading = compositional(
        "When this creature enters, suspect target creature an opponent controls. As long \
         as this creature remains on the battlefield, that creature is also goaded.",
    );
    let suspect = operation(&reading, 0);
    let goad = operation(&reading, 1);
    assert_eq!(suspect["args"]["objects"], goad["args"]["objects"]);
    assert_eq!(
        goad["args"]["action"]["Goad"]["until"],
        json!({"WhileOnBattlefield":{"kind":"Source"}})
    );
    printed(&reading);
    reading[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["action"]["Goad"]
        ["until"]["WhileOnBattlefield"] = json!({"kind":"AttachedTo","args":{"kind":"Source"}});
    let text = printed_types(&reading, &["Enchantment", "Aura"]);
    assert!(
        text.contains("enchanted") && text.contains("remains"),
        "{text}"
    );
}

#[test]
fn a_suspected_condition_names_the_holder_or_creator_that_was_written() {
    for (subject, object) in [
        ("this creature", "Source"),
        ("Context example", "GrantingObject"),
    ] {
        let text = format!(
            "Target creature gains \"At the beginning of your upkeep, if {subject} is not \
             suspected, draw a card.\" until end of turn."
        );
        let reading = compositional(&text);
        let mut tests = Vec::new();
        visit(&reading, "ObjectMatches", &mut tests);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0]["object"], json!({"kind":object}));
        assert_eq!(
            tests[0]["filter"],
            json!({"kind":"Not","args":{"kind":"State","args":"Suspected"}})
        );
        printed(&reading);
    }
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
