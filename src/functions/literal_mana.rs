//! Split one literal unit from current color totals, in every supported order.
use crate::model::{
    canonical,
    ir::{Expr, ManaProduction},
};
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

/// Fixed declarations use the same literal normal form in both directions.
/// A hole, splice or semantic function leaves the declaration open and intact.
pub(crate) fn constant_pattern(pattern: Pattern) -> Pattern {
    fn closed(
        pattern: &Pattern,
        depth: usize,
        work: &mut usize,
        bytes: &mut usize,
    ) -> Option<Value> {
        if depth > 128 || *work == 0 || *bytes == 0 {
            return None;
        }
        *work -= 1;
        match pattern {
            Pattern::Value(value) => {
                super::branch_bounds(value).ok()?;
                *bytes = bytes.checked_sub(serde_json::to_vec(value).ok()?.len())?;
                Some(value.clone())
            }
            Pattern::Object(fields) => Some(Value::Object(
                fields
                    .iter()
                    .map(|(key, child)| Some((key.clone(), closed(child, depth + 1, work, bytes)?)))
                    .collect::<Option<_>>()?,
            )),
            Pattern::List(items) => Some(Value::Array(
                items
                    .iter()
                    .map(|item| match item {
                        crate::pattern::Item::One(child) => closed(child, depth + 1, work, bytes),
                        _ => None,
                    })
                    .collect::<Option<_>>()?,
            )),
            _ => None,
        }
    }
    let Pattern::Object(fields) = &pattern else {
        return pattern;
    };
    if !fields.iter().any(|(key, _)| key == "All") {
        return pattern;
    }
    let Some(value) = closed(&pattern, 0, &mut 32_768, &mut (1024 * 1024)) else {
        return pattern;
    };
    let Ok(typed) = serde_json::from_value::<ManaProduction>(value.clone()) else {
        return pattern;
    };
    if serde_json::to_value(&typed).ok().as_ref() != Some(&value) {
        return pattern;
    }
    let Some(normalized) = canonical::literal_mana(&typed) else {
        return pattern;
    };
    Pattern::Value(serde_json::to_value(normalized).expect("literal mana declaration"))
}

pub(crate) fn join(first: &Value, rest: &Value) -> Result<Value, String> {
    let mut parts = Vec::new();
    for value in [first, rest] {
        super::branch_bounds(value)?;
        let typed: ManaProduction = serde_json::from_value(value.clone())
            .map_err(|e| format!("literal mana sequence: {e}"))?;
        if serde_json::to_value(&typed).map_err(|e| e.to_string())? != *value {
            return Err("literal mana sequence has undeclared fields".into());
        }
        parts.push(typed);
    }
    let production = canonical::literal_mana(&ManaProduction::All(parts))
        .ok_or("literal mana sequence needs positive fixed units without overflow")?;
    serde_json::to_value(production).map_err(|e| e.to_string())
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [first, rest] = args else {
        return Vec::new();
    };
    if super::branch_bounds(value).is_err() {
        return Vec::new();
    }
    let Ok(typed) = serde_json::from_value::<ManaProduction>(value.clone()) else {
        return Vec::new();
    };
    if serde_json::to_value(&typed).ok().as_ref() != Some(value) {
        return Vec::new();
    }
    let Some(production) = canonical::literal_mana(&typed) else {
        return Vec::new();
    };
    let parts = match production {
        ManaProduction::All(parts) => parts,
        one => vec![one],
    };
    let mut output = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        let ManaProduction::Amount {
            color,
            amount: Expr::Constant(n),
        } = part
        else {
            continue;
        };
        let mut remainder = parts.clone();
        if *n == 1 {
            remainder.remove(index);
        } else {
            remainder[index] = ManaProduction::Amount {
                color: color.clone(),
                amount: Expr::Constant(n - 1),
            };
        }
        if remainder.is_empty() {
            continue;
        }
        let unit = serde_json::to_value(ManaProduction::Amount {
            color: color.clone(),
            amount: Expr::Constant(1),
        })
        .expect("literal mana unit");
        let tail = if remainder.len() == 1 {
            remainder.remove(0)
        } else {
            ManaProduction::All(remainder)
        };
        let tail = serde_json::to_value(tail).expect("literal mana remainder");
        for solution in first.unify_made(&unit, &bindings) {
            output.extend(rest.unify_made(&tail, &solution));
        }
    }
    output
}
