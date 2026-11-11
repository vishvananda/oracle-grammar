use super::*;

fn creature() -> Value {
    json!({"kind":"CardType","args":"Creature"})
}
fn target_ref(slot: u64) -> Value {
    json!({"kind":"Target","args":{"slot":slot,"index":0}})
}
fn scope() -> Scope {
    super::super::scope_for(&json!({"kind":"Spell","args":{}}), "Fixture", 0)
}
fn remember(scope: &mut Scope, slot: u64, filter: Value) {
    scope.references.push(described(
        target_ref(slot),
        vec![ObjectKind::Permanent],
        &filter,
        true,
    ));
}
fn node() -> Value {
    json!({"args":["Permanent",creature()]})
}

#[test]
fn facts_union_conjunction_and_intersect_nonempty_disjunction() {
    let equipment = json!({"kind":"Subtype","args":"Equipment"});
    let one = described(
        Value::Null,
        vec![],
        &json!({"kind":"AllOf","args":[creature(),equipment]}),
        true,
    );
    assert!(contains(&one.types, &CardType::Creature));
    assert!(!one.subtypes.is_empty());
    for filter in [
        json!({"kind":"AnyOf","args":[creature(),{"kind":"CardType","args":"Land"}]}),
        json!({"kind":"Not","args":creature()}),
        json!({"kind":"AnyOf","args":[]}),
    ] {
        assert!(
            described(Value::Null, vec![], &filter, true)
                .types
                .is_empty()
        );
    }
    let filter = json!({"kind":"AnyOf","args":[{"kind":"AllOf","args":[creature(),{"kind":"Color","args":"Red"}]},creature()]});
    assert_eq!(
        serde_json::to_value(described(Value::Null, vec![], &filter, true).types).unwrap(),
        json!(["Creature"])
    );
}

#[test]
fn unique_proven_noun_recovers_across_unrelated_focus_and_deduplicates() {
    let mut scope = scope();
    remember(&mut scope, 2, creature());
    remember(&mut scope, 3, json!({"kind":"Subtype","args":"Equipment"}));
    scope.named_object = Some(target_ref(3));
    assert_eq!(resolve(&node(), &scope, false).unwrap(), target_ref(2));
    remember(&mut scope, 2, creature());
    assert_eq!(resolve(&node(), &scope, false).unwrap(), target_ref(2));
    remember(&mut scope, 4, creature());
    assert!(resolve(&node(), &scope, false).is_err());
    scope.named_object = Some(target_ref(4));
    assert_eq!(resolve(&node(), &scope, false).unwrap(), target_ref(4));
}

#[test]
fn unknown_group_and_announced_receipts_do_not_prove_single_noun() {
    let mut scope = scope();
    scope.named_object = Some(json!({"kind":"Source"}));
    remember(&mut scope, 2, json!({"kind":"Any"}));
    let mut group = described(
        target_ref(3),
        vec![ObjectKind::Permanent],
        &creature(),
        false,
    );
    scope.references.push(group.clone());
    assert_eq!(
        resolve(&node(), &scope, false).unwrap(),
        json!({"kind":"Source"})
    );
    group.reference = json!({"kind":"Result","args":{"result":7,"phase":"After","index":0}});
    group.single = true;
    scope.references.push(group);
    assert_eq!(
        resolve(&node(), &scope, true).unwrap(),
        json!({"kind":"Source"})
    );
}

#[test]
fn objectless_player_event_never_turns_default_source_into_that_card() {
    let mut scope = Scope {
        objectless_player_event: true,
        objects: vec![json!({"kind":"Source"})],
        ..Scope::default()
    };
    assert!(card(&scope).is_err());
    let target = json!({"kind":"Target","args":{"slot":0,"index":0}});
    scope.objects.push(target.clone());
    assert_eq!(card(&scope).unwrap(), target);
    scope.objects.pop();
    scope.objects.push(json!({"kind":"Source"}));
    assert_eq!(card(&scope).unwrap(), json!({"kind":"Source"}));
    scope.objects.pop();
    scope.objectless_player_event = false;
    assert_eq!(card(&scope).unwrap(), json!({"kind":"Source"}));
}
