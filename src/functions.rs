//! What the pattern functions compute, and how each is undone when printing:
//! conjunction, `@times`, `@apply`, `@where`, `@plus_one` and `@negate`.
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

mod branches;
pub(crate) mod capture_amount;
pub(crate) mod class_levels;
pub(crate) mod conditional;
mod continuous;
mod costs;
pub(crate) mod counts;
pub(crate) mod definitions;
pub(crate) mod payment;
pub(crate) mod unordered;
pub(crate) use branches::{branch_bounds, unify_kicker_branch};
pub(crate) use costs::{guard_cost, guard_counter_price, unify_cost_guard, unify_counter_price};
mod contracts;
pub(crate) mod entering;
mod events;
mod keywords;
mod mana;
pub(crate) mod trigger_batches;
pub(crate) use mana::{mana_options, unify_mana_options};
mod objects;
pub(crate) mod optional_scope;
mod repeated;
pub(crate) mod repeated_damage_actor;
pub(crate) mod target_scopes;
mod tokens;
pub(crate) mod value_snapshot;
pub(crate) use continuous::{parts as continuous_parts, unify_parts as unify_continuous_parts};
pub(crate) use continuous::{
    retain_animation, retain_animation_types, unify_retained_animation,
    unify_retained_animation_types,
};
pub(crate) use contracts::checked_call;
pub(crate) use events::{either_event, unify_either_event};
pub(crate) use keywords::{keyword_list, keyword_list_type, unify_keyword_list};
pub(crate) use objects::{unify_zone_members, zone_members};
pub(crate) use repeated::{agreed, unify_agree};
pub(crate) use tokens::{unify_token_abilities, with_token_abilities};

pub(crate) fn conjuncts(value: Value, out: &mut Vec<Value>) {
    match value.get("kind").and_then(Value::as_str) {
        Some("Any") => {}
        Some("AllOf") => {
            if let Some(Value::Array(items)) = value.get("args") {
                for item in items {
                    conjuncts(item.clone(), out);
                }
            }
        }
        _ => out.push(value),
    }
}

/// `@and`: the conjunction of filters as the IR holds it, flat, with no `All` of one.
pub(crate) fn conjunction(parts: Vec<Value>) -> Value {
    if parts
        .iter()
        .any(|p| p.get("$sym").is_some() || p.as_str().is_some())
    {
        return serde_json::json!({"kind":"AllOf","args":parts});
    }
    let mut items = Vec::new();
    for part in parts {
        conjuncts(part, &mut items);
    }
    match items.len() {
        0 => serde_json::json!({"kind": "Any"}),
        1 => items.remove(0),
        _ => serde_json::json!({"kind":"AllOf","args":items}),
    }
}

pub(crate) fn subset(items: &[Value], mask: usize, keep: bool) -> Value {
    let chosen: Vec<Value> = items
        .iter()
        .enumerate()
        .filter(|(index, _)| (mask >> index & 1 == 1) == keep)
        .map(|(_, item)| item.clone())
        .collect();
    conjunction(chosen)
}

pub(crate) fn constant(value: &Value) -> Option<i64> {
    (value.get("kind")? == "Constant")
        .then(|| value.get("args")?.as_i64())
        .flatten()
}

/// `@times`: N of something, where one of it is the thing itself.
pub(crate) fn product(factor: &Value, count: &Value) -> Value {
    if constant(factor) == Some(1) {
        count.clone()
    } else {
        serde_json::json!({"kind": "Multiply", "args": [factor, count]})
    }
}

pub(crate) fn unify_times<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [factor, count] = args else {
        return Vec::new();
    };
    let mut solutions = Vec::new();
    if value.get("kind").and_then(Value::as_str) == Some("Multiply")
        && let Some([first, second]) = value["args"].as_array().map(Vec::as_slice)
    {
        for b in factor.unify(first, bindings.clone()) {
            solutions.extend(count.unify(second, b));
        }
    }
    for b in factor.unify_made(
        &serde_json::json!({"kind": "Constant", "args": 1}),
        &bindings,
    ) {
        solutions.extend(count.unify(value, b));
    }
    solutions
}

pub(crate) fn is_subject(value: &Value) -> bool {
    value.get("$sym").and_then(Value::as_str) == Some("subject")
}

/// `@apply`: a verb phrase's `@subject` positions take the clause's subject.
pub(crate) fn applied(subject: &Value, mut body: Value) -> Value {
    fn walk(value: &mut Value, subject: &Value) {
        match value {
            Value::Object(object) => object.values_mut().for_each(|v| {
                if is_subject(v) {
                    *v = subject.clone();
                } else {
                    walk(v, subject);
                }
            }),
            Value::Array(items) => items.iter_mut().for_each(|v| walk(v, subject)),
            _ => {}
        }
    }
    walk(&mut body, subject);
    body
}

/// A subject as a value: the mention that introduces a target and one that
/// refers to it are the same player.
pub(crate) fn plain(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            if value.get("$sym").and_then(Value::as_str) == Some("target") {
                return value["args"][0]["slot"].clone();
            }
            Value::Object(object.iter().map(|(k, v)| (k.clone(), plain(v))).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(plain).collect()),
        other => other.clone(),
    }
}

/// A keyword action calls its player set `actor`; the other player verbs use
/// `players`. Only the immediate operation participates, never a nested holder.
fn subject_field(operation: &Value) -> &'static str {
    if operation["kind"] == "Keyword" {
        "actor"
    } else {
        "players"
    }
}

/// Going backward, the first verb supplies the shared player set. Every verb
/// acting on that set takes it as `@subject`, including mixed field spellings.
pub(crate) fn unify_apply<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [subject, body] = args else {
        return Vec::new();
    };
    let marker = serde_json::json!({"$sym": "subject", "args": [], "at": 0});
    // One operation on its own, not yet an instruction.
    if value.get("args").is_some() {
        let field = subject_field(value);
        let Some(players) = value.get("args").and_then(|a| a.get(field)) else {
            return Vec::new();
        };
        let mut abstracted = value.clone();
        abstracted["args"][field] = marker;
        return subject
            .unify(players, bindings)
            .into_iter()
            .flat_map(|b| body.unify_made(&abstracted, &b))
            .collect();
    }
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    // An eager scalar capture can precede the first verb. It has no actor;
    // the following Draw/Discard still share the written clause's subject.
    let Some(players) = items
        .iter()
        .find(|item| {
            item.pointer("/args/operation/kind")
                .is_none_or(|kind| kind != "BindValue")
        })
        .and_then(|item| {
            let operation = item.pointer("/args/operation")?;
            operation.get("args")?.get(subject_field(operation))
        })
    else {
        return Vec::new();
    };
    let mut abstracted = items.clone();
    let same = plain(players);
    for item in &mut abstracted {
        let Some(operation) = item.pointer_mut("/args/operation") else {
            continue;
        };
        let key = subject_field(operation);
        if let Some(field) = operation.get_mut("args").and_then(|args| args.get_mut(key))
            && plain(field) == same
        {
            *field = marker.clone();
        }
    }
    let abstracted = Value::Array(abstracted);
    subject
        .unify(players, bindings)
        .into_iter()
        .flat_map(|b| body.unify_made(&abstracted, &b))
        .collect()
}

pub(crate) fn is_x(value: &Value) -> bool {
    value.get("kind").and_then(Value::as_str) == Some("AnnouncedX")
}

/// `@where`: a sentence's `X` is the value its `where X is` clause names.
pub(crate) fn substituted(mut body: Value, value: &Value) -> Value {
    fn definition(value: &mut Value) {
        use crate::traverse::{Boundary, Visit};
        crate::traverse::rewrite(value, &mut |node| {
            if matches!(
                crate::traverse::boundary(node),
                Boundary::Filter
                    | Boundary::Holder
                    | Boundary::Lexical
                    | Boundary::Reflexive
                    | Boundary::Target
            ) {
                return Visit::Skip;
            }
            let name = match node.get("$sym").and_then(Value::as_str) {
                Some("it") => Some("defined_it"),
                Some("that_object") => Some("defined_object"),
                Some("named_object") => Some("defined_named_object"),
                Some("that_player" | "that_player_literal") => Some("defined_player"),
                _ => None,
            };
            if let Some(name) = name {
                node["$sym"] = Value::from(name);
            }
            Visit::Children
        });
    }
    // Substitution moves an expression structurally, not lexically. Keep the
    // definition's references outside the context of the filter receiving X.
    let mut value = value.clone();
    definition(&mut value);
    // This production substitutes operands, not the body node itself.
    crate::traverse::children(&mut body, |child| {
        crate::traverse::rewrite(child, &mut |node| {
            if is_x(node) {
                *node = value.clone();
                crate::traverse::Visit::Skip
            } else {
                crate::traverse::Visit::Children
            }
        });
    });
    body
}

/// What a `where X is` clause could have named: each computed number in the body.
pub(crate) fn computed_numbers(node: &Value, out: &mut Vec<Value>) {
    match node {
        Value::Object(object) => {
            // A native Pump owns two Expr operands. Enumerate those typed
            // positions explicitly, rather than treating arbitrary numeric
            // fields (or source/affected metadata) as where-clause values.
            if node["kind"] == "Pump" {
                for key in ["power", "toughness"] {
                    let item = &node["args"][key];
                    if item.get("kind").is_some() && constant(item).is_none() && !out.contains(item)
                    {
                        out.push(item.clone());
                    }
                }
            }
            // Conjure's count is an Expr, unlike arbitrary count metadata.
            if node["kind"] == "Conjure" {
                let item = &node["args"]["count"];
                if item.get("kind").is_some() && constant(item).is_none() && !out.contains(item) {
                    out.push(item.clone());
                }
            }
            // These are initial copiable Expr slots of a Defined token recipe,
            // not ability definitions or arbitrary numeric characteristic data.
            if let Some(stats) = node["Defined"]["power_toughness"].as_array() {
                for item in stats {
                    if item.get("kind").is_some() && constant(item).is_none() && !out.contains(item)
                    {
                        out.push(item.clone());
                    }
                }
            }
            for (key, item) in object {
                let payment_x = key == "x" && object.contains_key("mana");
                if (payment_x || matches!(key.as_str(), "amount" | "value" | "right"))
                    && item.get("kind").is_some()
                    && (payment_x || constant(item).is_none())
                    && !out.contains(item)
                {
                    out.push(item.clone());
                }
                computed_numbers(item, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| computed_numbers(item, out)),
        _ => {}
    }
}

pub(crate) fn replaced(mut node: Value, from: &Value, to: &Value) -> Value {
    crate::traverse::replace(&mut node, from, to);
    node
}

/// Going backward, any computed number may be the one `X` stands for.
pub(crate) fn unify_where<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [body, amount] = args else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    computed_numbers(value, &mut candidates);
    let x = serde_json::json!({"kind": "AnnouncedX"});
    let mut solutions = value_snapshot::unify_shared(args, value, bindings.clone());
    for candidate in candidates {
        if is_x(&candidate) {
            continue;
        }
        let abstracted = replaced(value.clone(), &candidate, &x);
        if value_snapshot::shared(&abstracted) && value_snapshot::shared_definition(&candidate) {
            continue;
        }
        for b in amount.unify_made(&candidate, &bindings) {
            solutions.extend(body.unify_made(&abstracted, &b));
        }
    }
    solutions
}

/// `@plus_one`: a count of others that excludes the object itself.
pub(crate) fn incremented(value: &Value) -> Result<Value, String> {
    constant(value)
        .map(|n| serde_json::json!({"kind": "Constant", "args": n + 1}))
        .ok_or_else(|| "a count of others must be a constant".to_string())
}

pub(crate) fn unify_plus_one<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else {
        return Vec::new();
    };
    match constant(value) {
        Some(n) if n > 0 => inner.unify_made(
            &serde_json::json!({"kind": "Constant", "args": n - 1}),
            &bindings,
        ),
        _ => Vec::new(),
    }
}

/// `@subtype_name`: the subtype itself out of a filter for it.
pub(crate) fn subtype_name(value: &Value) -> Value {
    value["args"].clone()
}

pub(crate) fn unify_subtype_name<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else {
        return Vec::new();
    };
    let filter = serde_json::json!({"kind": "Subtype", "args": value});
    inner.unify_made(&filter, &bindings)
}

/// `@negate`: a loyalty cost's minus sign.
pub(crate) fn negated(value: &Value) -> Value {
    match constant(value) {
        Some(number) => serde_json::json!({"kind": "Constant", "args": -number}),
        None => serde_json::json!({"kind": "Multiply", "args": [
            {"kind": "Constant", "args": -1}, value
        ]}),
    }
}

pub(crate) fn unify_negate<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else {
        return Vec::new();
    };
    let positive = match constant(value) {
        // Oracle writes `-2/-0`, so zero has a minus sign to print as well.
        Some(number) if number <= 0 => serde_json::json!({"kind": "Constant", "args": -number}),
        Some(_) => return Vec::new(),
        None => match value["args"].as_array().map(Vec::as_slice) {
            Some([factor, item]) if constant(factor) == Some(-1) => item.clone(),
            _ => return Vec::new(),
        },
    };
    inner.unify_made(&positive, &bindings)
}

/// The inverse of `@and` of two parts: every split of the conjuncts.
pub(crate) fn unify_and<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let mut items = Vec::new();
    conjuncts(value.clone(), &mut items);
    if items.len() > 10 {
        return Vec::new();
    }
    unify_conjuncts(args, &items, bindings)
}

/// Deal the conjuncts out among the arguments, in every way that fits.
pub(crate) fn unify_conjuncts<'p, 'v>(
    args: &'p [Pattern],
    items: &[Value],
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [first, rest @ ..] = args else {
        return Vec::new();
    };
    if rest.is_empty() {
        return first.unify_made(&subset(items, usize::MAX, true), &bindings);
    }
    let mut solutions = Vec::new();
    for mask in 0..(1usize << items.len()) {
        let remaining: Vec<Value> = items
            .iter()
            .enumerate()
            .filter(|(index, _)| mask >> index & 1 == 0)
            .map(|(_, item)| item.clone())
            .collect();
        for found in first.unify_made(&subset(items, mask, true), &bindings) {
            solutions.extend(unify_conjuncts(rest, &remaining, found));
        }
    }
    solutions
}

// Context metadata and retention share the bounded continuous-effect inverse.
pub(crate) fn unify_continuous<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    match name {
        "defining_static" | "defining_changes" => definitions::unify(name, args, value, bindings),
        "retain_animation_types" => unify_retained_animation_types(args, value, bindings),
        "retain_animation" => unify_retained_animation(args, value, bindings),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod conjure_count_inverse_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn computed_numbers_owns_conjure_count_but_not_arbitrary_count_metadata() {
        let value = json!({"kind":"CountCounters","args":{
            "recipients":{"kind":"Objects","args":{"kind":"One","args":{"kind":"EventSubject"}}},
            "kind":{"Named":"OilCounter"},"information":"CurrentOrLastKnown"}});
        let body = json!({"kind":"Conjure","args":{"count":value}});
        let mut candidates = Vec::new();
        computed_numbers(&body, &mut candidates);
        assert_eq!(candidates, vec![value.clone()]);
        computed_numbers(
            &json!({"kind":"Pump","args":{"power":value,"toughness":value}}),
            &mut candidates,
        );
        assert_eq!(candidates, vec![value.clone()]);
        let mut unrelated = Vec::new();
        computed_numbers(
            &json!({"kind":"UnrelatedMetadata","args":{"count":value}}),
            &mut unrelated,
        );
        assert!(unrelated.is_empty());
        computed_numbers(
            &json!({"kind":"Conjure","args":{"count":{"kind":"Constant","args":3}}}),
            &mut unrelated,
        );
        assert!(unrelated.is_empty());
    }
}
