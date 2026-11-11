//! Chosen payment X names an actual result in its lexical program, in both directions.
use super::{Scope, Step, at_path};
use serde_json::{Value, json};

pub(super) fn marker(
    root: &mut Value,
    scope: &Scope,
    path: &[Step],
    name: &str,
) -> Result<bool, String> {
    if name == "announced_x" {
        let value = at_path(root, path).ok_or("announced cost part path")?;
        *value = value["args"][0].take();
        return Ok(true);
    }
    if !matches!(name, "payment_cost" | "announced_cost") {
        return Ok(false);
    }
    let value = at_path(root, path).ok_or("payment cost path")?;
    let definition = &value["args"][1];
    let cost = &value["args"][0];
    let variable = declared(cost);
    if name == "announced_cost" {
        if variable
            && (!crate::functions::is_x(definition)
                || !(scope.defined_x || scope.possible_announced_x))
        {
            return Err("this payment has no announced X".into());
        }
        *value = cost.clone();
    } else {
        if variable && scope.defined_x && crate::functions::is_x(definition) {
            return Err("X is already defined in this scope".into());
        }
        *value = crate::functions::payment::resolve(
            cost,
            (!crate::functions::is_x(definition)).then_some(definition),
        )?;
    }
    Ok(true)
}

/// Only the grammar's native variable resolution costs introduce this binding.
pub(super) fn bind(value: &mut Value) -> Result<(), String> {
    transform(value, false)
}
pub(super) fn surface(value: &mut Value) -> Result<(), String> {
    transform(value, true)
}

fn transform(value: &mut Value, inverse: bool) -> Result<(), String> {
    if has_variable_payment(value) {
        crate::functions::branch_bounds(value)?;
        walk(value, None, inverse);
    }
    Ok(())
}

fn chosen_payment(instruction: &Value) -> Option<u64> {
    let operation = &instruction["args"]["operation"];
    if instruction["kind"] != "Effect" || operation["kind"] != "PayCost" {
        return None;
    }
    operation["args"]["cost"]["parts"]
        .as_array()?
        .iter()
        .any(|p| p.get("ManaWithX").is_some_and(|x| x["x"].is_null()))
        .then(|| instruction["args"]["result"].as_u64())
        .flatten()
}

fn has_variable_payment(value: &Value) -> bool {
    chosen_payment(value).is_some()
        || match value {
            Value::Object(fields) => fields.values().any(has_variable_payment),
            Value::Array(items) => items.iter().any(has_variable_payment),
            _ => false,
        }
}

fn instructions(items: &mut [Value], mut active: Option<u64>, inverse: bool) -> Option<u64> {
    for item in items {
        if item["kind"] == "Group" {
            if let Some(group) = item["args"]["instructions"].as_array_mut() {
                active = instructions(group, active, inverse);
            }
        } else {
            walk(item, active, inverse);
            if let Some(result) = chosen_payment(item) {
                active = Some(result);
            }
        }
    }
    active
}

fn walk(value: &mut Value, active: Option<u64>, inverse: bool) {
    if let Some(result) = active {
        if !inverse && value["kind"] == "AnnouncedX" {
            *value = json!({"kind":"BoundValue","args":result});
            return;
        }
        if inverse && value["kind"] == "BoundValue" && value["args"].as_u64() == Some(result) {
            *value = json!({"kind":"AnnouncedX"});
            return;
        }
    }
    let fresh = matches!(
        crate::traverse::boundary(value),
        crate::traverse::Boundary::Ability | crate::traverse::Boundary::Mode
    );
    let active = if fresh { None } else { active };
    if let Some(items) = value.get_mut("instructions").and_then(Value::as_array_mut) {
        instructions(items, active, inverse);
        return;
    }
    crate::traverse::children(value, |child| walk(child, active, inverse));
}

/// An activation's declared X is already fixed before its body resolves.
pub(super) fn declared(value: &Value) -> bool {
    if value["$sym"] == "announced_x" || value["kind"] == "AnnouncedX" {
        return true;
    }
    if value
        .get("Mana")
        .and_then(Value::as_array)
        .is_some_and(|mana| mana.iter().any(|s| s == "X"))
    {
        return true;
    }
    if value
        .get("ManaWithX")
        .and_then(|v| v["mana"].as_array())
        .is_some_and(|mana| mana.iter().any(|s| s == "X"))
    {
        return true;
    }
    match value {
        Value::Object(fields) => fields.values().any(declared),
        Value::Array(items) => items.iter().any(declared),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pay(result: u64) -> Value {
        json!({"kind":"Effect","args":{"result":result,"operation":{"kind":"PayCost","args":{"player":{"kind":"AbilityController"},"cost":{"parts":[{"ManaWithX":{"mana":["X"],"x":null}}]},"optional":true}}}})
    }
    fn amount(value: Value) -> Value {
        json!({"kind":"Effect","args":{"result":200,"operation":{"kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":value}}}})
    }

    #[test]
    fn result_ids_groups_and_independent_payments_roundtrip() {
        let x = json!({"kind":"AnnouncedX"});
        let mut program = json!({"instructions":[amount(x.clone()),{"kind":"Group","args":{"instructions":[pay(70)]}},amount(x.clone()),pay(91),amount(x.clone()),amount(json!({"kind":"BoundValue","args":55}))]});
        let original = program.clone();
        bind(&mut program).unwrap();
        assert_eq!(
            program["instructions"][0]["args"]["operation"]["args"]["amount"],
            x
        );
        assert_eq!(
            program["instructions"][2]["args"]["operation"]["args"]["amount"],
            json!({"kind":"BoundValue","args":70})
        );
        assert_eq!(
            program["instructions"][4]["args"]["operation"]["args"]["amount"],
            json!({"kind":"BoundValue","args":91})
        );
        surface(&mut program).unwrap();
        assert_eq!(program, original);
    }

    #[test]
    fn branch_choices_do_not_escape_and_new_holders_keep_their_own_x() {
        let x = json!({"kind":"AnnouncedX"});
        let inner = json!({"kind":"Activated","args":{"cost":{"parts":[{"Mana":["X"]}]},"body":{"instructions":[amount(x.clone())]}}});
        let mut value = json!({"instructions":[{"kind":"If","args":{"then":{"instructions":[pay(71),amount(x.clone())]},"otherwise":{"instructions":[]}}},amount(x.clone()),pay(92),{"kind":"GrantAbility","args":inner}]});
        bind(&mut value).unwrap();
        assert_eq!(
            value["instructions"][0]["args"]["then"]["instructions"][1]["args"]["operation"]["args"]
                ["amount"],
            json!({"kind":"BoundValue","args":71})
        );
        assert_eq!(
            value["instructions"][1]["args"]["operation"]["args"]["amount"],
            x
        );
        assert_eq!(
            value["instructions"][3]["args"]["args"]["body"]["instructions"][0]["args"]["operation"]
                ["args"]["amount"],
            x
        );
    }

    #[test]
    fn only_cost_variables_declare_an_announced_x() {
        assert!(declared(&json!({"parts":[{"Mana":["X"]}]})));
        assert!(declared(
            &json!({"parts":[{"ManaWithX":{"mana":["X"],"x":null}}]})
        ));
        assert!(!declared(
            &json!({"parts":[{"Sacrifice":{"objects":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"Name","args":"X"}}}}}]})
        ));
    }
}
