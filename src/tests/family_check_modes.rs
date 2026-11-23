use super::*;

fn fixture_engine() -> Engine {
    let mut card = oracle_ir_grammar::engine::module_source("card").unwrap();
    card = card.replacen(
        "ability ::=",
        "ability ::= m_family_mode_seeded | family_mode_compositional |",
        1,
    );
    card.push_str(
        r#"
/* @sem Ability::Spell{announcement:_{targets:[]}, body:_{instructions:[Effect{result:7, operation:GainLife{players:One(AbilityController), amount:Constant(2)}}]}} */
m_family_mode_seeded ::= "seededmodephrase" "."
/* @sem Ability::Spell{announcement:_{targets:[]}, body:_{instructions:[Effect{result:7, operation:GainLife{players:One(AbilityController), amount:Constant(2)}}]}} */
family_mode_compositional ::= "reusablemodephrase" "."
"#,
    );
    Engine::load_with(&[("card".into(), card)]).unwrap()
}

#[test]
fn mixed_mode_uses_each_case_override_and_reports_actual_coverage() {
    let engine = fixture_engine();
    let off_seeded = engine.rule_set(|name| name.starts_with("m_") || name.starts_with("t_"));
    let off_none = RuleSet::default();
    let mut family: Family = serde_json::from_value(json!({
        "family":"mixed",
        "cases":[
            {"name":"Example", "text":"Reusablemodephrase."},
            {"name":"Example", "text":"Seededmodephrase.", "compositional":false}
        ]
    }))
    .unwrap();
    let results: Vec<_> = family
        .cases
        .iter()
        .map(|case| check_case(&engine, &off_seeded, &off_none, case, true))
        .collect();
    assert!(results.iter().all(Result::is_ok));
    let (report, passed) = report_family(&family, &results, true);
    assert!(passed);
    assert_eq!(report["compositional"], false);
    assert_eq!(report["compositional_cases"], 1);
    assert_eq!(report["seeded_cases"], 1);

    let expected = engine
        .parse("Example", "Seededmodephrase.")
        .unwrap()
        .readings
        .remove(0);
    family.cases[1].expected = Some(expected);
    assert!(check_case(&engine, &off_seeded, &off_none, &family.cases[1], true).is_ok());
    family.cases[1].expected = Some(json!([]));
    assert!(check_case(&engine, &off_seeded, &off_none, &family.cases[1], true).is_err());
    family.cases[1].expected = None;
    family.cases[1].compositional = None;
    assert!(check_case(&engine, &off_seeded, &off_none, &family.cases[1], true).is_err());
    assert!(check_case(&engine, &off_seeded, &off_none, &family.cases[1], false).is_ok());
    family.cases[1].compositional = Some(true);
    assert!(check_case(&engine, &off_seeded, &off_none, &family.cases[1], false).is_err());
}

#[test]
fn family_argument_count_accepts_large_batches_and_rejects_excess() {
    let mut args = vec!["family.json".to_string(); 65];
    args.push("--compositional".into());
    assert_eq!(family_paths(&args).unwrap().len(), 65);
    args.extend(vec!["another.json".to_string(); 16_319]);
    assert_eq!(family_paths(&args).unwrap().len(), 16_384);
    args.push("excess.json".into());
    assert!(family_paths(&args).is_err());
}

#[test]
fn aggregate_limits_use_actual_read_bytes_and_case_counts() {
    let fixture = br#"{"family":"x","cases":[{"name":"a","text":"a"}]}"#;
    let mut budget = FamilyBudget {
        remaining_bytes: fixture.len() * 2,
        remaining_cases: 2,
    };
    assert!(read_family(&fixture[..], &mut budget).is_ok());
    assert!(read_family(&fixture[..], &mut budget).is_ok());
    assert_eq!(budget.remaining_bytes, 0);
    assert_eq!(budget.remaining_cases, 0);
    assert!(read_family(&fixture[..], &mut budget).is_err());

    let mut bytes_limited = FamilyBudget {
        remaining_bytes: fixture.len() - 1,
        remaining_cases: 2,
    };
    assert!(read_family(&fixture[..], &mut bytes_limited).is_err());
    let mut cases_limited = FamilyBudget {
        remaining_bytes: fixture.len() * 2,
        remaining_cases: 1,
    };
    assert!(read_family(&fixture[..], &mut cases_limited).is_ok());
    assert!(read_family(&fixture[..], &mut cases_limited).is_err());
}
