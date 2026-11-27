//! Implicit action pronouns after an optional, physically single card entry.
//! This changes references only. Optional/payment scope is deliberately preserved.
use super::{Step, symbol_name};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
struct Receipt {
    result: u64,
    creature: bool,
}
fn at<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get(key)?,
            Step::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}
fn controller() -> Value {
    json!({"kind":"AbilityController"})
}
fn is_list(path: &[Step]) -> bool {
    path.len() == 3
        && path
            .iter()
            .zip(["args", "body", "instructions"])
            .all(|(s, k)| matches!(s, Step::Key(key) if key == k))
}
fn optional(value: &Value) -> bool {
    value["kind"] == "Optional" && value["args"]["decider"] == controller()
}
fn own_list(root: &Value, path: &[Step]) -> bool {
    is_list(path)
        || (path.len() == 7
            && is_list(&path[..3])
            && matches!(path[3], Step::Index(_))
            && is_list(&path[4..])
            && at(root, &path[..4]).is_some_and(optional))
}
fn bounded(value: &Value, depth: usize, work: &mut usize, bytes: &mut usize) -> bool {
    if depth > 32 || *work == 0 {
        return false;
    }
    *work -= 1;
    match value {
        Value::String(s) => match bytes.checked_sub(s.len()) {
            Some(left) => {
                *bytes = left;
                true
            }
            None => false,
        },
        Value::Object(fields) => fields.iter().all(|(k, v)| {
            if let Some(left) = bytes.checked_sub(k.len()) {
                *bytes = left;
            } else {
                return false;
            }
            bounded(v, depth + 1, work, bytes)
        }),
        Value::Array(items) => items.iter().all(|v| bounded(v, depth + 1, work, bytes)),
        _ => true,
    }
}
fn inspected(value: &Value) -> bool {
    bounded(value, 0, &mut 4096, &mut 65536)
}
fn positive(filter: &Value, kind: &str) -> bool {
    filter == &json!({"kind":"CardType","args":kind})
        || (filter["kind"] == "AllOf"
            && filter["args"].as_array().is_some_and(|parts| {
                parts.len() <= 8
                    && parts
                        .iter()
                        .any(|p| p == &json!({"kind":"CardType","args":kind}))
            }))
}
fn action(mut value: &Value) -> &Value {
    if matches!(
        symbol_name(value),
        Some("sacrificed_result" | "choice_move_result")
    ) {
        value = &value["args"][0];
    }
    value
}
fn operation(instruction: &Value) -> Option<&Value> {
    (instruction["kind"] == "Effect").then(|| action(&instruction["args"]["operation"]))
}
fn entry(move_op: &Value) -> bool {
    let a = &move_op["args"];
    a["destination"]["Battlefield"]["controller"] == json!({"Player":controller()})
        && a["qualifiers"] == json!([])
        && (a["destination"]["Battlefield"]["arrival"]
            == json!({"controller":null,"tapped":false,"counters":[],"states":[]})
            || a["destination"]["Battlefield"]["arrival"]
                == json!({"controller":null,"tapped":true,"counters":[],"states":[{"Attacking":{"player":null,"or_planeswalker":false}}]}))
}
fn producer(items: &[Value]) -> Option<Receipt> {
    if !(1..=2).contains(&items.len()) || !items.iter().all(inspected) {
        return None;
    }
    let moved = items.last()?;
    let m = operation(moved)?;
    if m["kind"] != "Move" || !entry(m) {
        return None;
    }
    let result = moved["args"]["result"].as_u64()?;
    if result > u64::from(u32::MAX) {
        return None;
    }
    if items.len() == 1 {
        let spec = super::move_after::input_spec(m)?;
        let own = json!({"kind":"InZone","args":{"Players":{"zone":"Graveyard","players":{"kind":"One","args":controller()}}}});
        let has_own =
            spec["filter"]["kind"] == "AllOf" && spec["filter"]["args"].as_array()?.contains(&own);
        return (has_own && super::move_after::eligible(m).ok()?).then_some(Receipt {
            result,
            creature: true,
        });
    }
    let chosen = &items[0];
    let choose = operation(chosen)?;
    let c = &choose["args"];
    let objects = &c["objects"];
    let filter = &objects["args"]["filter"];
    let creature = positive(filter, "Creature");
    let single = ["Exactly", "AsManyAsCan"]
        .iter()
        .any(|kind| c["count"][*kind] == json!({"kind":"Constant","args":1}));
    (choose["kind"] == "ChooseObjects"
        && c["player"] == controller()
        && single
        && c["constraints"].as_array()?.is_empty()
        && objects["kind"] == "Zone"
        && objects["args"]["kind"] == "Card"
        && objects["args"]["scope"]
            == json!({"Players":{"zone":"Hand","players":{"kind":"One","args":controller()}}})
        && (creature || positive(filter, "Planeswalker"))
        && m["args"]["objects"] == json!({"kind":"Selected","args":chosen["args"]["result"]}))
    .then_some(Receipt { result, creature })
}
fn grant(op: &Value) -> bool {
    op["kind"] == "CreateContinuousEffect"
        && op["args"]["evaluation"] == "Snapshot"
        && matches!(
            op["args"]["duration"].as_str(),
            Some("EndOfTurn" | "EndOfCombat" | "Indefinite")
        )
        && op["args"]["changes"].as_array().is_some_and(|changes| {
            !changes.is_empty()
                && changes.len() <= 8
                && changes.iter().all(|c| {
                    c["kind"] == "GrantKeyword"
                        && c["args"]
                            .as_object()
                            .is_some_and(|a| a.len() == 1 && a.contains_key("kind"))
                })
        })
}
fn receipt_object(result: u64) -> Value {
    json!({"kind":"One","args":{"kind":"Result","args":{"result":result,"phase":"After","index":0}}})
}
fn transparent(mut value: &Value) -> &Value {
    for _ in 0..12 {
        if matches!(
            symbol_name(value),
            Some(
                "zone_members"
                    | "remember_operand"
                    | "member_count_it"
                    | "distributive_pump_subject"
            )
        ) {
            value = &value["args"][0];
        } else {
            break;
        }
    }
    value
}
fn skip_grant(instruction: &Value, receipt: Receipt) -> bool {
    operation(instruction).is_some_and(|op| {
        let objects = transparent(&op["args"]["affected"]["Objects"]);
        grant(op)
            && objects["kind"] == "One"
            && transparent(&objects["args"]) == &receipt_object(receipt.result)["args"]
    })
}
fn scalar_path(mut value: &Value, mut path: &[Step]) -> bool {
    let mut one = false;
    for _ in 0..12 {
        if path.is_empty() {
            return one;
        }
        if matches!(
            symbol_name(value),
            Some(
                "zone_members"
                    | "remember_operand"
                    | "member_count_it"
                    | "distributive_pump_subject"
            )
        ) && matches!(path, [Step::Key(k), Step::Index(0), ..] if k == "args")
        {
            value = &value["args"][0];
            path = &path[2..];
        } else if !one
            && symbol_name(value) == Some("qualified_members")
            && value["args"].as_array().is_some_and(|args| args.len() == 1)
            && matches!(path, [Step::Key(k), Step::Index(0), ..] if k == "args")
        {
            one = true;
            value = &value["args"][0];
            path = &path[2..];
        } else if !one
            && value["kind"] == "One"
            && matches!(path.first(), Some(Step::Key(k)) if k == "args")
        {
            one = true;
            value = &value["args"];
            path = &path[1..];
        } else {
            return false;
        }
    }
    false
}
fn role_site(root: &Value, path: &[Step]) -> Option<usize> {
    let (depth, op) = (0..path.len()).rev().find_map(|n| {
        let v = at(root, &path[..n])?;
        matches!(
            v["kind"].as_str(),
            Some("CreateContinuousEffect" | "Move" | "Sacrifice")
        )
        .then_some((n, v))
    })?;
    let fields: &[&str] = if grant(op) {
        &["args", "affected", "Objects"]
    } else {
        &["args", "objects"]
    };
    let tail = path.get(depth..)?;
    if !tail
        .iter()
        .zip(fields)
        .all(|(s, k)| matches!(s, Step::Key(key) if key.as_str() == *k))
        || tail.len() < fields.len()
    {
        return None;
    }
    let mut objects = op;
    for k in fields {
        objects = &objects[*k];
    }
    if !scalar_path(objects, &tail[fields.len()..]) {
        return None;
    }
    if grant(op) {
        return inspected(op).then_some(depth);
    }
    let delayed_depth = (0..depth)
        .rev()
        .find(|n| at(root, &path[..*n]).is_some_and(|v| v["kind"] == "CreateDelayedTrigger"))?;
    let delayed_op = at(root, &path[..delayed_depth])?;
    (delayed(delayed_op)? == op).then_some(delayed_depth)
}
fn delayed(operation: &Value) -> Option<&Value> {
    if operation["kind"] != "CreateDelayedTrigger" || !inspected(operation) {
        return None;
    }
    let d = &operation["args"];
    let body = d["body"]["instructions"].as_array()?;
    if body.len() != 1 {
        return None;
    }
    let op = self::operation(&body[0])?;
    let allowed = (op["kind"] == "Move"
        && (op["args"]["destination"] == json!({"OwnersHand":{"counters":[],"states":[]}})
            || op["args"]["destination"]
                == json!({"Exile":{"visibility":"FaceUp","arrival":{"counters":[],"states":[]}}}))
        && op["args"]["qualifiers"] == json!([]))
        || (op["kind"] == "Sacrifice"
            && op["args"]["players"] == json!({"kind":"One","args":controller()})
            && op["args"]["choose"].is_null());
    (allowed
        && d["event"] == json!({"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}})
        && d["start"] == "Immediately"
        && d["expires"].is_null()
        && d["intervening_if"].is_null())
    .then_some(op)
}
fn terminal_followups(items: &[Value]) -> bool {
    match items {
        [only] => operation(only).is_some_and(|op| grant(op) || delayed(op).is_some()),
        [first, second] => {
            operation(first).is_some_and(grant)
                && operation(second).is_some_and(|op| delayed(op).is_some())
        }
        _ => false,
    }
}

fn before(path: &[Step], index: usize, items: &[Value]) -> Option<Receipt> {
    for skipped in 0..=1 {
        let end = index.checked_sub(skipped)?;
        // Check the complete producer-owned tail, including an earlier grant.
        // A valid current suffix cannot authorize part of a longer unsupported chain.
        if !terminal_followups(&items[end..]) {
            continue;
        }
        for width in 1..=2 {
            let start = match end.checked_sub(width) {
                Some(n) => n,
                None => continue,
            };
            let receipt = if !is_list(path) {
                producer(&items[start..end])
            } else if width == 1 && optional(&items[start]) {
                let body = items[start]["args"]["body"]["instructions"].as_array()?;
                producer(body)
            } else {
                None
            };
            if let Some(receipt) = receipt
                && (skipped == 0 || skip_grant(&items[index - 1], receipt))
            {
                return Some(receipt);
            }
        }
    }
    None
}
/// No literal Source/name rewrite and no scope relocation: the existing lexical
/// marker alone can acquire this narrowly proved actual After incarnation.
pub(super) fn reference(
    root: &Value,
    path: &[Step],
    name: &str,
    created_result: Option<u64>,
) -> Result<Option<Value>, String> {
    if !matches!(name, "it" | "qualified_object")
        || path.len() > 128
        || !matches!(
            root["kind"].as_str(),
            Some("Spell" | "Activated" | "Triggered")
        )
    {
        return Ok(None);
    }
    let noun = name == "qualified_object";
    if noun
        && at(root, path).is_none_or(|v| {
            v["args"][0] != "Permanent"
                || v["args"][1] != json!({"kind":"CardType","args":"Creature"})
        })
    {
        return Ok(None);
    }
    let Some(depth) = role_site(root, path) else {
        return Ok(None);
    };
    let Some((n, index)) = path[..depth].iter().enumerate().rev().find_map(|(n, s)| {
        if let Step::Index(i) = s
            && n > 0
            && matches!(&path[n - 1], Step::Key(k) if k == "instructions")
        {
            Some((n, *i))
        } else {
            None
        }
    }) else {
        return Ok(None);
    };
    let list_path = &path[..n];
    if !own_list(root, list_path) {
        return Ok(None);
    }
    let Some(items) = at(root, list_path).and_then(Value::as_array) else {
        return Ok(None);
    };
    // A closed keyword/delayed tail cannot leak a newly remembered pronoun into
    // a later property, payment, new target, or unrelated action in this scope.
    if items.len() > 16 || index >= items.len() || !terminal_followups(&items[index..]) {
        return Ok(None);
    }
    Ok(before(list_path, index, items)
        .filter(|r| !noun || r.creature)
        // A nested lexical cursor may restart while creation metadata is inherited.
        // Never let qualified_members promote this Move receipt into that batch.
        .filter(|r| Some(r.result) != created_result)
        .map(|r| receipt_object(r.result)["args"].take()))
}

fn paid_then(branch: &Value) -> Option<&[Value]> {
    let args = &branch["args"];
    if branch["kind"] != "If"
        || args["condition"]["kind"] != "PaymentChosen"
        || args["otherwise"]["instructions"] != json!([])
    {
        return None;
    }
    args["then"]["instructions"].as_array().map(Vec::as_slice)
}

fn attaches_to_recent_entrant(objects: &Value) -> bool {
    let objects = transparent(objects);
    let objects = if symbol_name(objects) == Some("return_grant_set") {
        &objects["args"][0]
    } else {
        objects
    };
    let mut reference = if objects["kind"] == "One" {
        &objects["args"]
    } else {
        objects
    };
    if symbol_name(reference) == Some("qualified_members") {
        reference = &reference["args"][0];
    }
    let reference = transparent(reference);
    matches!(
        symbol_name(reference),
        Some("it" | "created_reference_single" | "counter_reference_single")
    ) || symbol_name(reference) == Some("qualified_object")
        && reference["args"] == json!(["Permanent",{"kind":"CardType","args":"Creature"}])
}

// A terminal keyword grant or delayed sacrifice of the entrant still uses
// the card offered in the immediately preceding optional entry or payment.
pub(super) fn detached_followup(ability: &Value) -> bool {
    let Some([payment, checked, continuation]) = ability["args"]["body"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return false;
    };
    let (Some(paid), Some(remaining)) = (operation(payment), operation(continuation)) else {
        return false;
    };
    let Some([inside]) = paid_then(checked) else {
        return false;
    };
    if paid["kind"] != "PayCost" || paid["args"]["optional"] != true {
        return false;
    }
    if optional(inside) && grant(remaining) {
        let Some([choice, placed]) = inside["args"]["body"]["instructions"]
            .as_array()
            .map(Vec::as_slice)
        else {
            return false;
        };
        let (Some(choice), Some(placed)) = (operation(choice), operation(placed)) else {
            return false;
        };
        return choice["kind"] == "ChooseObjects"
            && choice["args"]["objects"]["args"]["scope"]["Players"]["zone"] == "Hand"
            && placed["kind"] == "Move"
            && entry(placed)
            && placed["args"]["destination"]["Battlefield"]["arrival"]["tapped"] == true
            && attaches_to_recent_entrant(&remaining["args"]["affected"]["Objects"]);
    }
    let Some(inside) = operation(inside) else {
        return false;
    };
    let Some([part]) = paid["args"]["cost"]["parts"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let move_into = &part["Move"];
    let Some([selection]) = move_into["selections"].as_array().map(Vec::as_slice) else {
        return false;
    };
    if !grant(inside)
        || !attaches_to_recent_entrant(&inside["args"]["affected"]["Objects"])
        || selection["objects"]["args"]["scope"]["Players"]["zone"] != "Hand"
        || !positive(&selection["objects"]["args"]["filter"], "Creature")
        || selection["choose"] != json!({"Exactly":{"kind":"Constant","args":1}})
        || move_into["destination"].get("Battlefield").is_none()
        || remaining["kind"] != "CreateDelayedTrigger"
        || remaining["args"]["event"]
            != json!({"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}})
    {
        return false;
    }
    let Some([sacrifice, check]) = remaining["args"]["body"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return false;
    };
    let Some(sacrifice) = operation(sacrifice) else {
        return false;
    };
    let Some([returned]) = paid_then(check) else {
        return false;
    };
    let expected_return = operation(returned).is_some_and(|returned| {
        returned["kind"] == "Move" && returned["args"]["destination"].get("Battlefield").is_some()
    });
    sacrifice["kind"] == "PayCost"
        && sacrifice["args"]["cost"]["parts"].as_array().is_some_and(|parts| {
            matches!(parts.as_slice(), [part] if attaches_to_recent_entrant(&part["Sacrifice"]["objects"]))
        })
        && expected_return
}

// A keyword grant using the same bare pronoun as a future attack
// trigger's sole counter placement must execute in that event frame.
pub(super) fn detached_temporary_event(program: &Value) -> bool {
    let Some([creating, granting]) = program["instructions"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let mut create = &creating["args"]["operation"];
    if symbol_name(create) == Some("scope") {
        create = &create["args"][0];
    }
    let Some(granted) = operation(granting) else {
        return false;
    };
    let args = &create["args"];
    if creating["kind"] != "Effect"
        || create["kind"] != "CreateTrigger"
        || args["event"]["Object"]["event"] != "AttacksAlone"
        || args["duration"] != "EndOfTurn"
        || !args["intervening_if"].is_null()
        || !grant(granted)
        || !attaches_to_recent_entrant(&granted["args"]["affected"]["Objects"])
    {
        return false;
    }
    let Some([place]) = args["body"]["instructions"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let Some(place) = operation(place) else {
        return false;
    };
    place["kind"] == "PutCounters"
        && place["args"]["placements"].as_array().is_some_and(|parts| {
            matches!(parts.as_slice(), [part]
                if part["recipients"]["kind"] == "Objects"
                && attaches_to_recent_entrant(&part["recipients"]["args"]))
        })
}

#[cfg(test)]
#[path = "optional_entered_card_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "optional_entered_card_guards.rs"]
mod guards;
