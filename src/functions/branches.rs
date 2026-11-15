//! Inverse of native casting-branch specialization: shared prefix/suffix and one test,
//! or for overload the ordinary spell itself.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

pub(crate) fn branch_bounds(value: &Value) -> Result<(), String> {
    let mut pending = vec![(value, 0)];
    let mut nodes = 0;
    let mut bytes = 0;
    while let Some((value, depth)) = pending.pop() {
        nodes += 1;
        if nodes > 16_384 || depth > 128 || bytes > 1024 * 1024 {
            return Err("branch semantic work bound".into());
        }
        match value {
            Value::Object(items) => {
                if pending.len() + nodes + items.len() > 16_384 {
                    return Err("branch semantic work bound".into());
                }
                for (key, child) in items {
                    bytes += key.len();
                    pending.push((child, depth + 1));
                }
            }
            Value::Array(items) => {
                if pending.len() + nodes + items.len() > 16_384 {
                    return Err("branch semantic work bound".into());
                }
                pending.extend(items.iter().map(|v| (v, depth + 1)));
            }
            Value::String(text) => bytes += text.len(),
            _ => {}
        }
        if bytes > 1024 * 1024 {
            return Err("branch semantic work bound".into());
        }
    }
    Ok(())
}

// Effect definitions receive fresh ids when each branch is canonicalized. The
// inverse may share their spelling; the final reparse still checks every use.
fn same_spelling(left: &Value, right: &Value) -> bool {
    if left["$sym"] == "target" && right["$sym"] == "target" {
        let mut a = left["args"][0].clone();
        let mut b = right["args"][0].clone();
        if let (Some(a), Some(b)) = (a.as_object_mut(), b.as_object_mut()) {
            a.remove("slot");
            b.remove("slot");
        }
        return a == b;
    }
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            let effect = left["kind"] == "Effect" && right["kind"] == "Effect";
            a.len() == b.len()
                && a.iter().all(|(key, value)| {
                    b.get(key).is_some_and(|other| {
                        if effect && key == "args" {
                            same_spelling(&value["operation"], &other["operation"])
                        } else {
                            same_spelling(value, other)
                        }
                    })
                })
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_spelling(a, b))
        }
        _ => left == right,
    }
}

fn spell(value: &Value) -> Option<&[Value]> {
    (value["kind"] == "Spell" && value["args"]["announcement"]["modes"].is_null())
        .then(|| {
            value["args"]["body"]["instructions"]
                .as_array()
                .map(Vec::as_slice)
        })
        .flatten()
}

/// What a casting branch turns on: a kicker, an optional additional cost or a gift.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Choice {
    Kicker,
    Additional,
    Gift,
}

impl Choice {
    pub(crate) fn of(symbol: &str) -> Option<Self> {
        match symbol {
            "kicker_branch" => Some(Self::Kicker),
            "additional_branch" => Some(Self::Additional),
            "gift_branch" => Some(Self::Gift),
            _ => None,
        }
    }

    /// The `CostBranch` keyword a paid cost names.
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            Self::Kicker => "Kicker",
            Self::Additional => "AdditionalCost",
            Self::Gift => "Gift",
        }
    }

    /// The test of the spell itself that its paid branch answers yes.
    pub(crate) fn paid_test(self) -> Value {
        let state = match self {
            Self::Kicker => json!("Kicked"),
            Self::Additional => json!({"CostPaid":"Additional"}),
            Self::Gift => return json!({"kind":"GiftPromised","args":true}),
        };
        json!({"kind":"ObjectMatches","args":{"object":{"kind":"Source"},
            "filter":{"kind":"State","args":state}}})
    }
}

/// The cost a "was beheld" test names: one creature of a kind.
pub(crate) fn beheld_kind(cost: &Value) -> Option<&Value> {
    match cost["parts"].as_array()?.as_slice() {
        [part] if part["Behold"]["count"] == json!({"kind":"Constant","args":1}) => {
            Some(&part["Behold"]["what"])
        }
        _ => None,
    }
}

/// Each spelling of the branch test, with whether it holds on the paid side. A gift
/// may be tested either way; an optional cost to behold also asks what was beheld.
fn branch_tests(kind: Choice, cost: &Value) -> Vec<(bool, Value)> {
    if kind == Choice::Gift {
        return [true, false]
            .map(|promised| (promised, json!({"kind":"GiftPromised","args":promised})))
            .to_vec();
    }
    let mut tests = vec![(true, kind.paid_test())];
    if let Some(what) = beheld_kind(cost).filter(|_| kind == Choice::Additional) {
        tests.push((true, json!({"$sym":"beheld","args":[what]})));
    }
    tests
}

/// "If a Dragon creature was beheld, it perpetually gets +2/+2": the branch's own
/// payment is an object no ordinary production names, so no spelling of this
/// branch can print it.
fn reads_branch_payment(value: &Value, depth: usize) -> bool {
    depth <= 128
        && ((value["kind"] == "CostObjects" && value["args"]["payment"] == "Branch")
            || match value {
                Value::Object(fields) => {
                    fields.values().any(|v| reads_branch_payment(v, depth + 1))
                }
                Value::Array(items) => items.iter().any(|v| reads_branch_payment(v, depth + 1)),
                _ => false,
            })
}

/// Whether a symbol specializes one spell for a choice made as it is cast.
pub(crate) fn casting_branch(name: &str) -> bool {
    Choice::of(name).is_some() || name == "overload_branch"
}

/// The inverse of a branch over one spell: a cost choice, overload, or a condition.
pub(crate) fn unify_branch<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    match (Choice::of(name), name) {
        (Some(kind), _) => unify_cost_choice(kind, args, value, bindings),
        (None, "overload_branch") => unify_overload_branch(args, value, bindings),
        _ => super::conditional::unify(args, value, bindings),
    }
}

fn unify_cost_choice<'p, 'v>(
    kind: Choice,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let gift = kind == Choice::Gift;
    let [choice, source] = args else {
        return Vec::new();
    };
    if branch_bounds(value).is_err()
        || reads_branch_payment(value, 0)
        || if gift {
            value["kind"] != "Gift"
        } else {
            value["kind"] != "CostBranch" || value["args"]["keyword"] != kind.keyword()
        }
    {
        return Vec::new();
    }
    let (paid_key, unpaid_key) = if gift {
        ("promised", "unpromised")
    } else {
        ("paid", "unpaid")
    };
    let Some(paid) = spell(&value["args"][paid_key]) else {
        return Vec::new();
    };
    let Some(unpaid) = spell(&value["args"][unpaid_key]) else {
        return Vec::new();
    };
    if paid.len() + unpaid.len() > 128 {
        return Vec::new();
    }
    let mut head = value["args"]["cost"].clone();
    if gift {
        head = value.clone();
        head["args"]["promised"] = Value::Null;
        head["args"]["unpromised"] = Value::Null;
    }
    let prefix = paid
        .iter()
        .zip(unpaid)
        .take_while(|(p, u)| same_spelling(p, u))
        .count();
    let suffix = paid
        .iter()
        .rev()
        .zip(unpaid.iter().rev())
        .take_while(|(p, u)| same_spelling(p, u))
        .count();
    let mut candidates = Vec::new();
    if gift && paid.len() == unpaid.len() && prefix == paid.len() {
        candidates.push(paid.to_vec());
    }
    let tests = branch_tests(kind, &value["args"]["cost"]);
    for start in (0..=prefix).rev() {
        for end in (0..=suffix.min(paid.len().min(unpaid.len()) - start)).rev() {
            for (positive, condition) in &tests {
                if candidates.len() == 64 {
                    return unify_candidates(choice, source, &head, &candidates, &bindings);
                }
                let (yes, no) = if *positive {
                    (
                        &paid[start..paid.len() - end],
                        &unpaid[start..unpaid.len() - end],
                    )
                } else {
                    (
                        &unpaid[start..unpaid.len() - end],
                        &paid[start..paid.len() - end],
                    )
                };
                let mut instructions = paid[..start].to_vec();
                if gift
                    && no.is_empty()
                    && yes.len() == 1
                    && yes[0]["kind"] == "If"
                    && yes[0]["args"]["otherwise"]["instructions"] == json!([])
                {
                    let mut combined = instructions.clone();
                    let mut inner = yes[0].clone();
                    inner["args"]["condition"] =
                        json!({"kind":"AllOf","args":[condition, inner["args"]["condition"]]});
                    combined.push(inner);
                    combined.extend_from_slice(&paid[paid.len() - end..]);
                    candidates.push(combined);
                    if candidates.len() == 64 {
                        return unify_candidates(choice, source, &head, &candidates, &bindings);
                    }
                }
                instructions.push(json!({"kind":"If","args":{"condition":condition,"then":{"instructions":yes},"otherwise":{"instructions":no}}}));
                instructions.extend_from_slice(&paid[paid.len() - end..]);
                candidates.push(instructions);
            }
        }
    }
    unify_candidates(choice, source, &head, &candidates, &bindings)
}

/// The overloaded branch is the ordinary spell with "target" changed to "each", so
/// only the ordinary spell is printed.
fn unify_overload_branch<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [choice, source] = args else {
        return Vec::new();
    };
    let unpaid = &value["args"]["unpaid"];
    if branch_bounds(value).is_err()
        || value["kind"] != "CostBranch"
        || value["args"]["keyword"] != "Overload"
        || spell(unpaid).is_none()
        || super::overload::paid(unpaid).as_ref() != Ok(&value["args"]["paid"])
    {
        return Vec::new();
    }
    let mut found = Vec::new();
    for bound in choice.unify_made(&value["args"]["cost"], &bindings) {
        found.extend(source.unify_made(unpaid, &bound));
    }
    found
}

fn unify_candidates<'p, 'v>(
    choice: &'p Pattern,
    source: &'p Pattern,
    head: &Value,
    candidates: &[Vec<Value>],
    bindings: &Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let mut found = Vec::new();
    for instructions in candidates {
        let made = json!({"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},"body":{"instructions":instructions}}});
        for bound in choice.unify_made(head, bindings) {
            found.extend(source.unify_made(&made, &bound));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_bounds_cover_last_strings_wide_arrays_and_depth() {
        assert!(branch_bounds(&json!("x".repeat(1024 * 1024 + 1))).is_err());
        assert!(branch_bounds(&json!(vec![0; 16_384])).is_err());
        let mut nested = json!(0);
        for _ in 0..129 {
            nested = json!([nested]);
        }
        assert!(branch_bounds(&nested).is_err());
        assert!(branch_bounds(&json!({"instructions":[]})).is_ok());
    }
}
