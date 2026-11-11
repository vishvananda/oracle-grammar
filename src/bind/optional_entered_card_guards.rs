use super::*;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/optional-entered-card-receipts.json"
    ))
    .unwrap()
}
fn steps(pointer: &str) -> Vec<Step> {
    pointer
        .trim_start_matches('/')
        .split('/')
        .map(|part| {
            part.parse()
                .map_or_else(|_| Step::Key(part.into()), Step::Index)
        })
        .collect()
}
fn marker(name: &str) -> Value {
    if name == "qualified_object" {
        json!({"$sym":name,"args":["Permanent",{"kind":"CardType","args":"Creature"}],"at":1})
    } else {
        json!({"$sym":name,"args":[],"at":1})
    }
}
fn hand(inside: bool) -> (Value, String) {
    let cases = fixture();
    let row = cases
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "Planebound Accomplice")
        .unwrap();
    let mut root = row["expected_readings"][usize::from(inside)][0].clone();
    let body = &mut root["args"]["body"]["instructions"][0]["args"]["body"]["instructions"];
    body[0]["args"]["result"] = json!(29);
    body[1]["args"]["result"] = json!(41);
    body[1]["args"]["operation"]["args"]["objects"] = json!({"kind":"Selected","args":29});
    let path = if inside {
        "/args/body/instructions/0/args/body/instructions/2/args/operation/args/body/instructions/0/args/operation/args/objects/args"
    } else {
        "/args/body/instructions/1/args/operation/args/body/instructions/0/args/operation/args/objects/args"
    };
    *root.pointer_mut(path).unwrap() = marker("it");
    (root, path.into())
}
fn lower(root: &Value, pointer: &str, name: &str) -> Option<Value> {
    reference(root, &steps(pointer), name, None).unwrap()
}
fn body_mut(root: &mut Value) -> &mut Value {
    &mut root["args"]["body"]["instructions"][0]["args"]["body"]["instructions"]
}

#[test]
fn physical_card_and_actual_selected_move_ids_are_required_in_both_scopes() {
    for inside in [false, true] {
        let (root, path) = hand(inside);
        assert_eq!(
            lower(&root, &path, "it"),
            Some(receipt_object(41)["args"].clone())
        );
        for count in [
            json!({"Exactly":{"kind":"Constant","args":2}}),
            json!({"UpTo":{"kind":"Constant","args":2}}),
            json!({"AsManyAsCan":{"kind":"AnnouncedX"}}),
        ] {
            let mut wrong = root.clone();
            body_mut(&mut wrong)[0]["args"]["operation"]["args"]["count"] = count;
            assert_eq!(lower(&wrong, &path, "it"), None);
        }
        for (pointer, value) in [
            (
                "/0/args/operation/args/objects/args/kind",
                json!("Permanent"),
            ),
            (
                "/0/args/operation/args/objects/args/scope/Players/zone",
                json!("Graveyard"),
            ),
            (
                "/0/args/operation/args/objects/args/filter",
                json!({"kind":"Not","args":{"kind":"CardType","args":"Creature"}}),
            ),
            ("/1/args/operation/args/objects/args", json!(30)),
            ("/1/args/operation/args/destination", json!("OwnersHand")),
        ] {
            let mut wrong = root.clone();
            *body_mut(&mut wrong).pointer_mut(pointer).unwrap() = value;
            assert_eq!(lower(&wrong, &path, "it"), None);
        }
    }
}

#[test]
fn creature_noun_needs_positive_creature_proof_and_explicit_names_stay_literal() {
    let (mut root, path) = hand(false);
    *root.pointer_mut(&path).unwrap() = marker("qualified_object");
    assert_eq!(lower(&root, &path, "qualified_object"), None);
    body_mut(&mut root)[0]["args"]["operation"]["args"]["objects"]["args"]["filter"] =
        json!({"kind":"CardType","args":"Creature"});
    assert!(lower(&root, &path, "qualified_object").is_some());
    for name in [
        "self",
        "named_source",
        "named_object",
        "current_named_object",
        "current_it",
        "defined_it",
    ] {
        *root.pointer_mut(&path).unwrap() = marker(name);
        assert_eq!(lower(&root, &path, name), None);
    }
}

#[test]
fn no_intervening_action_future_property_or_nested_optional_is_inferred() {
    let (root, path) = hand(false);
    let draw = json!({"kind":"Effect","args":{"result":3,"operation":{
        "kind":"Draw","args":{"players":{"kind":"One","args":controller()},"amount":{"kind":"Constant","args":1}}
    }}});
    let mut future = root.clone();
    future["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .push(draw.clone());
    assert_eq!(lower(&future, &path, "it"), None);
    let mut intervening = root.clone();
    intervening["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .insert(1, draw);
    let shifted = path.replacen("instructions/1/", "instructions/2/", 1);
    assert_eq!(lower(&intervening, &shifted, "it"), None);
    let mut nested = root.clone();
    let old = nested["args"]["body"]["instructions"].take();
    nested["args"]["body"]["instructions"] = json!([{"kind":"Optional","args":{
        "decider":controller(),"body":{"instructions":old}
    }}]);
    let deeper = path.replacen(
        "/args/body/instructions/",
        "/args/body/instructions/0/args/body/instructions/",
        1,
    );
    assert_eq!(lower(&nested, &deeper, "it"), None);
}

#[test]
fn callbacks_queries_properties_announcements_and_holder_edges_do_not_supply_a_role() {
    let (root, path) = hand(false);
    for field in ["announcement", "cost", "event"] {
        let mut wrapper = json!({"kind":"Activated","args":{}});
        wrapper["args"][field] = root.clone();
        let nested_path = format!("/args/{field}{path}");
        assert_eq!(lower(&wrapper, &nested_path, "it"), None);
    }
    for name in ["GrantAbility", "CreateTrigger", "ForEachPlayer", "If"] {
        let wrapper = json!({"kind":"Activated","args":{"body":{"instructions":[{
            "kind":name,"args":{"body":root}
        }]}}});
        let nested_path = format!("/args/body/instructions/0/args/body{path}");
        assert_eq!(lower(&wrapper, &nested_path, "it"), None);
    }
    let mut query = root.clone();
    let object_path = path.trim_end_matches("/args");
    *query.pointer_mut(object_path).unwrap() = json!({"kind":"One","args":{"kind":"Query","args":{
        "objects":{"kind":"Matching","args":{"objects":{"kind":"One","args":marker("it")},"filter":{"kind":"Any"}}},"index":0
    }}});
    assert_eq!(
        lower(
            &query,
            &format!("{path}/args/objects/args/objects/args"),
            "it"
        ),
        None
    );
}

#[test]
fn delayed_gate_and_work_bounds_are_not_silently_widened() {
    let (root, path) = hand(false);
    let delayed = "/args/body/instructions/1/args/operation/args";
    for (suffix, value) in [
        (
            "/event/Turn/players",
            json!({"kind":"One","args":controller()}),
        ),
        ("/expires", json!("EndOfTurn")),
        ("/start", json!({"PlayersNextTurn":controller()})),
        ("/intervening_if", json!({"kind":"PaymentChosen","args":0})),
    ] {
        let mut wrong = root.clone();
        *wrong.pointer_mut(&format!("{delayed}{suffix}")).unwrap() = value;
        assert_eq!(lower(&wrong, &path, "it"), None);
    }
    let mut huge = root;
    body_mut(&mut huge)[0]["args"]["operation"]["args"]["constraints"] = json!(["x".repeat(65537)]);
    assert_eq!(lower(&huge, &path, "it"), None);
}

fn keyword_grant(objects: Value) -> Value {
    json!({"kind":"Effect","args":{"result":51,"operation":{
        "kind":"CreateContinuousEffect","args":{
            "source":{"kind":"Source"},"affected":{"Objects":objects},
            "changes":[{"kind":"GrantKeyword","args":{"kind":"Haste"}}],
            "duration":"EndOfTurn","evaluation":"Snapshot"
        }
    }}})
}

#[test]
fn two_delays_and_delay_then_grant_cannot_be_partially_rebound() {
    let (base, path) = hand(false);
    let delayed = base["args"]["body"]["instructions"][1].clone();
    for second in [
        delayed,
        keyword_grant(json!({"kind":"One","args":marker("it")})),
    ] {
        let mut root = base.clone();
        root["args"]["body"]["instructions"]
            .as_array_mut()
            .unwrap()
            .push(second);
        assert_eq!(lower(&root, &path, "it"), None);
        let last = if root["args"]["body"]["instructions"][2]["args"]["operation"]["kind"]
            == "CreateDelayedTrigger"
        {
            path.replacen("instructions/1/", "instructions/2/", 1)
        } else {
            "/args/body/instructions/2/args/operation/args/affected/Objects/args".into()
        };
        assert_eq!(lower(&root, &last, "it"), None);
    }
}

#[test]
fn a_proper_earlier_grant_does_not_authorize_a_later_suffix_of_two_grants() {
    let (mut root, _) = hand(false);
    let tail = root["args"]["body"]["instructions"][1].clone();
    let instructions = root["args"]["body"]["instructions"].as_array_mut().unwrap();
    // The earlier receipt is already correct: skip_grant alone would pass it.
    // Only checking the full producer tail rejects the second grant's valid suffix.
    instructions[1] = keyword_grant(receipt_object(41));
    instructions.push(keyword_grant(json!({"kind":"One","args":marker("it")})));
    instructions.push(tail);
    let grant = "/args/body/instructions/2/args/operation/args/affected/Objects/args";
    let delayed = "/args/body/instructions/3/args/operation/args/body/instructions/0/args/operation/args/objects/args";
    assert_eq!(lower(&root, grant, "it"), None);
    assert_eq!(lower(&root, delayed, "it"), None);
}

fn qualified_envelope(inside: bool) -> (Value, String) {
    let (mut root, old_path) = hand(inside);
    body_mut(&mut root)[0]["args"]["operation"]["args"]["objects"]["args"]["filter"] =
        json!({"kind":"CardType","args":"Creature"});
    let objects = old_path.trim_end_matches("/args");
    // Exact nouns.ebnf qualified_set -> that_noun -> named_creature expansion.
    *root.pointer_mut(objects).unwrap() = json!({"$sym":"qualified_members","at":1,"args":[
        {"$sym":"remember_operand","at":1,"args":[marker("qualified_object")]}
    ]});
    (root, format!("{objects}/args/0/args/0"))
}

#[test]
fn exact_raw_qualified_members_envelope_uses_the_same_actual_card_move() {
    for inside in [false, true] {
        let (root, path) = qualified_envelope(inside);
        assert_eq!(
            lower(&root, &path, "qualified_object"),
            Some(receipt_object(41)["args"].clone())
        );
        let mut malformed = root.clone();
        let objects = path.trim_end_matches("/args/0/args/0");
        malformed.pointer_mut(objects).unwrap()["args"]
            .as_array_mut()
            .unwrap()
            .push(marker("qualified_object"));
        assert_eq!(lower(&malformed, &path, "qualified_object"), None);
    }
}

#[test]
fn inherited_creation_id_cannot_promote_a_move_receipt_into_a_created_batch() {
    let mut outer = super::super::Scope::default();
    outer.results.allocate().unwrap();
    let token = serde_json::to_value(outer.results.allocate().unwrap())
        .unwrap()
        .as_u64()
        .unwrap();
    outer.created_result = Some((token, false));
    let mut nested = outer.nested();
    let choice = serde_json::to_value(nested.results.allocate().unwrap())
        .unwrap()
        .as_u64()
        .unwrap();
    let moved = serde_json::to_value(nested.results.allocate().unwrap())
        .unwrap()
        .as_u64()
        .unwrap();
    assert_eq!(token, moved); // Existing namespace inheritance, not a new Scope policy.
    let (mut root, path) = qualified_envelope(false);
    body_mut(&mut root)[0]["args"]["result"] = json!(choice);
    body_mut(&mut root)[1]["args"]["result"] = json!(moved);
    body_mut(&mut root)[1]["args"]["operation"]["args"]["objects"]["args"] = json!(choice);
    assert_eq!(
        reference(
            &root,
            &steps(&path),
            "qualified_object",
            nested.created_result.map(|(id, _)| id)
        )
        .unwrap(),
        None
    );
    assert!(
        reference(&root, &steps(&path), "qualified_object", Some(99))
            .unwrap()
            .is_some()
    );
    let node = json!({"$sym":"qualified_members","args":[receipt_object(token)["args"]]});
    assert!(super::super::qualified::members(&node, &nested).is_err());
    nested.created_result = Some((token, true));
    assert_eq!(
        super::super::qualified::members(&node, &nested).unwrap(),
        json!({"kind":"Created","args":token})
    );
}

#[test]
fn a_token_creator_is_not_a_single_physical_card_move_producer() {
    let (mut root, path) = qualified_envelope(false);
    body_mut(&mut root)[1]["args"]["operation"]["kind"] = json!("CreateTokens");
    assert_eq!(lower(&root, &path, "qualified_object"), None);
    for name in [
        "created_reference_single",
        "created_reference",
        "created_batch_reference",
    ] {
        assert_eq!(reference(&root, &steps(&path), name, None).unwrap(), None);
    }
}
