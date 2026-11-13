use super::*;

fn top_card() -> Value {
    json!({"kind":"Take","args":{"objects":{"kind":"Zone","args":{"scope":{"Players":{
        "zone":"Library","players":{"kind":"One","args":{"kind":"AbilityController"}}}},
        "kind":"Card","filter":{"kind":"Any"}}},"count":{"kind":"Constant","args":1},"from":"Top"}})
}

fn exiled(objects: Value) -> Value {
    json!({"kind":"Effect","args":{"result":0,"operation":{"kind":"Move","args":{
        "objects":objects,"destination":{"Exile":{"visibility":"FaceUp"}},"qualifiers":[]}}}})
}

fn counted() -> Value {
    json!({"kind":"If","args":{"condition":{"kind":"Compare","args":{
        "left":{"kind":"CountObjects","args":{"kind":"Matching","args":{
            "objects":{"kind":"Transition","args":{"result":0,"phase":"After"}},
            "filter":{"kind":"CardType","args":"Land"}}}},
        "test":{"kind":"Compare","args":{"comparison":"Greater","value":{"kind":"Constant","args":0}}}}},
        "then":{"instructions":[]},"otherwise":{"instructions":[]}}})
}

fn tested_in_place(instruction: &Value) -> bool {
    instruction["args"]["condition"]["kind"] == "ObjectMatches"
}

fn spell(instructions: Value) -> Value {
    json!({"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},
        "body":{"instructions":instructions}}})
}

#[test]
fn one_moved_card_is_tested_in_place_and_back() {
    let printed = spell(json!([exiled(top_card()), counted()]));
    let mut ability = printed.clone();
    assert!(lower(&mut ability));
    let test = &ability["args"]["body"]["instructions"][1];
    assert!(tested_in_place(test));
    assert_eq!(
        test["args"]["condition"]["args"]["object"],
        json!({"kind":"Result","args":{"result":0,"phase":"After","index":0}})
    );
    assert!(surface(&mut ability));
    assert_eq!(ability, printed);
}

#[test]
fn a_mill_of_one_or_a_loop_receipt_stays_a_batch() {
    let milled = json!({"kind":"Effect","args":{"result":0,"operation":{"kind":"Mill","args":{
        "players":{"kind":"One","args":{"kind":"AbilityController"}},
        "amount":{"kind":"Constant","args":1}}}}});
    assert!(!lower(&mut spell(json!([milled, counted()]))));
    let looped = json!({"kind":"ForEachPlayer","args":{"players":{"kind":"All"},"binding":0,
        "starting_with":null,"in_chosen_direction":false,
        "body":{"instructions":[exiled(top_card()), counted()]}}});
    let mut ability = spell(json!([looped, counted()]));
    assert!(lower(&mut ability));
    let body = &ability["args"]["body"]["instructions"];
    assert!(tested_in_place(&body[0]["args"]["body"]["instructions"][1]));
    assert!(!tested_in_place(&body[1]));
}

#[test]
fn a_branch_proves_nothing_to_what_follows_it() {
    let branch = json!({"kind":"If","args":{"condition":{"kind":"Always"},
        "then":{"instructions":[exiled(top_card()), counted()]},"otherwise":{"instructions":[]}}});
    let mut ability = spell(json!([branch, counted()]));
    assert!(lower(&mut ability));
    let body = &ability["args"]["body"]["instructions"];
    assert!(tested_in_place(&body[0]["args"]["then"]["instructions"][1]));
    assert!(!tested_in_place(&body[1]));
}

#[test]
fn one_card_target_is_one_but_a_permanent_leaves_no_proven_card() {
    let target = |zones: Value| {
        json!({"kind":"Spell","args":{"announcement":{"targets":[{"slot":0,"kind":{"Object":{
            "kind":"Card","zones":zones}},"count":{"Exactly":{"kind":"Constant","args":1}},
            "filter":{"kind":"Any"},"players":null,"distribute":null,"constraints":[],
            "chooser":null}],"modes":null},"body":{"instructions":[
            exiled(json!({"kind":"One","args":{"kind":"Target","args":{"slot":0,"index":0}}})),
            counted()]}}})
    };
    let mut card = target(json!(["Graveyard"]));
    assert!(lower(&mut card));
    let mut permanent = target(json!(["Battlefield"]));
    assert!(!lower(&mut permanent));
}

#[test]
fn a_sum_over_one_moved_card_is_its_property() {
    let gained = json!({"kind":"Effect","args":{"result":1,"operation":{"kind":"GainLife","args":{
        "players":{"kind":"One","args":{"kind":"AbilityController"}},
        "amount":{"kind":"Aggregate","args":{"objects":{"kind":"Transition","args":{
            "result":0,"phase":"After"}},"property":"ManaValue","aggregation":"Sum"}}}}}});
    let printed = spell(json!([exiled(top_card()), gained]));
    let mut ability = printed.clone();
    assert!(lower(&mut ability));
    let amount = &ability["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["amount"];
    assert_eq!(amount["kind"], "ObjectProperty");
    assert_eq!(amount["args"]["information"], "CurrentOrLastKnown");
    assert!(surface(&mut ability));
    assert_eq!(ability, printed);
}
