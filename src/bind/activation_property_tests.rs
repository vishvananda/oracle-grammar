use super::super::Step;
use super::*;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/activation-sacrifice-properties-native.json"
    ))
    .unwrap()
}
fn ability(snapshot: bool) -> Value {
    fixture()[0]["expected_readings"][usize::from(snapshot)][0].clone()
}
fn path(snapshot: bool) -> Vec<Step> {
    [
        "args",
        "body",
        "instructions",
        "0",
        "args",
        "operation",
        "args",
        if snapshot { "value" } else { "amount" },
        "args",
        "object",
    ]
    .iter()
    .map(|key| {
        key.parse::<usize>()
            .map_or_else(|_| Step::Key((*key).into()), Step::Index)
    })
    .collect()
}
fn source() -> Value {
    json!({"kind":"Source"})
}
fn expected(component: usize) -> Value {
    json!({"kind":"Query","args":{"objects":{"kind":"CostObjects","args":{
        "payment":"Activation","component":[component],"view":"Inputs","phase":"Before"
    }},"index":0}})
}
fn lower(root: &Value, snapshot: bool) -> Result<Value, String> {
    reference(
        root,
        &Scope {
            cost_index: Some(99),
            ..Scope::default()
        },
        &path(snapshot),
        source(),
    )
}

#[test]
fn direct_and_capture_retire_source_using_the_actual_unique_component() {
    for captured in [false, true] {
        assert_eq!(lower(&ability(captured), captured).unwrap(), expected(2));
        let mut shifted = ability(captured);
        shifted["args"]["cost"]["parts"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"Life":{"kind":"Constant","args":2}}));
        assert_eq!(lower(&shifted, captured).unwrap(), expected(3));
    }
}

#[test]
fn lexical_wrapper_runs_after_children_and_replaces_both_old_property_operands() {
    for snapshot in [false, true] {
        let mut root = ability(snapshot);
        let p = path(snapshot);
        *at_path(&mut root, &p).unwrap() = json!({"$sym":NAME,"args":[source()],"at":100});
        assert!(marker(&mut root, &Scope::default(), &p, NAME).unwrap());
        assert_eq!(at_path(&mut root, &p).unwrap(), &expected(2));
    }
}

#[test]
fn variable_plural_optional_and_unproved_counts_cannot_infer_one_creature() {
    for choose in [
        json!({"Exactly":{"kind":"Constant","args":2}}),
        json!({"Exactly":{"kind":"AnnouncedX"}}),
        json!({"UpTo":{"kind":"Constant","args":1}}),
        json!("AnyNumber"),
    ] {
        let mut root = ability(false);
        root["args"]["cost"]["parts"][2]["Sacrifice"]["choose"] = choose;
        assert!(lower(&root, false).is_err());
    }
}

#[test]
fn multiple_nested_repeated_and_object_changing_payments_do_not_use_first_sacrifice() {
    let original = ability(false);
    let part = original["args"]["cost"]["parts"][2].clone();
    for parts in [
        json!([part, part]),
        json!([{"Choice":[{"parts":[part]}]}]),
        json!([{"Repeated":{"part":part,"times":{"kind":"Constant","args":1}}}]),
        json!([{"Constrained":{"part":part,"constraints":[]}}]),
        json!([part, "DiscardHand"]),
    ] {
        let mut root = original.clone();
        root["args"]["cost"]["parts"] = parts;
        assert!(lower(&root, false).is_err());
    }
}

#[test]
fn every_filter_branch_must_prove_a_creature() {
    for filter in [
        json!({"kind":"CardType","args":"Land"}),
        json!({"kind":"AnyOf","args":[{"kind":"CardType","args":"Creature"},{"kind":"CardType","args":"Artifact"}]}),
    ] {
        let mut root = ability(false);
        root["args"]["cost"]["parts"][2]["Sacrifice"]["objects"]["args"]["filter"] = filter;
        assert!(lower(&root, false).is_err());
    }
}

#[test]
fn a_target_receipt_or_granter_remains_the_existing_named_object() {
    let mut root = ability(false);
    root["args"]["announcement"]["targets"] = json!([{}]);
    for named in [
        json!({"kind":"Target","args":{"slot":0,"index":0}}),
        json!({"kind":"GrantingObject"}),
        expected(2),
    ] {
        assert_eq!(
            reference(&root, &Scope::default(), &path(false), named.clone()).unwrap(),
            named
        );
    }
    assert!(lower(&root, false).is_err());
    root["args"]["announcement"]["targets"] = json!([]);
    root["args"]["announcement"]["modes"] = json!({});
    assert!(lower(&root, false).is_err());
}

#[test]
fn explicit_source_properties_and_holder_sacrifices_are_not_reinterpreted() {
    let mut root = ability(false);
    assert!(!marker(&mut root, &Scope::default(), &path(false), "self").unwrap());
    root["args"]["cost"]["parts"] =
        json!([{"Sacrifice":{"objects":{"kind":"One","args":{"kind":"Source"}},"choose":null}}]);
    assert_eq!(lower(&root, false).unwrap(), source());
    root["kind"] = json!("Spell");
    assert_eq!(lower(&root, false).unwrap(), source());
}

#[test]
fn other_operations_scopes_prefixes_and_shared_x_do_not_gain_an_activation_receipt() {
    let mut root = ability(false);
    root["args"]["body"]["instructions"][0]["args"]["operation"]["kind"] = json!("GainLife");
    assert_eq!(lower(&root, false).unwrap(), source());
    root = ability(true);
    root["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["amount"] =
        json!({"kind":"BoundValue","args":999});
    assert_eq!(lower(&root, true).unwrap(), source());
    root = ability(false);
    let earlier = root["args"]["body"]["instructions"][0].clone();
    root["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .insert(0, earlier);
    assert_eq!(lower(&root, false).unwrap(), source());
}

#[test]
fn the_same_before_receipt_supports_edited_properties_and_arithmetic() {
    let mut root = ability(false);
    root["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"]["args"]["property"] =
        json!("Toughness");
    assert_eq!(lower(&root, false).unwrap(), expected(2));
    let value =
        root["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"].clone();
    root["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"] =
        json!({"kind":"Multiply","args":[{"kind":"Constant","args":2},value]});
    let mut p = path(false);
    p.splice(8..8, [Step::Key("args".into()), Step::Index(1)]);
    assert_eq!(
        reference(&root, &Scope::default(), &p, source()).unwrap(),
        expected(2)
    );
}

#[test]
fn inverse_keeps_exact_activation_receipt_shape_and_ordinary_references() {
    let args = vec![Pattern::Symbol("named_object".into(), vec![])];
    assert_eq!(unify(&args, &expected(2), Bindings::default()).len(), 1);
    for (field, value) in [
        ("phase", json!("After")),
        ("view", json!("Performed")),
        ("component", json!([2, 0])),
    ] {
        let mut wrong = expected(2);
        wrong["args"]["objects"]["args"][field] = value;
        assert!(unify(&args, &wrong, Bindings::default()).is_empty());
    }
    assert_eq!(unify(&args, &source(), Bindings::default()).len(), 1);
}

fn engine() -> &'static crate::engine::Engine {
    static ENGINE: std::sync::OnceLock<crate::engine::Engine> = std::sync::OnceLock::new();
    ENGINE.get_or_init(|| crate::engine::Engine::load().unwrap())
}

#[test]
fn actual_shadowheart_retires_both_source_readings_before_cli_relink() {
    let f = fixture();
    let c = &f[0];
    let name = c["name"].as_str().unwrap();
    let text = c["text"].as_str().unwrap();
    let actual = engine().parse(name, text).unwrap().readings;
    assert_eq!(actual.len(), 2);
    for expected in c["expected_readings"].as_array().unwrap() {
        let expected = crate::engine::canonical_abilities(expected).unwrap();
        assert!(actual.contains(&expected));
        let trail = engine().trail_of(name, &expected, text).unwrap().unwrap();
        assert_eq!(
            engine()
                .print_trail(name, &expected, &trail)
                .unwrap()
                .as_deref(),
            Some(text)
        );
        let printed = engine()
            .print_keyed(name, &expected, &[], &[])
            .unwrap()
            .unwrap();
        assert!(
            engine()
                .parse(name, &printed)
                .unwrap()
                .readings
                .contains(&expected)
        );
    }
    for wrong in c["wrong_old_readings"].as_array().unwrap() {
        assert!(!actual.contains(&crate::engine::canonical_abilities(wrong).unwrap()));
    }
}

#[test]
fn actual_edited_mana_and_property_print_before_cli_relink() {
    let f = fixture();
    let name = f[0]["name"].as_str().unwrap();
    let old_text = f[0]["text"].as_str().unwrap();
    let approved: Vec<_> = f[0]["expected_readings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|original| {
            let mut edited = original.clone();
            edited[0]["args"]["cost"]["parts"][0] =
                json!({"Mana":[{"Generic":3},{"Color":"Green"}]});
            let operation = &mut edited[0]["args"]["body"]["instructions"][0]["args"]["operation"];
            let field = if operation["kind"] == "Draw" {
                "amount"
            } else {
                "value"
            };
            operation["args"][field]["args"]["property"] = json!("Toughness");
            crate::engine::canonical_abilities(&edited).unwrap()
        })
        .collect();
    for expected in &approved {
        assert!(
            !engine()
                .parse(name, old_text)
                .unwrap()
                .readings
                .contains(expected)
        );
        let text = engine()
            .print_keyed(name, expected, &[], &[])
            .unwrap()
            .unwrap();
        assert!(
            text.contains("{3}{G}") && text.contains("toughness"),
            "{text}"
        );
        let actual = engine().parse(name, &text).unwrap().readings;
        assert!(actual.contains(expected), "{text}");
        assert!(
            actual.iter().all(|reading| approved.contains(reading)),
            "{text}"
        );
    }
}

#[test]
fn lexical_announcement_proves_both_bound_and_later_pending_targets_absent() {
    for snapshot in [false, true] {
        let mut root = ability(snapshot);
        root["args"]["announcement"] = json!({"$sym":"announcement","args":[],"at":0});
        assert_eq!(lower(&root, snapshot).unwrap(), expected(2));
        let mut malformed = root.clone();
        malformed["args"]["announcement"]["args"] = json!([{}]);
        assert!(lower(&malformed, snapshot).is_err());
        let scope = Scope {
            targets: vec![json!({"slot":0})],
            ..Scope::default()
        };
        assert!(reference(&root, &scope, &path(snapshot), source()).is_err());
        // The full footprint catches an introduction scheduled after the
        // possessive, even though scope.targets has not acquired it yet.
        let instruction = usize::from(snapshot);
        root["args"]["body"]["instructions"][instruction]["args"]["operation"]["args"]["players"] = json!({"kind":"One","args":{"kind":"Target","args":{"$sym":"target","args":[{"slot":0}],"at":999999}}});
        assert!(lower(&root, snapshot).is_err());
    }
}
