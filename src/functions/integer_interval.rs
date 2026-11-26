//! An inclusive literal number choice, reversible from its current domain.
use crate::model::ir_schema::{Schema, Ty};
use crate::pattern::{Bindings, Pattern, Syntax, elaborate};
use serde_json::{Value, json};

const MAX_EXPLICIT: i64 = 64;

fn options(lower: i64, upper: i64) -> Result<Value, String> {
    if lower < 0 || upper < lower {
        return Err("number interval needs nonnegative ascending endpoints".into());
    }
    if upper - lower >= MAX_EXPLICIT {
        let bound = |comparison, n| json!({"kind":"Comparison","args":[comparison,{"kind":"Constant","args":n}]});
        return Ok(json!([
            bound("GreaterOrEqual", lower),
            bound("LessOrEqual", upper)
        ]));
    }
    Ok(Value::Array(
        (lower..=upper)
            .map(|n| json!({"kind":"Integer","args":n}))
            .collect(),
    ))
}

pub(crate) fn build(lower: &Value, upper: &Value) -> Result<Value, String> {
    let (Some(lower), Some(upper)) = (lower.as_i64(), upper.as_i64()) else {
        return Err("number interval needs integer endpoints".into());
    };
    options(lower, upper)
}

pub(crate) fn checked_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    let [lower, upper] = args else {
        return Err("@integer_interval needs two integers".into());
    };
    if *ty != Ty::Vec(Box::new(Ty::Named("Operand".into()))) {
        return Err("@integer_interval produces a list of operands".into());
    }
    let endpoint = Ty::Named("i64".into());
    Ok(Pattern::Symbol(
        "integer_interval".into(),
        vec![
            elaborate(lower, &endpoint, schema)?,
            elaborate(upper, &endpoint, schema)?,
        ],
    ))
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [lower, upper] = args else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    if items.is_empty() || items.len() > MAX_EXPLICIT as usize {
        return Vec::new();
    }
    let extract = |item: &Value| match item["kind"].as_str() {
        Some("Integer") => item["args"].as_i64(),
        Some("Comparison") => item["args"][1]["args"].as_i64(),
        _ => None,
    };
    let (Some(first), Some(last)) = (extract(&items[0]), extract(&items[items.len() - 1])) else {
        return Vec::new();
    };
    if !options(first, last).is_ok_and(|expected| expected == *value) {
        return Vec::new();
    }
    lower
        .unify_made(&Value::from(first), &bindings)
        .into_iter()
        .flat_map(|matched| upper.unify_made(&Value::from(last), &matched))
        .collect()
}
