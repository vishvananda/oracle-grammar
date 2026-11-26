//! A sole-use scalar capture immediately before a pure Snapshot pump is inline.
use super::scalar_reads::{amount, constant, signed_use};
use super::walk::{Uses, Walker, lossless};
use serde_json::Value;

fn candidate(first: &Value, next: &Value, uses: &Uses, tracing: bool) -> Option<(u64, Value)> {
    if first["kind"] != "Effect"
        || first["args"]["operation"]["kind"] != "BindValue"
        || next["kind"] != "Effect"
        || next["args"]["operation"]["kind"] != "CreateContinuousEffect"
        || lossless::<crate::ir::Instruction>(first, tracing).is_none()
        || lossless::<crate::ir::Instruction>(next, tracing).is_none()
    {
        return None;
    }
    let id = super::trace::number(&first["args"]["result"])?;
    let args = &next["args"]["operation"]["args"];
    if args["evaluation"] != "Snapshot" || args["affected"].get("Objects").is_none() {
        return None;
    }
    let [change] = args["changes"].as_array()?.as_slice() else {
        return None;
    };
    if change["kind"] != "Pump" {
        return None;
    }
    let mut reads = 0;
    for field in ["power", "toughness"] {
        let operand = &change["args"][field];
        if signed_use(operand, id).is_some() {
            reads += 1;
        } else if !constant(operand) {
            return None;
        }
    }
    if reads == 0
        || uses.definitions.get(&id) != Some(&1)
        || uses.total.get(&id) != Some(&(reads + 1))
    {
        return None;
    }
    Some((id, first["args"]["operation"]["args"]["value"].clone()))
}
pub(super) fn inline_array(
    items: &mut Vec<Value>,
    uses: &Uses,
    walker: &mut Walker<'_>,
    tracing: bool,
) -> Result<bool, String> {
    let mut changed = false;
    let mut index = 0;
    while index + 1 < items.len() {
        if let Some((id, mut definition)) =
            candidate(&items[index], &items[index + 1], uses, tracing)
            && super::scalar_reads::pure(&mut definition, walker)?
        {
            let stats = &items[index + 1]["args"]["operation"]["args"]["changes"][0]["args"];
            let replacements: Option<Vec<_>> = ["power", "toughness"]
                .into_iter()
                .map(|field| {
                    signed_use(&stats[field], id).map_or_else(
                        || Some(stats[field].clone()),
                        |sign| amount(&definition, sign),
                    )
                })
                .collect();
            if let Some(values) = replacements {
                for (field, value) in ["power", "toughness"].into_iter().zip(values) {
                    items[index + 1]["args"]["operation"]["args"]["changes"][0]["args"][field] =
                        value;
                }
                items.remove(index);
                changed = true;
            }
        }
        index += 1;
    }
    Ok(changed)
}
