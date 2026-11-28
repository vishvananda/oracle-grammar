use crate::{ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"[{"kind": "WithPresentation", "args": {"label": null, "reminder": true, "ability": {"kind": "Triggered", "args": {"event": {"AnyOf": [{"Player": {"event": "Planeswalks", "players": {"kind": "One", "args": {"kind": "AbilityController"}}, "objects": {"kind": "SameObject", "args": {"kind": "Source"}}}}, {"Turn": {"point": "UpkeepBegins", "players": {"kind": "One", "args": {"kind": "AbilityController"}}}}]}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "ClashEachOpponent", "args": {"actor": {"kind": "AbilityController"}, "on_winner": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "CreateTokens", "args": {"players": {"kind": "One", "args": {"kind": "ClashWinner"}}, "tokens": [{"amount": {"kind": "Constant", "args": 1}, "words": "CreatingEffect", "recipe": {"Defined": {"name": {"Explicit": "Omenpath"}, "colors": [], "chosen_color": false, "chosen_creature_type": false, "supertypes": [], "card_types": ["Enchantment"], "subtypes": [], "power_toughness": null, "abilities": [{"kind": "Activated", "args": {"origin": "Ordinary", "cost": {"parts": [{"Sacrifice": {"objects": {"kind": "One", "args": {"kind": "Source"}}, "choose": null}}]}, "limits": {"once": false, "once_each_turn": false, "sorcery": true, "condition": null, "activators": null, "keyword": null, "modifiers": []}, "announcement": {"targets": [], "modes": {"count": {"minimum": {"kind": "Constant", "args": 1}, "maximum": {"kind": "Constant", "args": 1}}, "options": [{"announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "Keyword", "args": {"actor": {"kind": "One", "args": {"kind": "AbilityController"}}, "action": "Planeswalk"}}}}]}, "weight": 1, "requires": null}, {"announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967294, "operation": {"kind": "Keyword", "args": {"actor": {"kind": "One", "args": {"kind": "AbilityController"}}, "action": "ChaosEnsues"}}}}]}, "weight": 1, "requires": null}], "rules": {"repetition": "OnceEach", "availability": "Any", "surcharge": null, "targets": "MayShare", "acceptance": "Required"}, "mechanism": {"Announced": {"chooser": "Controller", "method": "PlayerChoice"}}, "budget": null}}, "body": {"instructions": []}}}], "abilities_of": null}}}], "entry": {"controller": null, "tapped": false, "counters": [], "states": []}}}}}]}}}}}]}}}}}, {"kind": "Triggered", "args": {"event": {"Player": {"event": "ChaosEnsues", "players": {"kind": "All"}, "objects": null}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967294, "operation": {"kind": "ClashEachOpponent", "args": {"actor": {"kind": "AbilityController"}, "on_winner": {"instructions": [{"kind": "Optional", "args": {"decider": {"kind": "ClashWinner"}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "PlayCards", "args": {"actor": {"kind": "One", "args": {"kind": "ClashWinner"}}, "action": "CastSpell", "cards": {"kind": "ClashWinningCard"}, "count": {"Exactly": {"kind": "Constant", "args": 1}}, "cost": "Free", "into_exile": false}}}}]}}}]}}}}}]}}}]"###).unwrap()
}
fn checked(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn complete_face_validates() {
    for a in source().as_array().unwrap() {
        assert!(checked(a.clone()));
    }
}
#[test]
fn winner_cannot_be_the_outer_actor() {
    let mut a = source()[1].clone();
    *a.pointer_mut("/args/body/instructions/0/args/operation/args/actor")
        .unwrap() = json!({"kind":"ClashWinner"});
    assert!(!checked(a));
}
#[test]
fn winning_card_and_winner_cannot_escape_callback() {
    let mut a = source()[1].clone();
    let op = a
        .pointer("/args/body/instructions/0/args/operation/args/on_winner/instructions/0")
        .unwrap()
        .clone();
    a.pointer_mut("/args/body/instructions")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(op);
    assert!(!checked(a));
}
