//! A repeated requested scalar has a definition without a lexical game action.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

fn paired_amounts(body: &Value, amount: &Value) -> bool {
    let Some(instructions) = body.as_array() else {
        return false;
    };
    instructions.len() == 2
        && instructions
            .iter()
            .zip(["Draw", "Discard"])
            .all(|(i, kind)| {
                i["kind"] == "Effect"
                    && i["args"]["operation"]["kind"] == kind
                    && i["args"]["operation"]["args"]["amount"] == *amount
            })
}

pub(crate) fn lower(body: &Value, amount: &Value, at: usize) -> Result<Value, String> {
    let x = json!({"kind":"AnnouncedX"});
    if !paired_amounts(body, &x) {
        return Err("requested capture requires paired immediate amounts".into());
    }
    let reference = json!({"$sym":"capture_ref","args":[at],"at":at});
    let mut instructions = super::substituted(body.clone(), &reference);
    instructions.as_array_mut().unwrap().insert(
        0,
        json!({"kind":"Effect","args":{
            "result":{"$sym":"capture_next","args":[at],"at":at},
            "operation":{"kind":"BindValue","args":{"value":amount}}
        }}),
    );
    Ok(instructions)
}

/// Remove only this definition and its two exact numeric uses. Other lexical
/// result reads, quantities and created/granted abilities stay untouched.
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [body, amount] = args else {
        return Vec::new();
    };
    let Some(instructions) = value.as_array() else {
        return Vec::new();
    };
    let [definition, rest @ ..] = instructions.as_slice() else {
        return Vec::new();
    };
    if definition["kind"] != "Effect" || definition["args"]["operation"]["kind"] != "BindValue" {
        return Vec::new();
    }
    let Some(id) = definition["args"]["result"].as_u64() else {
        return Vec::new();
    };
    let mut abstracted = json!(rest);
    if !paired_amounts(&abstracted, &json!({"kind":"BoundValue","args":id})) {
        return Vec::new();
    }
    for instruction in abstracted.as_array_mut().unwrap() {
        instruction["args"]["operation"]["args"]["amount"] = json!({"kind":"AnnouncedX"});
    }
    amount
        .unify_made(&definition["args"]["operation"]["args"]["value"], &bindings)
        .into_iter()
        .flat_map(|b| body.unify_made(&abstracted, &b))
        .collect()
}
