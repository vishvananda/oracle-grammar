//! Inverse shapes for contextual group and quantity words. Reparse checks scope.
use super::{Bindings, Pattern, Value};
use std::borrow::Cow;

pub(super) fn is_context(name: &str) -> bool {
    matches!(
        name,
        "card_name" | "group_reference" | "that_much" | "that_many"
    )
}

pub(super) fn context<'p, 'v>(
    name: &str,
    value: &'v Value,
    mut bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    if name == "card_name" {
        bindings.insert(super::CARD_NAME, Cow::Borrowed(value));
        vec![bindings]
    } else {
        unify(name, value, bindings)
    }
}

pub(super) fn sacrificed_damage_source<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    if value["kind"] != "Query"
        || value["args"]["index"] != 0
        || value["args"]["objects"]["kind"] != "CostObjects"
    {
        return Vec::new();
    }
    let paid = &value["args"]["objects"]["args"];
    if paid["payment"] != "Activation"
        || paid["view"] != "Inputs"
        || paid["phase"] != "Before"
        || !paid["component"]
            .as_array()
            .is_some_and(|path| path.len() == 1 && path[0].as_u64().is_some())
    {
        return Vec::new();
    }
    args[0].unify_made(&serde_json::json!({"kind":"Source"}), &bindings)
}

pub(super) fn return_ref<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let Some(inner) = args.first() else {
        return Vec::new();
    };
    if value["kind"] == "AuraAfterEnchantedLeft" {
        if value["args"]["aura"] != serde_json::json!({"kind":"Source"})
            || value["args"]["host"] != serde_json::json!({"kind":"EventSubject"})
        {
            return Vec::new();
        }
        return inner.unify_made(&serde_json::json!({"kind":"Source"}), &bindings);
    }
    inner.unify(value, bindings)
}

pub(super) fn unify<'p, 'v>(
    name: &str,
    value: &Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    if name == "group_reference" {
        let shape = if value["kind"] == "InZone" {
            &value["args"]["objects"]
        } else {
            value
        };
        return if shape["kind"] == "EventGroup"
            || shape["kind"] == "Targets" && shape["args"].is_u64()
            || shape["kind"] == "Transition" && shape["args"]["phase"] == "After"
        {
            vec![bindings]
        } else {
            Vec::new()
        };
    }
    if value["kind"] == "CountObjects" && value["args"]["kind"] == "EventGroup" {
        return vec![bindings];
    }
    let kinds: &[&str] = if name == "that_much" {
        &["EventAmount", "Performed"]
    } else {
        &["EventAmount", "CountObjects"]
    };
    let counted = value["kind"] != "CountObjects" || value["args"]["kind"] == "Transition";
    match value.get("kind").and_then(Value::as_str) {
        Some(kind) if kinds.contains(&kind) && counted => vec![bindings],
        _ => Vec::new(),
    }
}

/// A qualified plural can refer to one object or one member of a created batch.
pub(super) fn qualified_members<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let Some(inner) = args.first() else {
        return Vec::new();
    };
    if value["kind"] == "One" {
        return inner.unify(&value["args"], bindings);
    }
    if value["kind"] == "Created" && value["args"].is_u64() {
        return inner.unify_made(
            &serde_json::json!({"kind":"Result","args":{
                "result":value["args"],"phase":"After","index":0
            }}),
            &bindings,
        );
    }
    Vec::new()
}
