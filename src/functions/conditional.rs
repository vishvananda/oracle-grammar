//! One available ability, preserving any zones in which it functions.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

pub(crate) fn wrap(condition: Value, mut ability: Value) -> Value {
    if ability["kind"] == "Scoped" {
        let inner = ability["args"]["ability"].take();
        ability["args"]["ability"] = wrap(condition, inner);
        ability
    } else {
        json!({"kind":"Conditional","args":{
            "condition":condition,"then":[ability],"otherwise":[]
        }})
    }
}

fn split(value: &Value) -> Option<(Value, Value)> {
    if value["kind"] == "Scoped" {
        let (condition, inner) = split(&value["args"]["ability"])?;
        let mut scoped = value.clone();
        scoped["args"]["ability"] = inner;
        return Some((condition, scoped));
    }
    if value["kind"] != "Conditional" || !value["args"]["otherwise"].as_array()?.is_empty() {
        return None;
    }
    let [ability] = value["args"]["then"].as_array()?.as_slice() else {
        return None;
    };
    Some((value["args"]["condition"].clone(), ability.clone()))
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    if super::branch_bounds(value).is_err() {
        return Vec::new();
    }
    let [condition, ability] = args else {
        return Vec::new();
    };
    let Some((test, inner)) = split(value) else {
        return Vec::new();
    };
    if wrap(test.clone(), inner.clone()) != *value {
        return Vec::new();
    }
    condition
        .unify_made(&test, &bindings)
        .into_iter()
        .flat_map(|binding| ability.unify_made(&inner, &binding))
        .collect()
}

fn named_move(instruction: &Value) -> Option<&Value> {
    let operation = &instruction["args"]["operation"];
    let objects = &operation["args"]["objects"];
    let named = &objects["args"][0];
    (instruction["kind"] == "Effect"
        && operation["kind"] == "Move"
        && objects["$sym"] == "exile_followup"
        && named["kind"] == "One"
        && named["args"]["$sym"] == "current_named_object")
        .then_some(&operation["args"]["destination"])
}

// A trailing condition chooses the destination of the card just searched for.
// Its otherwise instruction also needs that selected card.
pub(crate) fn postposed_choice(instruction: &Value) -> Result<Value, String> {
    let args = &instruction["args"];
    if let (Some([search, hand]), Some([top])) = (
        args["then"]["instructions"].as_array().map(Vec::as_slice),
        args["otherwise"]["instructions"]
            .as_array()
            .map(Vec::as_slice),
    ) && args["condition"]["kind"] == "Controls"
        && search["kind"] == "Optional"
        && let Some([find, reveal, shuffle]) = search["args"]["body"]["instructions"]
            .as_array()
            .map(Vec::as_slice)
        && find["args"]["operation"]["kind"] == "Search"
        && reveal["args"]["operation"]["kind"] == "Reveal"
        && shuffle["args"]["operation"]["kind"] == "Shuffle"
        && named_move(hand).is_some_and(|d| d.get("OwnersHand").is_some())
        && named_move(top).is_some_and(|d| d["Library"]["position"] == "Top")
    {
        return Err("the postposed condition chooses where the found card goes".into());
    }
    Ok(instruction.clone())
}
