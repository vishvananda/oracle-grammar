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
fn a_vacuous_count_wrapper_is_unnecessary_but_cardinality_is_still_proved() {
    let mut test = counted();
    test["args"]["condition"]["args"]["left"]["args"] =
        json!({"kind":"Transition","args":{"result":0,"phase":"After"}});
    let batch_test = test.clone();
    let mut ability = spell(json!([exiled(top_card()), test]));
    assert!(lower(&mut ability));
    let condition = &ability["args"]["body"]["instructions"][1]["args"]["condition"];
    assert_eq!(condition["kind"], "ObjectMatches");
    assert_eq!(condition["args"]["filter"], json!({"kind":"Any"}));
    let mut multiple = top_card();
    multiple["args"]["count"]["args"] = json!(2);
    let mut batch = spell(json!([exiled(multiple), batch_test]));
    assert!(!lower(&mut batch));
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

fn paid_card_measure() -> Value {
    let fixtures: Vec<Value> =
        serde_json::from_str(include_str!("../../tests/fixtures/copy-actions.json")).unwrap();
    let case = fixtures
        .iter()
        .find(|case| case["name"] == "Ellie and Alan, Paleontologists")
        .unwrap();
    case["expected"][0].clone()
}

fn paid_amount(ability: &mut Value) -> &mut Value {
    &mut ability["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["action"]["Discover"]
        ["amount"]
}

#[test]
fn a_single_activation_exile_sum_and_scalar_share_the_actual_receipt() {
    let scalar = paid_card_measure();
    let mut counted = scalar.clone();
    assert!(surface(&mut counted));
    assert_eq!(paid_amount(&mut counted)["kind"], "Aggregate");
    assert_eq!(
        paid_amount(&mut counted)["args"]["objects"]["args"]["component"],
        json!([1])
    );
    let mut lowered = counted.clone();
    assert!(lower(&mut lowered));
    assert_eq!(lowered, scalar);
    assert!(!lower(&mut lowered));
    let mut scoped = json!([
        counted.clone(),
        spell(counted["args"]["body"]["instructions"].clone())
    ]);
    assert!(lower(&mut scoped));
    assert_eq!(scoped[0], scalar);
    assert_eq!(scoped[1]["args"]["body"], counted["args"]["body"]);
}

#[test]
fn activation_measure_proof_keeps_other_costs_views_and_invalid_operands_distinct() {
    let mut counted = paid_card_measure();
    assert!(surface(&mut counted));
    for (path, value) in [
        (
            "/args/cost/parts/1/Move/selections/0/choose",
            json!({"Exactly":{"kind":"Constant","args":2}}),
        ),
        (
            "/args/cost/parts/1/Move/selections/0/choose",
            json!({"UpTo":{"kind":"Constant","args":1}}),
        ),
        ("/args/cost/extra", json!(true)),
        (
            "/args/body/instructions/0/args/operation/args/action/Discover/amount/extra",
            json!(true),
        ),
    ] {
        let mut negative = counted.clone();
        let (parent, key) = path.rsplit_once('/').unwrap();
        negative.pointer_mut(parent).unwrap()[key] = value;
        let original = negative.clone();
        assert!(!lower(&mut negative), "{path}");
        assert_eq!(negative, original, "{path}");
    }
    for (field, value) in [
        ("phase", json!("Before")),
        ("view", json!("Inputs")),
        ("payment", json!({"Resolution":0})),
        ("component", json!([0])),
    ] {
        let mut negative = counted.clone();
        paid_amount(&mut negative)["args"]["objects"]["args"][field] = value;
        assert!(!lower(&mut negative), "{field}");
    }
}
