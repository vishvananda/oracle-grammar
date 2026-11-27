//! A sole-use pure capture and its next atomic parameter read share one instant.
use super::{
    scalar_reads,
    walk::{Uses, Walker, lossless},
};
use serde_json::{Value, json};

pub(super) fn slots(operation: &Value) -> Option<&'static [&'static str]> {
    let args = &operation["args"];
    match operation["kind"].as_str()? {
        "DealDamage" => Some(&["/args/amount"]),
        "Draw" | "GainLife" | "LoseLife" | "Mill" | "Scry" | "Surveil"
            if args["players"]["kind"] == "One"
                || matches!(operation["kind"].as_str(), Some("LoseLife" | "Mill"))
                    && args["players"]
                        == json!({"kind":"OpponentsOf","args":{"kind":"AbilityController"}}) =>
        {
            Some(&["/args/amount"])
        }
        // These stats execute together in one Snapshot change. A live
        // continuous modifier does not share this capture instant.
        "CreateContinuousEffect"
            if args["evaluation"] == "Snapshot"
                && args["affected"].get("Objects").is_some()
                && args["changes"].as_array().is_some_and(
                    |changes| matches!(changes.as_slice(), [change] if change["kind"] == "Pump"),
                ) =>
        {
            Some(&[
                "/args/changes/0/args/power",
                "/args/changes/0/args/toughness",
            ])
        }
        "PutCounters"
            if args["placements"].as_array()?.len() == 1
                && args["placements"][0]["recipients"]["kind"] == "Objects" =>
        {
            Some(&["/args/placements/0/amount"])
        }
        "CreateTokens" if single_token(args) => Some(&[
            "/args/tokens/0/recipe/Defined/power_toughness/0",
            "/args/tokens/0/recipe/Defined/power_toughness/1",
        ]),
        _ => None,
    }
}

fn single_token(args: &Value) -> bool {
    let Some(tokens) = args["tokens"].as_array() else {
        return false;
    };
    let [token] = tokens.as_slice() else {
        return false;
    };
    let definition = &token["recipe"]["Defined"];
    args["players"]["kind"] == "One"
        && token["amount"] == json!({"kind":"Constant","args":1})
        && token["words"] == "CreatingEffect"
        && definition["power_toughness"]
            .as_array()
            .is_some_and(|stats| stats.len() == 2)
        && definition["chosen_color"] == false
        && definition["chosen_creature_type"] == false
        && definition["abilities"] == json!([])
        && definition["abilities_of"].is_null()
        && args["entry"] == json!({"controller":null,"tapped":false,"counters":[],"states":[]})
}

fn candidate(
    first: &Value,
    next: &Value,
    uses: &Uses,
    tracing: bool,
    keep_order: bool,
) -> Option<(Value, &'static [&'static str], u64)> {
    if first["kind"] != "Effect"
        || first["args"]["operation"]["kind"] != "BindValue"
        || next["kind"] != "Effect"
        || lossless::<crate::ir::Instruction>(first, tracing).is_none()
        || lossless::<crate::ir::Instruction>(next, tracing).is_none()
    {
        return None;
    }
    let id = super::trace::number(&first["args"]["result"])?;
    let operation = &next["args"]["operation"];
    if keep_order && operation["kind"] != "CreateContinuousEffect" {
        return None;
    }
    let slots = slots(operation)?;
    let mut reads = 0;
    for slot in slots {
        let value = operation.pointer(slot)?;
        if scalar_reads::signed_use(value, id).is_some() {
            reads += 1;
        } else if !scalar_reads::constant(value) {
            return None;
        }
    }
    if reads == 0
        || uses.definitions.get(&id) != Some(&1)
        || uses.total.get(&id) != Some(&(reads + 1))
    {
        return None;
    }
    let value = first["args"]["operation"]["args"]["value"].clone();
    Some((value, slots, id))
}

pub(super) fn inline_array(
    items: &mut Vec<Value>,
    uses: &Uses,
    walker: &mut Walker<'_>,
    tracing: bool,
    keep_order: bool,
) -> Result<bool, String> {
    let mut changed = false;
    let mut index = 0;
    while index + 1 < items.len() {
        if let Some((mut value, slots, id)) =
            candidate(&items[index], &items[index + 1], uses, tracing, keep_order)
            && scalar_reads::pure(&mut value, walker)?
        {
            let operation = &mut items[index + 1]["args"]["operation"];
            let values: Option<Vec<Value>> = slots
                .iter()
                .map(|slot| {
                    let operand = operation.pointer(slot)?;
                    scalar_reads::signed_use(operand, id).map_or_else(
                        || Some(operand.clone()),
                        |sign| scalar_reads::amount(&value, sign),
                    )
                })
                .collect();
            // Preserve dynamic count validation and printable amount domains;
            // a signed substitution must not introduce a negative literal.
            if let Some(values) = values.filter(|values| {
                matches!(
                    operation["kind"].as_str(),
                    Some("CreateTokens" | "CreateContinuousEffect")
                ) || !values.iter().any(|value| {
                    scalar_reads::constant(value) && value["args"].as_i64().is_some_and(|n| n < 0)
                })
            }) {
                for (slot, value) in slots.iter().zip(values) {
                    *operation.pointer_mut(slot).expect("checked parameter slot") = value;
                }
                items.remove(index);
                changed = true;
            }
        }
        index += 1;
    }
    Ok(changed)
}
