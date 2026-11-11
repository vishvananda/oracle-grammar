//! Resolution costs distinguish a newly chosen X from a supplied definition.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

pub(crate) fn resolve(cost: &Value, definition: Option<&Value>) -> Result<Value, String> {
    super::branch_bounds(cost)?;
    if let Some(definition) = definition {
        super::branch_bounds(definition)?;
    }
    let mut cost = cost.clone();
    let parts = cost["parts"]
        .as_array_mut()
        .ok_or("payment needs Cost.parts")?;
    for part in parts {
        if let Some(mana) = part.get("Mana") {
            let symbols = mana.as_array().ok_or("payment mana symbols")?;
            if symbols.iter().any(|s| s == "X") {
                *part = json!({"ManaWithX":{"mana":symbols,"x":definition.cloned().unwrap_or(Value::Null)}});
            }
        }
    }
    Ok(cost)
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [cost, definition] = args else {
        return Vec::new();
    };
    let Some(parts) = value["parts"].as_array() else {
        return Vec::new();
    };
    let mut plain = Vec::new();
    let mut amounts = Vec::new();
    for part in parts {
        if let Some(variable) = part.get("ManaWithX") {
            let amount = (!variable["x"].is_null()).then(|| variable["x"].clone());
            if amounts.last().is_some_and(|old| *old != amount) {
                return Vec::new();
            }
            amounts.push(amount);
            plain.push(json!({"Mana":variable["mana"]}));
        } else {
            plain.push(part.clone());
        }
    }
    let amount = amounts.pop().flatten();
    let plain = json!({"parts":plain});
    if resolve(&plain, amount.as_ref()).as_ref() != Ok(value) {
        return Vec::new();
    }
    let amount = amount.unwrap_or_else(|| json!({"kind":"AnnouncedX"}));
    definition
        .unify_made(&amount, &bindings)
        .into_iter()
        .flat_map(|b| cost.unify_made(&plain, &b))
        .collect()
}

/// An announced-cost spelling keeps the original mana symbols and introduces no X.
pub(crate) fn unify_announced<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [cost, definition] = args else {
        return Vec::new();
    };
    definition
        .unify_made(&json!({"kind":"AnnouncedX"}), &bindings)
        .into_iter()
        .flat_map(|b| cost.unify(value, b))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_payments_keep_non_mana_parts_and_all_symbols() {
        let cost = json!({"parts":[{"Mana":["X", "X", {"Color":"Blue"}]},{"Life":{"kind":"Constant","args":2}}]});
        let chosen = resolve(&cost, None).unwrap();
        assert_eq!(
            chosen["parts"][0],
            json!({"ManaWithX":{"mana":["X","X",{"Color":"Blue"}],"x":null}})
        );
        assert_eq!(chosen["parts"][1], cost["parts"][1]);
        let fixed = resolve(&cost, Some(&json!({"kind":"Constant","args":3}))).unwrap();
        assert_eq!(
            fixed["parts"][0]["ManaWithX"]["x"],
            json!({"kind":"Constant","args":3})
        );
    }
}
