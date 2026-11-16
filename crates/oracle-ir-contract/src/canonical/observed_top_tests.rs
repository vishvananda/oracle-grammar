use super::*;

fn top() -> Value {
    json!({"kind":"Take","args":{"objects":{"kind":"Zone","args":{
        "scope":{"Players":{"zone":"Library","players":{"kind":"One","args":{
            "kind":"AbilityController"}}}},"kind":"Card","filter":{"kind":"Any"}}},
        "count":{"kind":"Constant","args":1},"from":"Top"}})
}
fn movement(result: u32) -> Value {
    json!({"kind":"Effect","args":{"result":result,"operation":{"kind":"Move","args":{
        "objects":top(),"destination":{"OwnersHand":{"counters":[],"states":[]}},"qualifiers":[]}}}})
}
fn example() -> Value {
    json!({"instructions":[{"kind":"Effect","args":{"result":0,
        "operation":{"kind":"Reveal","args":{"objects":top()}}}},
        {"kind":"If","args":{"condition":{"kind":"Compare","args":{
            "left":{"kind":"CountObjects","args":{"kind":"Matching","args":{
                "objects":{"kind":"Observed","args":0},"filter":{"kind":"CardType","args":"Creature"}}}},
            "test":{"kind":"Compare","args":{"comparison":"GreaterOrEqual","value":{"kind":"Constant","args":1}}}}},
            "then":{"instructions":[movement(1)]},"otherwise":{"instructions":[movement(2)]}}}]})
}
fn draw() -> Value {
    json!({"kind":"Effect","args":{"result":9,"operation":{"kind":"Draw","args":{
        "players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}}}}})
}
fn objects<'a>(program: &'a Value, branch: &str, index: usize) -> &'a Value {
    &program["instructions"][1]["args"][branch]["instructions"][index]["args"]["operation"]["args"]
        ["objects"]
}

#[test]
fn immediate_branch_moves_use_the_reveal_and_the_inverse_uses_current_operands() {
    let original = example();
    let mut value = original.clone();
    assert!(program(&mut value, false).unwrap());
    assert_eq!(
        objects(&value, "then", 0),
        &json!({"kind":"Observed","args":0})
    );
    assert_eq!(
        objects(&value, "otherwise", 0),
        &json!({"kind":"Observed","args":0})
    );
    assert!(!program(&mut value, false).unwrap());
    let ability =
        json!({"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},"body":value}});
    let projected = projection(&ability).unwrap().unwrap();
    assert_eq!(projected["args"]["body"], original);
    // Changing the current producer changes its spelling view, not its result identity.
    let mut changed = ability.clone();
    changed["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"]["args"]["objects"]
        ["args"]["scope"]["Players"]["players"]["args"] =
        json!({"kind":"OwnerOf","args":{"kind":"Source"}});
    let mut projected = projection(&changed).unwrap().unwrap();
    assert_eq!(
        objects(&projected["args"]["body"], "then", 0)["args"]["objects"]["args"]["scope"]["Players"]
            ["players"]["args"],
        json!({"kind":"OwnerOf","args":{"kind":"Source"}})
    );
    assert!(program(&mut projected["args"]["body"], false).unwrap());
    assert_eq!(projected["args"]["body"], changed["args"]["body"]);
}

#[test]
fn intervening_draws_other_libraries_and_other_observation_kinds_are_not_aliases() {
    let mut outer = example();
    outer["instructions"]
        .as_array_mut()
        .unwrap()
        .insert(1, draw());
    let before = outer.clone();
    assert!(!program(&mut outer, false).unwrap());
    assert_eq!(outer, before);
    let mut foreign = example();
    foreign["instructions"][1]["args"]["otherwise"]["instructions"] = json!([]);
    foreign["instructions"][1]["args"]["then"]["instructions"][0]["args"]["operation"]["args"]["objects"]
        ["args"]["objects"]["args"]["scope"]["Players"]["players"]["args"] =
        json!({"kind":"OwnerOf","args":{"kind":"Source"}});
    let before = foreign.clone();
    assert!(!program(&mut foreign, false).unwrap());
    assert_eq!(foreign, before);
    let mut inspected = example();
    inspected["instructions"][0]["args"]["operation"]["kind"] = json!("LookAt");
    let before = inspected.clone();
    assert!(!program(&mut inspected, false).unwrap());
    assert_eq!(inspected, before);
}

#[test]
fn child_effects_invalidate_only_their_own_branch_and_proofs_never_escape_a_join() {
    let mut value = example();
    value["instructions"][1]["args"]["then"]["instructions"]
        .as_array_mut()
        .unwrap()
        .insert(0, draw());
    value["instructions"]
        .as_array_mut()
        .unwrap()
        .push(movement(3));
    assert!(program(&mut value, false).unwrap());
    assert_eq!(objects(&value, "then", 1), &top());
    assert_eq!(
        objects(&value, "otherwise", 0),
        &json!({"kind":"Observed","args":0})
    );
    assert_eq!(
        value["instructions"][2]["args"]["operation"]["args"]["objects"],
        top()
    );
}

#[test]
fn mapped_observations_clone_read_origins_and_merge_redundant_query_origins() {
    let mut body = example();
    let player = json!({"kind":"One","args":{"kind":"Target","args":{"slot":7,"index":0}}});
    body["instructions"][0]["args"]["operation"]["args"]["objects"]["args"]["objects"]["args"]["scope"]
        ["Players"]["players"] = player.clone();
    for branch in ["then", "otherwise"] {
        body["instructions"][1]["args"][branch]["instructions"][0]["args"]["operation"]["args"]["objects"]
            ["args"]["objects"]["args"]["scope"]["Players"]["players"] = player.clone();
    }
    let card: crate::ir::UnifiedCard = serde_json::from_value(json!({
        "schema_version":crate::ir::SCHEMA_VERSION,"layout":"Card","data":[],"faces":[{
            "schema_version":crate::ir::SCHEMA_VERSION,"name":"Observation origin control",
            "characteristics":{"supertypes":[],"card_types":["Sorcery"],"subtypes":[],"mana_cost":null,
                "color_indicator":[],"power_toughness":null,"loyalty":null,"defense":null},
            "abilities":[{"kind":"Spell","args":{"body":body,"announcement":{"modes":null,"targets":[{
                "slot":7,"kind":"Player","count":{"Exactly":{"kind":"Constant","args":1}},
                "filter":{"kind":"Any"},"players":null,"distribute":null,"constraints":[],"chooser":null}]}}}]}]
    })).unwrap();
    let mapped = super::super::mapped_unified(&card).unwrap();
    assert!(mapped.map.introduced.is_empty());
    let prefix = "/faces/0/abilities/0/args/body/instructions";
    let condition = format!("{prefix}/1/args/condition/args/left/args/args/objects/args");
    assert!(mapped.map.sites[&condition].len() >= 3);
    let old = format!(
        "{prefix}/1/args/then/instructions/0/args/operation/args/objects/args/objects/args/scope/Players/players/args/args/slot"
    );
    let producer = format!(
        "{prefix}/0/args/operation/args/objects/args/objects/args/scope/Players/players/args/args/slot"
    );
    assert!(mapped.map.sites[&old].contains(&producer));
}
