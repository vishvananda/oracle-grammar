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

/// A named resolution payment has already moved its input. The before view
/// fixes the paid object's characteristics for both reads.
fn paid_before_power(value: &Value) -> bool {
    let object = &value["args"]["object"];
    let objects = &object["args"]["objects"];
    let history = &objects["args"];
    value["kind"] == "ObjectProperty"
        && value["args"]["property"] == "Power"
        && value["args"]["information"] == "CurrentOrLastKnown"
        && object["kind"] == "Query"
        && object["args"]["index"] == 0
        && objects["kind"] == "CostObjects"
        && super::trace::number(&history["payment"]["Resolution"]).is_some()
        && history["component"] == json!([0])
        && history["view"] == "Inputs"
        && history["phase"] == "Before"
}

/// A fixed event or payment receipt can provide both amount reads directly.
fn fixed_amount_pair(items: &mut [Value], uses: &Uses, tracing: bool) -> bool {
    let [capture, first, second, ..] = items else {
        return false;
    };
    let definition = &capture["args"]["operation"];
    let value = &definition["args"]["value"];
    if capture["kind"] != "Effect" || definition["kind"] != "BindValue" {
        return false;
    }
    let left = &first["args"]["operation"];
    let right = &second["args"]["operation"];
    let pair = (left["kind"].as_str(), right["kind"].as_str());
    let event_count = value == &json!({"kind":"CountObjects","args":{"kind":"EventGroup"}})
        && pair == (Some("Draw"), Some("Discard"))
        && right["args"]["random"] == false;
    let paid_power = matches!(
        pair,
        (Some("GainLife"), Some("Draw")) | (Some("Draw"), Some("Mill"))
    ) && paid_before_power(value);
    if !event_count && !paid_power {
        return false;
    }
    let Some(id) = super::trace::number(&capture["args"]["result"]) else {
        return false;
    };
    if uses.definitions.get(&id) != Some(&1) || uses.total.get(&id) != Some(&3) {
        return false;
    }
    let actor = json!({"kind":"One","args":{"kind":"AbilityController"}});
    let owns_amount = |item: &Value| {
        let args = &item["args"]["operation"]["args"];
        item["kind"] == "Effect"
            && args["players"] == actor
            && scalar_reads::signed_use(&args["amount"], id) == Some(1)
            && lossless::<crate::ir::Instruction>(item, tracing).is_some()
    };
    if !owns_amount(first)
        || !owns_amount(second)
        || lossless::<crate::ir::Instruction>(capture, tracing).is_none()
    {
        return false;
    }
    let value = value.clone();
    first["args"]["operation"]["args"]["amount"] = value.clone();
    second["args"]["operation"]["args"]["amount"] = value;
    true
}

/// An optional choice from hand reads a single scalar immediately. Neither
/// deciding whether to make this choice nor the candidate filter changes state.
fn optional_entry_bound(capture: &Value, next: &mut Value, uses: &Uses, tracing: bool) -> bool {
    let definition = &capture["args"]["operation"];
    let value = &definition["args"]["value"];
    let controller = json!({"kind":"AbilityController"});
    if capture["kind"] != "Effect"
        || definition["kind"] != "BindValue"
        || value["kind"] != "CountCounters"
        || value["args"]["information"] != "CurrentOrLastKnown"
        || value["args"]["recipients"]
            != json!({"kind":"Objects","args":{"kind":"One","args":{"kind":"Source"}}})
        || next["kind"] != "Optional"
        || next["args"]["decider"] != controller
    {
        return false;
    }
    let Some(id) = super::trace::number(&capture["args"]["result"]) else {
        return false;
    };
    if uses.definitions.get(&id) != Some(&1)
        || uses.total.get(&id) != Some(&2)
        || lossless::<crate::ir::Instruction>(capture, tracing).is_none()
        || lossless::<crate::ir::Instruction>(next, tracing).is_none()
    {
        return false;
    }
    let Some([choice, movement]) = next["args"]["body"]["instructions"]
        .as_array_mut()
        .map(Vec::as_mut_slice)
    else {
        return false;
    };
    if choice["kind"] != "Effect" {
        return false;
    }
    let chosen = super::trace::number(&choice["args"]["result"]);
    let choose = &mut choice["args"]["operation"];
    let mover = &movement["args"]["operation"];
    if choose["kind"] != "ChooseObjects"
        || choose["args"]["player"] != controller
        || choose["args"]["constraints"] != json!([])
        || choose["args"]["count"] != json!({"AsManyAsCan":{"kind":"Constant","args":1}})
        || movement["kind"] != "Effect"
        || mover["kind"] != "Move"
        || mover["args"]["objects"]["kind"] != "Selected"
        || super::trace::number(&mover["args"]["objects"]["args"]) != chosen
        || mover["args"]["destination"]["Battlefield"]["controller"]["Player"] != controller
    {
        return false;
    }
    let objects = &mut choose["args"]["objects"];
    if objects["kind"] != "Zone"
        || objects["args"]["kind"] != "Card"
        || objects["args"]["scope"]
            != json!({"Players":{"zone":"Hand","players":{"kind":"One","args":controller}}})
        || objects["args"]["filter"]["kind"] != "AllOf"
    {
        return false;
    }
    let Some(parts) = objects["args"]["filter"]["args"].as_array_mut() else {
        return false;
    };
    let Some(property) = parts.iter_mut().find(|part| {
        let test = &part["args"]["test"];
        part["kind"] == "Property"
            && part["args"]["property"] == "ManaValue"
            && test["kind"] == "Compare"
            && test["args"]["comparison"] == "LessOrEqual"
            && scalar_reads::signed_use(&test["args"]["value"], id) == Some(1)
    }) else {
        return false;
    };
    property["args"]["test"]["args"]["value"] = value.clone();
    true
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
        if !keep_order && fixed_amount_pair(&mut items[index..], uses, tracing) {
            items.remove(index);
            changed = true;
            continue;
        }
        if !keep_order {
            let (before, after) = items.split_at_mut(index + 1);
            if optional_entry_bound(&before[index], &mut after[0], uses, tracing) {
                items.remove(index);
                changed = true;
                continue;
            }
        }

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
