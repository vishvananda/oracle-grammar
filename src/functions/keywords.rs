//! The same keyword group can define abilities or changes that grant keywords.
use crate::model::ir_schema::Ty;
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

pub(crate) fn keyword_list_type(name: &str) -> Option<Ty> {
    let item = match name {
        "keyword_abilities" => "Ability",
        "keyword_changes" => "Change",
        _ => return None,
    };
    Some(Ty::Vec(Box::new(Ty::Named(item.into()))))
}

pub(crate) fn keyword_list(value: &Value, wrapper: &str) -> Result<Value, String> {
    let keywords = value.as_array().ok_or("a keyword group needs a list")?;
    Ok(Value::Array(
        keywords
            .iter()
            .map(|keyword| json!({"kind":wrapper,"args":keyword}))
            .collect(),
    ))
}

pub(crate) fn unify_keyword_list<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
    wrapper: &str,
) -> Vec<Bindings<'p, 'v>> {
    let [keywords] = args else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    let Some(values) = items
        .iter()
        .map(|item| {
            (item["kind"] == wrapper)
                .then(|| item.get("args").cloned())
                .flatten()
        })
        .collect::<Option<Vec<_>>>()
    else {
        return Vec::new();
    };
    keywords.unify_made(&Value::Array(values), &bindings)
}
