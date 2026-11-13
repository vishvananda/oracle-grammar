//! Unrepresentable counter-kind subsets must never become whole-component counts.
use super::*;

fn removal(kind: Value) -> Value {
    json!({"RemoveCounters":{"removals":[{
        "player":{"kind":"AbilityController"},
        "recipients":{"kind":"Objects","args":{"kind":"One","args":{"kind":"Source"}}},
        "kind":kind,"amount":{"Count":"AnyNumber"},"selection":null}]}})
}

fn typed_cost(parts: Vec<Value>) -> Cost {
    serde_json::from_value(json!({"parts":parts})).unwrap()
}

#[test]
fn counter_receipts_reject_mixed_unknown_duplicate_and_nested_components() {
    let lore = json!({"Named":"LoreCounter"});
    let charge = json!({"Named":"ChargeCounter"});
    let a = removal(lore.clone());
    let mut mixed = a.clone();
    let mut b = mixed["RemoveCounters"]["removals"][0].clone();
    b["kind"] = charge.clone();
    mixed["RemoveCounters"]["removals"]
        .as_array_mut()
        .unwrap()
        .push(b);
    for parts in [
        vec![mixed],
        vec![removal(Value::Null)],
        vec![a.clone(), a.clone()],
        vec![json!({"Choice":[{"parts":[a.clone()]}]})],
    ] {
        assert!(component(&typed_cost(parts), &lore).is_err());
    }
    assert_eq!(
        component(&typed_cost(vec![removal(charge), a]), &lore).unwrap(),
        1
    );
}

#[test]
fn counter_receipt_surface_is_bounded() {
    let mut deep = Value::Null;
    for _ in 0..140 {
        deep = json!([deep]);
    }
    assert!(surface(&json!({"kind":"Activated","deep":deep})).is_err());
    let wide = json!(vec![Value::Null; 20_000]);
    assert!(surface(&json!({"kind":"Activated","wide":wide})).is_err());
}
