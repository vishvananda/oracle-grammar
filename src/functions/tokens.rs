//! One reversible operation on a token description: its intrinsic abilities.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

pub(crate) fn with_token_abilities(base: &Value, abilities: &Value) -> Result<Value, String> {
    let mut recipe = base.clone();
    let field = recipe
        .pointer_mut("/Defined/abilities")
        .ok_or("intrinsic abilities need a defined token recipe")?;
    if !field.as_array().is_some_and(Vec::is_empty) || !abilities.is_array() {
        return Err("intrinsic abilities need an empty base and an ability list".into());
    }
    *field = abilities.clone();
    Ok(recipe)
}

pub(crate) fn unify_token_abilities<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [base, abilities] = args else {
        return Vec::new();
    };
    let Some(list) = value.pointer("/Defined/abilities").filter(|v| v.is_array()) else {
        return Vec::new();
    };
    let mut stripped = value.clone();
    stripped["Defined"]["abilities"] = json!([]);
    base.unify_made(&stripped, &bindings)
        .into_iter()
        .flat_map(|b| abilities.unify(list, b))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ir_schema::Ty;

    #[test]
    fn attaching_and_extracting_abilities_are_inverses() {
        for color in ["White", "Blue", "Green"] {
            for size in [0, 1, 3] {
                let base = json!({"Defined":{"colors":[color],"abilities":[],"subtypes":["Elf"]}});
                let abilities = Value::Array(
                    (0..size)
                        .map(|_| json!({"kind":"Keyword","args":{"kind":"Flying"}}))
                        .collect(),
                );
                let value = with_token_abilities(&base, &abilities).unwrap();
                let args = [
                    Pattern::Hole("base".into(), Ty::Opaque),
                    Pattern::Hole("abilities".into(), Ty::Opaque),
                ];
                let found = unify_token_abilities(&args, &value, Bindings::new());
                assert_eq!(found.len(), 1);
                assert_eq!(found[0].get("base"), Some(&base));
                assert_eq!(found[0].get("abilities"), Some(&abilities));
                assert_eq!(
                    with_token_abilities(
                        found[0].get("base").unwrap(),
                        found[0].get("abilities").unwrap()
                    )
                    .unwrap(),
                    value
                );
            }
        }
        assert!(with_token_abilities(&json!({"Copy":{}}), &json!([])).is_err());
        assert!(with_token_abilities(&json!({"Defined":{"abilities":[1]}}), &json!([])).is_err());
    }
}
