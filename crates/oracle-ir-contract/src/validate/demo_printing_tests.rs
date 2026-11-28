use crate::{ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"[{"kind": "WithPresentation", "args": {"label": "Play the Game, See the World", "reminder": false, "ability": {"kind": "Triggered", "args": {"event": {"Player": {"event": "Planeswalks", "players": {"kind": "One", "args": {"kind": "AbilityController"}}, "objects": {"kind": "SameObject", "args": {"kind": "Source"}}}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "ForEachPlayer", "args": {"players": {"kind": "All"}, "binding": 0, "starting_with": null, "in_chosen_direction": false, "body": {"instructions": [{"kind": "Optional", "args": {"decider": {"kind": "Bound", "args": 0}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 0, "operation": {"kind": "Search", "args": {"searcher": {"kind": "Bound", "args": 0}, "library_owner": {"kind": "Bound", "args": 0}, "top": null, "choices": [{"filter": {"kind": "AllOf", "args": [{"kind": "Supertype", "args": "Basic"}, {"kind": "CardType", "args": "Land"}]}, "count": {"UpTo": {"kind": "Constant", "args": 1}}, "constraints": []}], "also": [], "either": false, "outside": false}}}}, {"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "Move", "args": {"objects": {"kind": "Selected", "args": 0}, "destination": {"OwnersHand": {"counters": [], "states": []}}, "qualifiers": []}}}}, {"kind": "Effect", "args": {"result": 4294967294, "operation": {"kind": "Shuffle", "args": {"players": {"kind": "One", "args": {"kind": "Bound", "args": 0}}, "excluding": null}}}}]}}}]}}}]}}}}}, {"kind": "WithPresentation", "args": {"label": "Here I Rule", "reminder": false, "ability": {"kind": "Triggered", "args": {"event": {"Player": {"event": "PlaysCard", "players": {"kind": "One", "args": {"kind": "AbilityController"}}, "objects": {"kind": "AnyOf", "args": [{"kind": "PrintingHas", "args": "AlphaPrinting"}, {"kind": "AllOf", "args": [{"kind": "Supertype", "args": "Basic"}, {"kind": "CardType", "args": "Land"}]}, {"kind": "PrintingHas", "args": {"ExpansionSymbol": "CoreSet"}}, {"kind": "PrintingHas", "args": {"ExpansionSymbol": "Foundations"}}]}}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "Draw", "args": {"players": {"kind": "One", "args": {"kind": "AbilityController"}}, "amount": {"kind": "Constant", "args": 1}}}}}]}}}}}, {"kind": "WithPresentation", "args": {"label": "Ignite Your Spark", "reminder": false, "ability": {"kind": "Triggered", "args": {"event": {"Player": {"event": "ChaosEnsues", "players": {"kind": "All"}, "objects": null}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "GainLife", "args": {"players": {"kind": "One", "args": {"kind": "AbilityController"}}, "amount": {"kind": "PlayerNumber", "args": {"player": {"kind": "AbilityController"}, "number": "YearsPlayedMagic"}}}}}}, {"kind": "Effect", "args": {"result": 4294967294, "operation": {"kind": "Keyword", "args": {"actor": {"kind": "One", "args": {"kind": "AbilityController"}}, "action": "Planeswalk"}}}}]}}}}}]"###).unwrap()
}
fn checked(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn full_abilities_validate() {
    for a in source().as_array().unwrap() {
        assert!(checked(a.clone()));
    }
}
#[test]
fn play_event_is_single_and_uses_printing_not_oracle_name() {
    let a = source();
    let e = &a[1]["args"]["ability"]["args"]["event"]["Player"];
    assert_eq!(e["event"], "PlaysCard");
    let f = e["objects"]["args"].as_array().unwrap();
    assert!(
        f.iter()
            .any(|x| x["kind"] == "PrintingHas" && x["args"] == "AlphaPrinting")
    );
    assert!(
        f.iter()
            .any(|x| x["args"]["ExpansionSymbol"] == "Foundations")
    );
    assert!(f.iter().any(|x| x["args"]["ExpansionSymbol"] == "CoreSet"));
    assert!(f.iter().any(|x| {
        x["kind"] == "AllOf"
            && x["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["args"] == "Basic")
    }));
    assert!(
        f.iter()
            .all(|x| x["kind"] != "Name" && x["kind"] != "NameFirstPrintedIn")
    );
}
#[test]
fn years_require_a_scoped_player() {
    let mut a = source()[2].clone();
    *a.pointer_mut(
        "/args/ability/args/body/instructions/0/args/operation/args/amount/args/player",
    )
    .unwrap() = json!({"kind":"Bound","args":4});
    assert!(!checked(a));
}
#[test]
fn card_play_actor_requires_scope() {
    let mut a = source()[1].clone();
    *a.pointer_mut("/args/ability/args/event/Player/players/args")
        .unwrap() = json!({"kind":"Bound","args":4});
    assert!(!checked(a));
}
