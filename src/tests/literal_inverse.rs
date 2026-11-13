use super::*;

fn literal_engine() -> Engine {
    let mut card = crate::engine::module_source("card").unwrap();
    card = card.replacen("ability ::=", "ability ::= inverse_literal_case |", 1);
    card.push_str(
        r#"
/* @sem Ability::Spell{announcement:_{targets:[]}, body:_{instructions:[Group{instructions:[Effect{result:7, operation:GainLife{players:One(AbilityController), amount:Constant(2)}}]}]}} */
inverse_literal_case ::= "settle" "."
"#,
    );
    Engine::load_with(&[("card".into(), card)]).unwrap()
}

#[test]
fn closed_declarations_print_after_group_and_result_normalization() {
    let engine = literal_engine();
    let reading = engine
        .parse("Example", "Settle.")
        .unwrap()
        .readings
        .remove(0);
    assert_eq!(
        reading[0]["args"]["body"]["instructions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        reading[0]["args"]["body"]["instructions"][0]["kind"],
        "Effect"
    );
    assert_eq!(
        reading[0]["args"]["body"]["instructions"][0]["args"]["result"],
        u32::MAX
    );
    let trail = engine
        .trail_of("Example", &reading, "Settle.")
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .print_trail("Example", &reading, &trail)
            .unwrap()
            .as_deref(),
        Some("Settle.")
    );
    assert_eq!(
        engine
            .print_keyed("Example", &reading, &[], &trail)
            .unwrap()
            .as_deref(),
        Some("Settle.")
    );

    let mut invalid = reading.clone();
    invalid[0]["unexpected"] = json!(true);
    assert!(engine.print_trail("Example", &invalid, &trail).is_err());
    assert!(engine.print_keyed("Example", &invalid, &[], &[]).is_err());

    let mut edited = reading;
    edited[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"]["args"] =
        json!(3);
    validate_abilities(&edited).unwrap();
    assert!(
        engine
            .print_trail("Example", &edited, &trail)
            .unwrap()
            .is_none()
    );
    assert!(
        engine
            .trail_of("Example", &edited, "Settle.")
            .unwrap()
            .is_none()
    );
    assert!(
        engine
            .print_keyed("Example", &edited, &[], &trail)
            .unwrap()
            .is_none()
    );
    let text = engine
        .print_keyed("Example", &edited, &[], &[])
        .unwrap()
        .unwrap();
    assert!(
        engine
            .parse("Example", &text)
            .unwrap()
            .readings
            .contains(&edited)
    );
    assert_ne!(text, "Settle.");
}

#[test]
fn die_table_literal_has_verified_default_and_exact_trail() {
    let case: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/literal-die-table.json")).unwrap();
    let name = case["name"].as_str().unwrap();
    let text = case["text"].as_str().unwrap();
    let wanted = canonical_abilities(&case["expected"]).unwrap();
    assert!(
        engine()
            .parse(name, text)
            .unwrap()
            .readings
            .contains(&wanted)
    );
    let trail = engine()
        .trail_of(name, &wanted, text)
        .unwrap()
        .expect("literal inverse trail");
    assert_eq!(
        engine()
            .print_trail(name, &wanted, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
    let printed = engine()
        .print_keyed(name, &wanted, &["Instant".into()], &[])
        .unwrap()
        .expect("verified default");
    assert!(
        engine()
            .parse(name, &printed)
            .unwrap()
            .readings
            .contains(&wanted)
    );
}

#[test]
fn literal_normal_forms_do_not_crowd_existing_verified_defaults() {
    let case: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/literal-inverse-control.json"
    ))
    .unwrap();
    let name = case["name"].as_str().unwrap();
    let wanted = canonical_abilities(&case["expected"]).unwrap();
    assert!(
        engine()
            .parse(name, case["text"].as_str().unwrap())
            .unwrap()
            .readings
            .contains(&wanted)
    );
    let text = engine()
        .print_keyed(name, &wanted, &["Enchantment".into()], &[])
        .unwrap()
        .expect("existing verified default");
    assert!(
        engine()
            .parse(name, &text)
            .unwrap()
            .readings
            .contains(&wanted)
    );
}
