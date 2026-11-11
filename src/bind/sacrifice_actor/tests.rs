use super::*;
fn symbol(name: &str) -> Value {
    json!({"$sym":name,"args":[],"at":10})
}
fn it() -> Value {
    json!({"$sym":"member_count_it","args":[symbol("it")],"at":10})
}
fn noun(name: &str) -> Value {
    let filter = if name == "Permanent" {
        json!({"kind":"Any"})
    } else {
        json!({"kind":"CardType","args":name})
    };
    json!({"$sym":"remember_operand","args":[{"$sym":"qualified_object",
        "args":["Permanent",filter],"at":10}],"at":10})
}
fn raw(filter: Value, source: Value) -> Value {
    json!({"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},
    "body":{"instructions":[
        {"kind":"Effect","args":{"result":7,"operation":{"kind":"Sacrifice","args":{
            "players":{"kind":"One","args":{"kind":"AbilityController"}},
            "objects":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},
                "kind":"Permanent","filter":filter}},
            "choose":{"AsManyAsCan":{"kind":"Constant","args":1}}}}}},
        {"kind":"Effect","args":{"result":8,"operation":{"kind":"DealDamage","args":{
            "source":source,"recipients":{"kind":"Targets","args":2},
            "amount":{"kind":"Constant","args":3},"preventable":true}}}}
    ]}}})
}
fn path() -> Vec<Step> {
    super::super::path_of(&["args", "body", "instructions"])
        .into_iter()
        .chain([Step::Index(1)])
        .chain(super::super::path_of(&[
            "args",
            "operation",
            "args",
            "source",
        ]))
        .collect()
}
fn before() -> Value {
    json!({"kind":"Query","args":{"objects":{"kind":"Transition",
    "args":{"result":7,"phase":"Before"}},"index":0}})
}
fn creature() -> Value {
    json!({"kind":"CardType","args":"Creature"})
}
#[test]
fn ordinary_and_matching_nouns_use_actual_nonzero_before_result() {
    for source in [it(), noun("Creature"), noun("Permanent")] {
        let mut value = raw(creature(), source);
        prepare(&mut value).unwrap();
        assert_eq!(reference(&value, &path()).unwrap(), before());
        assert_eq!(value["args"]["announcement"]["modes"], Value::Null);
    }
}
#[test]
fn failed_positive_proof_is_never_disjointness_or_older_reference_fallback() {
    for (selected, named) in [("Artifact", "Creature"), ("Creature", "Artifact")] {
        let mut value = raw(json!({"kind":"CardType","args":selected}), noun(named));
        prepare(&mut value).unwrap();
        assert!(reference(&value, &path()).is_err());
    }
    let both = json!({"kind":"AllOf","args":[creature(),{"kind":"CardType","args":"Artifact"}]});
    for source in [noun("Creature"), noun("Artifact")] {
        let mut value = raw(both.clone(), source);
        prepare(&mut value).unwrap();
        assert_eq!(reference(&value, &path()).unwrap(), before());
    }
}
#[test]
fn raw_legacy_implicit_markers_cannot_keep_a_source_fallback() {
    for name in [
        "it",
        "current_it",
        "named_object",
        "current_named_object",
        "that_object",
    ] {
        let mut value = raw(creature(), symbol(name));
        prepare(&mut value).unwrap();
        assert!(reference(&value, &path()).is_err(), "{name}");
    }
}
#[test]
fn explicit_actors_and_intervening_instructions_are_untouched() {
    for source in [
        symbol("self"),
        symbol("named_source"),
        json!({"kind":"Target","args":{"slot":1,"index":0}}),
    ] {
        let mut value = raw(creature(), source);
        let original = value.clone();
        prepare(&mut value).unwrap();
        assert_eq!(value, original);
    }
    let mut value = raw(creature(), it());
    value["args"]["body"]["instructions"].as_array_mut().unwrap().insert(1,
        json!({"kind":"Effect","args":{"result":8,"operation":{"kind":"Draw","args":{
            "players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"Constant","args":1}}}}}));
    let original = value.clone();
    prepare(&mut value).unwrap();
    assert_eq!(value, original);
}
#[test]
fn fixed_plural_variable_zero_and_multiple_actors_do_not_establish_new_focus() {
    for count in [
        Value::Null,
        json!({"AsManyAsCan":{"kind":"Constant","args":2}}),
        json!({"AsManyAsCan":{"kind":"Constant","args":0}}),
        json!({"AsManyAsCan":{"kind":"AnnouncedX"}}),
        json!("AnyNumber"),
    ] {
        let mut value = raw(creature(), it());
        value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["choose"] = count;
        let original = value.clone();
        prepare(&mut value).unwrap();
        assert_eq!(value, original);
    }
    let mut value = raw(creature(), it());
    value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["players"] =
        json!({"kind":"All"});
    let original = value.clone();
    prepare(&mut value).unwrap();
    assert_eq!(value, original);
}
#[test]
fn optional_branch_and_holder_bodies_are_not_discovered() {
    let inner = raw(creature(), it())["args"]["body"].clone();
    for instruction in [
        json!({"kind":"Optional","args":{"decider":{"kind":"AbilityController"},"body":inner}}),
        json!({"kind":"If","args":{"condition":{"kind":"True"},"then":inner,"otherwise":{"instructions":[]}}}),
        json!({"kind":"Effect","args":{"result":0,"operation":{"kind":"GrantAbility","args":{"body":inner}}}}),
    ] {
        let mut value = json!({"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},"body":{"instructions":[instruction]}}});
        let original = value.clone();
        prepare(&mut value).unwrap();
        assert_eq!(value, original);
    }
}
#[test]
fn information_does_not_make_after_cards_or_cross_an_own_program_path() {
    let mut value = raw(creature(), it());
    prepare(&mut value).unwrap();
    let mut wrong = path();
    wrong.insert(0, Step::Key("foreign".into()));
    assert!(reference(&value, &wrong).is_err());
    value["args"]["body"]["instructions"][0]["args"]["result"] = json!(4294967296u64);
    assert!(reference(&value, &path()).is_err());
}

#[test]
fn full_producer_is_bounded_before_native_clone_and_decode() {
    let mut value = raw(creature(), it());
    let mut object = json!({"kind":"Source"});
    for _ in 0..40 {
        object = json!({"kind":"AttachedTo","args":object});
    }
    value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["players"] =
        json!({"kind":"One","args":{"kind":"ControllerOf","args":object}});
    prepare(&mut value).unwrap();
    assert!(
        reference(&value, &path())
            .unwrap_err()
            .contains("producer work/depth")
    );
}
