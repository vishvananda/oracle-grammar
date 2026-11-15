//! Reported reference corrections (#20, #23-#28) parse to their corrected meaning alone.
use super::*;
use crate::engine::semantic_abilities;

fn cases() -> Vec<Value> {
    let value: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/reported-corrections-20261114.json"
    ))
    .unwrap();
    value["cases"].as_array().unwrap().clone()
}

/// The only reading of a fixture face, after checking it means the expected IR
/// (presentation erased, as the corpus score compares references) and prints
/// back to the written text.
fn reading(name: &str) -> Value {
    let case = cases().into_iter().find(|c| c["name"] == name).unwrap();
    let text = case["text"].as_str().unwrap();
    validate_abilities(&case["expected"]).unwrap();
    let expected = semantic_abilities(&case["expected"]).unwrap();
    let context = serde_json::from_value(case["characteristics"].clone()).unwrap();
    let parsed = engine().parse_card(name, text, &context).unwrap();
    assert_eq!(parsed.readings.len(), 1, "{name}");
    let reading = &parsed.readings[0];
    assert_eq!(
        semantic_abilities(&reading.abilities).unwrap(),
        expected,
        "{name}"
    );
    // As the corpus score does: the default print, the derivation key, then its order.
    let abilities = &reading.abilities;
    let types = crate::corpus::print_context(&case);
    let first = engine().print_keyed(name, abilities, &types, &[]).unwrap();
    if first.as_deref() != Some(text) {
        let key = engine().derivation_of(name, abilities, text).unwrap();
        let key = key.unwrap_or_else(|| panic!("{name}: {first:?} and no derivation"));
        let keyed = engine().print_keyed(name, abilities, &types, &key);
        if keyed.unwrap().as_deref() != Some(text) {
            let trail = engine().trail_of(name, abilities, text).unwrap().unwrap();
            let printed = engine().print_trail(name, abilities, &trail).unwrap();
            assert_eq!(printed.as_deref(), Some(text), "{name}");
        }
    }
    abilities.clone()
}

fn operations(value: &Value, name: &str, out: &mut Vec<Value>) {
    if value["kind"] == name {
        out.push(value["args"].clone());
    }
    match value {
        Value::Object(fields) => fields.values().for_each(|v| operations(v, name, out)),
        Value::Array(items) => items.iter().for_each(|v| operations(v, name, out)),
        _ => {}
    }
}

fn found(value: &Value, name: &str) -> Vec<Value> {
    let mut out = Vec::new();
    operations(value, name, &mut out);
    out
}

fn lands(player: Value) -> Value {
    json!({"kind":"CountObjects","args":{"kind":"Zone","args":{
        "scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"AllOf","args":[
            {"kind":"CardType","args":"Land"},
            {"kind":"ControlledBy","args":{"kind":"One","args":player}}]}}}})
}

/// The operands of the subtraction bounding a face's only Search.
fn search_count(value: &Value) -> Value {
    let searches = found(value, "Search");
    assert_eq!(searches.len(), 1);
    let count = &searches[0]["choices"][0]["count"]["UpTo"];
    assert_eq!(count["kind"], "Subtract");
    count["args"].clone()
}

#[test]
fn snowfall_offers_one_or_two_blue_for_a_snow_island() {
    let reading = reading("Snowfall");
    let mana: Vec<_> = found(&reading, "AddMana")
        .into_iter()
        .map(|op| op["mana"].clone())
        .collect();
    let blue = |n: u32| json!({"Amount":{"color":"Blue","amount":{"kind":"Constant","args":n}}});
    assert_eq!(mana, [json!({"ChooseOne":[blue(1), blue(2)]}), blue(1)]);
}

#[test]
fn aladdins_lamp_leaves_the_kept_card_in_place() {
    let reading = reading("Aladdin's Lamp");
    let reorders = found(&reading, "ReorderLibrary");
    assert_eq!(reorders.len(), 1);
    assert_eq!(reorders[0]["end"], "Bottom");
    let excluding = &reorders[0]["objects"]["args"]["objects"]["args"]["excluding"];
    assert_eq!(excluding["args"][0]["kind"], "Selected");
}

#[test]
fn bison_whistle_offers_the_graveyard_for_any_looked_at_card() {
    let reading = reading("Bison Whistle");
    assert!(found(&reading, "If").is_empty());
    let choices = found(&reading, "ChooseOne");
    assert_eq!(choices.len(), 1);
    let options = choices[0]["options"].as_array().unwrap();
    assert_eq!(options.len(), 4);
    let graveyard = &options[2]["instructions"][0]["args"]["operation"]["args"];
    assert_eq!(
        graveyard["objects"]["args"]["objects"]["args"]["filter"],
        json!({"kind":"Any"})
    );
}

#[test]
fn land_differences_subtract_the_smaller_count() {
    for name in ["Scholarship Sponsor", "Enigma Ridges"] {
        let count = search_count(&reading(name));
        assert_eq!(
            count,
            json!([{"kind":"BoundValue","args":0}, lands(json!({"kind":"Bound","args":0}))]),
            "{name}"
        );
    }
    let count = search_count(&reading("Boreas Charger"));
    assert_eq!(
        count,
        json!([
            lands(json!({"kind":"Chosen"})),
            lands(json!({"kind":"AbilityController"}))
        ])
    );
}

#[test]
fn heroism_pays_for_each_attacking_creature_as_it_resolves() {
    let reading = reading("Heroism");
    let loops = found(&reading, "ForEachObject");
    assert_eq!(loops.len(), 1);
    let body = &loops[0]["body"]["instructions"];
    let pay = &body[0]["args"]["operation"];
    assert_eq!(pay["kind"], "PayCost");
    let bound = json!({"kind":"Bound","args":0});
    assert_eq!(
        pay["args"]["player"],
        json!({"kind":"ControllerOf","args":bound})
    );
    let shields = found(&body[1], "CreateReplacement");
    assert_eq!(shields.len(), 1);
    assert_eq!(
        shields[0]["event"]["Damage"]["sources"],
        json!({"kind":"SameObject","args":bound})
    );
    assert!(found(&shields[0]["body"], "PayCost").is_empty());
}

#[test]
fn infernal_denizen_sacrifices_what_it_can_before_testing() {
    let reading = reading("Infernal Denizen");
    let trigger = json!([reading[0].clone()]);
    assert!(found(&trigger, "PayCost").is_empty());
    let sacrifices = found(&trigger, "Sacrifice");
    assert_eq!(sacrifices.len(), 1);
    assert_eq!(
        sacrifices[0]["choose"],
        json!({"AsManyAsCan":{"kind":"Constant","args":2}})
    );
    let test = &found(&trigger, "If")[0]["condition"]["args"];
    assert_eq!(test["left"]["kind"], "Earlier");
    assert_eq!(test["left"]["args"]["what"], "Sacrificed");
    assert_eq!(
        test["test"],
        json!({"kind":"Compare","args":{"comparison":"Less","value":{"kind":"Constant","args":2}}})
    );
}

#[test]
fn tetravus_exiles_only_tokens_it_created() {
    let reading = reading("Tetravus");
    let made = json!({"kind":"MadeBy","args":{"kind":"Source"}});
    let conjunctions: Vec<_> = found(&reading, "AllOf")
        .into_iter()
        .filter(|parts| parts.as_array().unwrap().contains(&made))
        .collect();
    assert_eq!(conjunctions.len(), 1);
    let parts = conjunctions[0].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert!(parts.contains(&json!({"kind":"Token"})));
    let text = reading.to_string();
    assert_eq!(text.matches(&made.to_string()).count(), 1, "{text}");
}

/// The body of a face's only `PlayersDo`, in its only reading.
fn players_do_body(name: &str) -> Value {
    let reading = reading(name);
    let wrappers = found(&reading, "PlayersDo");
    assert_eq!(wrappers.len(), 1, "{name}");
    wrappers[0]["body"].clone()
}

fn one(player: &Value) -> Value {
    json!({"kind":"One","args":player})
}

#[test]
fn players_do_names_the_acting_player_acting() {
    let acting = json!({"kind":"Acting"});
    let mastery = players_do_body("Ingenious Mastery");
    assert_eq!(found(&mastery, "CreateTokens")[0]["players"], one(&acting));
    assert_eq!(found(&mastery, "Scry")[0]["players"], one(&acting));
    let selection = players_do_body("Natural Selection");
    assert_eq!(found(&selection, "Shuffle")[0]["players"], one(&acting));
    let valley = players_do_body("Stardew Valley");
    assert_eq!(found(&valley, "SetController"), vec![acting]);
}

#[test]
fn players_do_keeps_the_ability_controller_apart_from_the_actor() {
    let acting = json!({"kind":"Acting"});
    let controller = json!({"kind":"AbilityController"});
    // The opponent chooses and gains control of a creature the controller controls.
    let denizen = players_do_body("Infernal Denizen");
    let choice = &found(&denizen, "ChooseObjects")[0];
    assert_eq!(choice["player"], acting);
    assert_eq!(
        found(&choice["objects"], "ControlledBy"),
        vec![one(&controller)]
    );
    assert_eq!(found(&denizen, "SetController"), vec![acting.clone()]);
    // The target reveals; the controller chooses a card that player owns.
    let prison = players_do_body("Thought Prison");
    let revealed = &found(&prison, "Reveal")[0]["objects"]["args"]["scope"]["Players"];
    assert_eq!(revealed["players"], one(&acting));
    let choice = &found(&prison, "ChooseObjects")[0];
    assert_eq!(choice["player"], controller);
    assert_eq!(found(&choice["objects"], "OwnedBy"), vec![one(&acting)]);
    // Each opponent may pay, but "your library" stays the controller's.
    let racket = players_do_body("Protection Racket");
    assert!(!racket.to_string().contains(&acting.to_string()));
    let libraries = found(&racket, "Take");
    assert_eq!(libraries.len(), 3);
    for take in libraries {
        let scope = &take["objects"]["args"]["scope"]["Players"];
        assert_eq!(scope["players"], one(&controller));
    }
}
