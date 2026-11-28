use crate::{ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"[{"kind": "DeckRule", "args": {"CommanderWithCreatedToken": {"name": "Jimothy", "chapter": 1}}}, {"kind": "WithPresentation", "args": {"label": null, "reminder": true, "ability": {"kind": "Saga", "args": {"chapters": [{"kind": "Triggered", "args": {"event": {"SagaChapter": 1}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "CreateTokens", "args": {"players": {"kind": "One", "args": {"kind": "AbilityController"}}, "tokens": [{"amount": {"kind": "Constant", "args": 1}, "words": "CreatingEffect", "recipe": {"Defined": {"name": {"Explicit": "Jimothy"}, "colors": ["Green"], "chosen_color": false, "chosen_creature_type": false, "supertypes": ["Legendary"], "card_types": ["Creature"], "subtypes": ["Raccoon"], "power_toughness": [{"kind": "Constant", "args": 2}, {"kind": "Constant", "args": 1}], "abilities": [{"kind": "Triggered", "args": {"event": {"ZoneChange": {"objects": {"kind": "OwnedBy", "args": {"kind": "One", "args": {"kind": "AbilityController"}}}, "from": "Library", "to": "Graveyard"}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "PutCounters", "args": {"placements": [{"recipients": {"kind": "Objects", "args": {"kind": "One", "args": {"kind": "Source"}}}, "kind": {"PowerToughness": [1, 1]}, "amount": {"kind": "Constant", "args": 1}, "selection": null}]}}}}]}}}], "abilities_of": null}}}], "entry": {"controller": null, "tapped": false, "counters": [], "states": []}}}}}]}}}, {"kind": "Triggered", "args": {"event": {"SagaChapter": 2}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "Surveil", "args": {"players": {"kind": "One", "args": {"kind": "AbilityController"}}, "amount": {"kind": "Constant", "args": 1}}}}}]}}}, {"kind": "Triggered", "args": {"event": {"SagaChapter": 3}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "Surveil", "args": {"players": {"kind": "One", "args": {"kind": "AbilityController"}}, "amount": {"kind": "Constant", "args": 1}}}}}]}}}]}}}}]"###).unwrap()
}
fn checked(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn token_commander_points_to_the_created_chapter_token() {
    let a = source();
    for x in a.as_array().unwrap() {
        assert!(checked(x.clone()));
    }
    let designation = &a[0]["args"]["CommanderWithCreatedToken"];
    assert_eq!(designation, &json!({"name":"Jimothy","chapter":1}));
    let chapters = &a[1]["args"]["ability"]["args"]["chapters"];
    assert_eq!(chapters[0]["args"]["event"], json!({"SagaChapter":1}));
    let create = &chapters[0]["args"]["body"]["instructions"][0]["args"]["operation"];
    assert_eq!(create["kind"], "CreateTokens");
    let token = &create["args"]["tokens"][0]["recipe"]["Defined"];
    assert_eq!(token["name"], json!({"Explicit":"Jimothy"}));
    assert_eq!(token["supertypes"], json!(["Legendary"]));
    assert_eq!(token["subtypes"], json!(["Raccoon"]));
    assert_eq!(token["colors"], json!(["Green"]));
    let triggered = &token["abilities"][0]["args"];
    assert_eq!(triggered["event"]["ZoneChange"]["from"], "Library");
    assert_eq!(triggered["event"]["ZoneChange"]["to"], "Graveyard");
    let recipients = &triggered["body"]["instructions"][0]["args"]["operation"]["args"]["placements"]
        [0]["recipients"];
    assert_eq!(
        recipients,
        &json!({"kind":"Objects","args":{"kind":"One","args":{"kind":"Source"}}})
    );
    assert_eq!(chapters[1]["args"]["event"], json!({"SagaChapter":2}));
    assert_eq!(chapters[2]["args"]["event"], json!({"SagaChapter":3}));
}
#[test]
fn token_designation_is_not_a_named_deck_card_partner() {
    let a = source();
    assert!(a[0]["args"].get("CommanderPair").is_none());
    let pair:Ability=serde_json::from_value(json!({"kind":"DeckRule","args":{"CommanderPair":{"grants_source_eligibility":true,"partner":{"NamedCard":"Jimothy"}}}})).unwrap();
    assert_ne!(serde_json::to_value(pair).unwrap(), a[0]);
}
