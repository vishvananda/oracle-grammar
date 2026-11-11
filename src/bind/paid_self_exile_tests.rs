use super::*;
use crate::engine::{Engine, canonical_abilities, validate_abilities};

fn engine() -> &'static Engine {
    crate::test_support::engine()
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/paid-self-exile-native.json"
    ))
    .unwrap()
}
fn expected() -> Value {
    canonical_abilities(&fixture()["native"]).unwrap()
}
fn returned(frame: &Value) -> &Value {
    &frame[1]["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0]["args"]["operation"]
        ["args"]["body"]["instructions"][0]["args"]["operation"]["args"]
}
fn exact(engine: &Engine, name: &str, text: &str, frame: &Value) -> bool {
    let types = vec!["Creature".into(), "Legendary".into(), "Spirit".into()];
    if let Some(key) = engine.derivation_of(name, frame, text).unwrap()
        && engine
            .print_keyed(name, frame, &types, &key)
            .unwrap()
            .as_deref()
            == Some(text)
    {
        return true;
    }
    engine
        .trail_of(name, frame, text)
        .unwrap()
        .is_some_and(|trail| {
            engine.print_trail(name, frame, &trail).unwrap().as_deref() == Some(text)
        })
}
fn verified_print(name: &str, frame: &Value) -> String {
    validate_abilities(frame).unwrap();
    let text = engine()
        .print_keyed(name, frame, &["Creature".into()], &[])
        .unwrap()
        .unwrap();
    assert_eq!(
        engine().parse(name, &text).unwrap().readings,
        vec![canonical_abilities(frame).unwrap()],
        "{text}"
    );
    text
}

#[test]
fn original_hikari_has_only_the_source_correct_strict_full_population_reading() {
    let f = fixture();
    let name = f["name"].as_str().unwrap();
    let text = f["text"].as_str().unwrap();
    let parsed = engine().parse(name, text).unwrap();
    assert_eq!(parsed.readings, vec![expected()]);
    for wrong in f["wrong_old_readings"].as_array().unwrap() {
        assert!(
            !parsed
                .readings
                .contains(&canonical_abilities(wrong).unwrap())
        );
    }
    assert_eq!(
        parsed.readings[0][1]["args"]["event"],
        expected()[1]["args"]["event"]
    );
    assert!(exact(engine(), name, text, &f["native"]));
    verified_print(name, &expected());
}

#[test]
fn edited_control_and_entry_print_current_full_group_without_scalar_fallback() {
    let mut edited = expected();
    let path = "/1/args/body/instructions/1/args/then/instructions/0/args/operation/args/body/instructions/0/args/operation/args";
    let ret = edited.pointer_mut(path).unwrap();
    ret["destination"]["Battlefield"]["controller"] =
        json!({"Player":{"kind":"AbilityController"}});
    ret["destination"]["Battlefield"]["arrival"]["tapped"] = json!(true);
    let name = "Hikari, Twilight Guardian";
    let text = verified_print(name, &edited);
    assert!(text.to_ascii_lowercase().contains("tapped"));
    assert!(!text.contains("under its owner's control"));
    assert_eq!(
        returned(&edited)["objects"]["args"]["objects"]["kind"],
        "CostObjects"
    );
    assert!(
        returned(&edited)["objects"]["args"]["objects"]["args"]
            .get("index")
            .is_none()
    );
    assert_eq!(returned(&edited)["objects"]["args"]["zone"], "Exile");
    for (field, value) in [
        ("phase", json!("Before")),
        ("view", json!("Inputs")),
        ("component", json!([1])),
    ] {
        let mut wrong = edited.clone();
        wrong.pointer_mut(path).unwrap()["objects"]["args"]["objects"]["args"][field] = value;
        assert!(
            engine()
                .print_keyed(name, &wrong, &["Creature".into()], &[])
                .unwrap_or(None)
                .is_none()
        );
    }
}

#[test]
fn public_prefixed_activation_keeps_its_cost_and_actual_resolution_namespace() {
    let f = fixture();
    let case = &f["activated_prefix"];
    let name = case["name"].as_str().unwrap();
    let text = case["text"].as_str().unwrap();
    let wanted = canonical_abilities(&case["expected"]).unwrap();
    let parsed = engine().parse(name, text).unwrap();
    assert_eq!(parsed.readings, vec![wanted.clone()]);
    assert!(exact(engine(), name, text, &case["expected"]));
    verified_print(name, &wanted);
    let op = &wanted[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"];
    assert_eq!(op["optional"], true);
    assert_eq!(
        wanted[0]["args"]["cost"]["parts"],
        json!([{"Mana":[{"Generic":2},{"Color":"White"}]}])
    );
}

#[test]
fn public_explicit_foreign_target_is_not_replaced_by_the_self_payment() {
    let name = "Hikari, Twilight Guardian";
    let text = "Whenever you cast a Spirit or Arcane spell, you may exile Hikari. If you do, return target creature card from your graveyard to the battlefield at the beginning of the next end step.";
    let parsed = engine().parse(name, text).unwrap();
    assert!(!parsed.readings.is_empty());
    for frame in parsed.readings {
        let instructions = &frame[0]["args"]["body"]["instructions"];
        let objects = &instructions[1]["args"]["then"]["instructions"][0]["args"]["operation"]["args"]
            ["body"]["instructions"][0]["args"]["operation"]["args"]["objects"];
        assert!(objects.to_string().contains("Target"));
        assert!(!objects.to_string().contains("CostObjects"));
    }
}

fn raw(action: bool) -> Value {
    let moved = json!({"kind":"Move","args":{"objects":{"kind":"One","args":{"kind":"Source"}},"destination":{"Exile":{"visibility":"FaceUp","arrival":{"counters":[],"states":[]}}},"qualifiers":[]}});
    let part = if action {
        json!({"PlayerAction":{"player":controller(),"body":{"instructions":[{"kind":"Effect","args":{"result":24,"operation":moved}}]}}})
    } else {
        json!({"Move":{"selections":[{"objects":{"kind":"One","args":{"kind":"Source"}},"choose":null}],"destination":normal_exile()}})
    };
    let implicit = json!({"$sym":"zone_members","args":[{"$sym":"exile_followup","args":[{"kind":"One","args":{"$sym":"member_count_it","args":[{"$sym":"it","args":[],"at":10}],"at":10}}],"at":9}],"at":9});
    let returned = json!({"kind":"Effect","args":{"result":32,"operation":{"kind":"Move","args":{"objects":implicit,"destination":{"Battlefield":{"controller":"Owner","arrival":normal_entry()}},"qualifiers":[]}}}});
    let delay = json!({"kind":"Effect","args":{"result":31,"operation":{"kind":"CreateDelayedTrigger","args":{"event":{"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}},"start":"Immediately","expires":null,"intervening_if":null,"body":{"instructions":[returned]}}}}});
    let branch = json!({"kind":"If","args":{"condition":{"kind":"PaymentChosen","args":17},"then":{"instructions":[delay]},"otherwise":{"instructions":[]}}});
    let pay = json!({"kind":"Effect","args":{"result":17,"operation":{"kind":"PayCost","args":{"player":controller(),"cost":{"parts":[part]},"optional":true}}}});
    json!({"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},"body":{"instructions":[pay,branch]}}})
}
fn apply(value: &mut Value) {
    let note = prepare(value).unwrap();
    lower(value, note).unwrap();
}
fn ret_mut(value: &mut Value) -> &mut Value {
    &mut value["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0]["args"]["operation"]
        ["args"]["body"]["instructions"][0]["args"]["operation"]["args"]
}

#[test]
fn full_performed_after_population_and_strict_source_payment_share_actual_parent() {
    for action in [false, true] {
        let mut value = raw(action);
        let mut wanted = value.clone();
        ret_mut(&mut wanted)["objects"] = json!({"kind":"InZone","args":{"objects":{"kind":"CostObjects","args":{"payment":{"Resolution":17},"component":[0],"view":"Performed","phase":"After"}},"zone":"Exile"}});
        wanted["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"] = raw(false)["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"].clone();
        apply(&mut value);
        assert_eq!(value, wanted);
        validate_abilities(&json!([value])).unwrap();
    }
}

#[path = "paid_self_exile_guards.rs"]
mod guards;

#[path = "paid_self_exile_results_tests.rs"]
mod result_guards;
