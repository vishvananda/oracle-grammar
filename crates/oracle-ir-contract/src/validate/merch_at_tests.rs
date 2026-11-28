use crate::{ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"{"kind": "WithPresentation", "args": {"label": null, "reminder": true, "ability": {"kind": "Triggered", "args": {"event": {"Player": {"event": "EncountersPhenomenon", "players": {"kind": "One", "args": {"kind": "AbilityController"}}, "objects": {"kind": "SameObject", "args": {"kind": "Source"}}}}, "intervening_if": null, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "GainPrintedResource", "args": {"players": {"kind": "All"}, "resource": "AtSign", "amount": {"kind": "Constant", "args": 3}}}}}, {"kind": "ForEachPlayerInTurnOrder", "args": {"players": {"kind": "All"}, "binding": 0, "starting_with": {"kind": "AbilityController"}, "body": {"instructions": [{"kind": "Repeatable", "args": {"instructions": [{"kind": "ChooseOne", "args": {"decider": {"kind": "Bound", "args": 0}, "options": [{"instructions": [{"kind": "Effect", "args": {"result": 0, "operation": {"kind": "PayCost", "args": {"player": {"kind": "Bound", "args": 0}, "cost": {"parts": [{"SpendPrintedResource": {"resource": "AtSign", "amount": {"kind": "Constant", "args": 2}}}]}, "optional": true}}}}, {"kind": "If", "args": {"condition": {"kind": "PaymentChosen", "args": 0}, "then": {"instructions": [{"kind": "Effect", "args": {"result": 4294967294, "operation": {"kind": "Draw", "args": {"players": {"kind": "One", "args": {"kind": "Bound", "args": 0}}, "amount": {"kind": "Constant", "args": 1}}}}}, {"kind": "Effect", "args": {"result": 4294967293, "operation": {"kind": "RepeatProcess"}}}]}, "otherwise": {"instructions": []}}}]}, {"instructions": [{"kind": "Effect", "args": {"result": 1, "operation": {"kind": "PayCost", "args": {"player": {"kind": "Bound", "args": 0}, "cost": {"parts": [{"SpendPrintedResource": {"resource": "AtSign", "amount": {"kind": "Constant", "args": 1}}}]}, "optional": true}}}}, {"kind": "If", "args": {"condition": {"kind": "PaymentChosen", "args": 1}, "then": {"instructions": [{"kind": "Effect", "args": {"result": 4294967292, "operation": {"kind": "CreateTokens", "args": {"players": {"kind": "One", "args": {"kind": "Bound", "args": 0}}, "tokens": [{"amount": {"kind": "Constant", "args": 1}, "words": "CreatingEffect", "recipe": {"Defined": {"name": {"Explicit": "Fblthp"}, "colors": [], "chosen_color": false, "chosen_creature_type": false, "supertypes": [], "card_types": ["Artifact", "Creature"], "subtypes": ["Toy"], "power_toughness": [{"kind": "Constant", "args": 1}, {"kind": "Constant", "args": 1}], "abilities": [], "abilities_of": null}}}], "entry": {"controller": null, "tapped": false, "counters": [], "states": []}}}}}, {"kind": "Effect", "args": {"result": 4294967291, "operation": {"kind": "RepeatProcess"}}}]}, "otherwise": {"instructions": []}}}]}], "offered": null}}]}}]}}}]}}}}}"###).unwrap()
}
fn checked(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn grants_distinct_resource_and_orders_repeatable_purchases() {
    let a = source();
    assert!(checked(a.clone()));
    let seq = &a["args"]["ability"]["args"]["body"]["instructions"];
    let grant = &seq[0]["args"]["operation"]["args"];
    assert_eq!(grant["players"]["kind"], "All");
    assert_eq!(grant["resource"], "AtSign");
    assert_eq!(grant["amount"]["args"], 3);
    let r = &seq[1];
    assert_eq!(r["kind"], "ForEachPlayerInTurnOrder");
    assert_eq!(r["args"]["starting_with"]["kind"], "AbilityController");
    let rep = &r["args"]["body"]["instructions"][0];
    assert_eq!(rep["kind"], "Repeatable");
    let choice = &rep["args"]["instructions"][0]["args"];
    assert_eq!(choice["decider"], json!({"kind":"Bound","args":0}));
    for (option, price, action) in [(0, 2, "Draw"), (1, 1, "CreateTokens")] {
        let p = &choice["options"][option]["instructions"];
        let pay = &p[0]["args"]["operation"]["args"];
        assert_eq!(pay["player"], choice["decider"]);
        assert_eq!(pay["optional"], true);
        assert_eq!(
            pay["cost"]["parts"][0]["SpendPrintedResource"]["resource"],
            "AtSign"
        );
        assert_eq!(
            pay["cost"]["parts"][0]["SpendPrintedResource"]["amount"]["args"],
            price
        );
        assert_eq!(p[1]["args"]["condition"]["kind"], "PaymentChosen");
        assert_eq!(
            p[1]["args"]["then"]["instructions"][0]["args"]["operation"]["kind"],
            action
        );
        assert_eq!(
            p[1]["args"]["then"]["instructions"][1]["args"]["operation"]["kind"],
            "RepeatProcess"
        );
    }
}
#[test]
fn player_binding_cannot_escape_ordered_visit() {
    let mut a = source();
    *a.pointer_mut("/args/ability/args/body/instructions/0/args/operation/args/players")
        .unwrap() = json!({"kind":"One","args":{"kind":"Bound","args":0}});
    assert!(!checked(a));
}
