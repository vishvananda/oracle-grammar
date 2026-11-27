use super::{Introductions, Step, get, symbol_name};
use serde_json::{Value, json};

/// A characteristic test after an immediate return to a hidden hand uses
/// the previously public object's last known state (CR 608.2h). Only the
/// exact object the preceding instruction moved supplies that state.
pub(super) fn reference(
    root: &Value,
    path: &[Step],
    name: &str,
    introductions: &Introductions<'_>,
    reference: &Value,
) -> Result<Option<Value>, String> {
    if reference["kind"] != "Target"
        || !matches!(name, "it" | "current_it" | "current_named_object")
        || path.len() > 128
    {
        return Ok(None);
    }
    let Some((depth, index)) = path.iter().enumerate().rev().find_map(|(depth, step)| {
        let Step::Index(index) = step else {
            return None;
        };
        (depth != 0 && matches!(&path[depth-1], Step::Key(key) if key == "instructions"))
            .then_some((depth, *index))
    }) else {
        return Ok(None);
    };
    let direct = &path[depth + 1..];
    let [
        Step::Key(args),
        Step::Key(condition),
        Step::Key(condition_args),
        Step::Key(object),
        suffix @ ..,
    ] = direct
    else {
        return Ok(None);
    };
    if args != "args"
        || condition != "condition"
        || condition_args != "args"
        || object != "object"
        || !matches!(suffix, [] | [Step::Key(_), Step::Index(0)])
    {
        return Ok(None);
    }
    let Some(items) = get(root, &path[..depth]).and_then(Value::as_array) else {
        return Ok(None);
    };
    let Some(branch) = items.get(index) else {
        return Ok(None);
    };
    let test = &branch["args"]["condition"];
    if branch["kind"] != "If"
        || test["kind"] != "ObjectMatches"
        || !(matches!(
            test["args"]["filter"]["kind"].as_str(),
            Some("CardType" | "Color" | "Token")
        ) || test["args"]["filter"]["kind"] == "Property"
            && test["args"]["filter"]["args"]["property"] == "ColorCount")
        || !suffix.is_empty() && symbol_name(&test["args"]["object"]) != Some("member_count_it")
    {
        return Ok(None);
    }
    let Some(effect) = index.checked_sub(1).and_then(|at| items.get(at)) else {
        return Ok(None);
    };
    let mut movement = &effect["args"]["operation"];
    if symbol_name(movement) == Some("owner_return") {
        movement = &movement["args"][0];
    }
    let Some(result) = effect["args"]["result"].as_u64() else {
        return Ok(None);
    };
    if effect["kind"] != "Effect"
        || movement["kind"] != "Move"
        || !movement["args"]["destination"]
            .get("OwnersHand")
            .is_some_and(Value::is_object)
        || movement["args"]["qualifiers"] != json!([])
    {
        return Ok(None);
    }
    let mut selected = &movement["args"]["objects"];
    while matches!(
        symbol_name(selected),
        Some("zone_members" | "exile_followup")
    ) {
        selected = &selected["args"][0];
    }
    let matched = match selected["kind"].as_str() {
        Some("One") => {
            super::super::references::bound_reference(
                &selected["args"],
                introductions.keys,
                introductions.base,
            )? == *reference
        }
        Some("Targets") => {
            super::super::optional_exile_target::reference(root, selected, introductions)?.as_ref()
                == Some(reference)
        }
        _ => false,
    };
    Ok(matched
        .then(|| json!({"kind":"Result","args":{"result":result,"phase":"Before","index":0}})))
}

// A perpetual change can follow an actual returned card into the hand. A
// stale graveyard Target cannot act on the new object. The lexical condition
// and the actual movement receipt remain distinct operands.
fn stale_perpetual(first: &Value, next: &Value) -> bool {
    let op = &first["args"]["operation"];
    let check = &next["args"]["condition"]["args"]["object"];
    let target = &op["args"]["objects"]["args"];
    if first["kind"] != "Effect"
        || op["kind"] != "Move"
        || op["args"]["destination"].get("OwnersHand").is_none()
        || op["args"]["objects"]["kind"] != "One"
        || target["kind"] != "Target"
        || next["kind"] != "If"
        || next["args"]["condition"]["kind"] != "ObjectMatches"
        || check["kind"] != "Result"
        || check["args"]["result"] != first["args"]["result"]
        || check["args"]["phase"] != "Before"
        || check["args"]["index"] != 0
    {
        return false;
    }
    next["args"]["then"]["instructions"]
        .as_array()
        .is_some_and(|steps| {
            steps.first().is_some_and(|step| {
                let action = &step["args"]["operation"];
                step["kind"] == "Effect"
                    && action["kind"] == "CreatePerpetualEffect"
                    && action["args"]["objects"]["kind"] == "One"
                    && action["args"]["objects"]["args"] == *target
            })
        })
}

pub(in crate::bind) fn validate(value: &Value) -> Result<(), String> {
    let mut pending = vec![(value, 0)];
    let mut work = 131_072;
    while let Some((node, depth)) = pending.pop() {
        if depth > 128 || work == 0 {
            return Err("returned card follow-up proof exceeds bounds".into());
        }
        work -= 1;
        if node["instructions"].as_array().is_some_and(|items| {
            items
                .windows(2)
                .any(|pair| stale_perpetual(&pair[0], &pair[1]))
        }) {
            return Err("the perpetual change needs the card actually moved to hand".into());
        }
        match node {
            Value::Object(fields) => pending.extend(fields.values().map(|v| (v, depth + 1))),
            Value::Array(items) => pending.extend(items.iter().map(|v| (v, depth + 1))),
            _ => {}
        }
    }
    Ok(())
}
