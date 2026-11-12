//! "You may put a creature card from your hand onto the battlefield. That creature gains
//! haste." The sentences after an optional entry that act on the entered card happen
//! only if the card entered, so the entry is an optional payment and those sentences
//! are guarded by its having been paid. They read the entered card from the payment.
//! A sentence that only tests the card ("If you put a Cave onto the battlefield this
//! way, you gain 4 life"), or any later sentence independent of the card, leaves the
//! entry an ordinary optional action. Binding lowers the optional action to the payment;
//! the surface restores it for the printer.
//!
//! Any other optional entry, a return, a put or a token creation, becomes a payment by
//! performing the action when every later instruction of the optional action acts on
//! the entered objects: "you may return target creature card from your graveyard to the
//! battlefield. That creature gains haste." Those instructions keep naming the result.
use crate::traverse::{Visit, rewrite};
use serde_json::{Value, json};

/// An optional choice of one card from a hand, moved onto the battlefield.
struct Entry {
    player: Value,
    cards: Value,
    destination: Value,
    chosen: u64,
    moved: u64,
}

fn constant_one() -> Value {
    json!({"kind": "Constant", "args": 1})
}

fn operation<'a>(instruction: &'a Value, kind: &str) -> Option<&'a Value> {
    let operation = &instruction["args"]["operation"];
    (instruction["kind"] == "Effect" && operation["kind"] == kind).then(|| &operation["args"])
}

fn hand_entry(decider: &Value, body: &[Value]) -> Option<Entry> {
    let [choice, moved, ..] = body else {
        return None;
    };
    let choose = operation(choice, "ChooseObjects")?;
    let cards = &choose["objects"];
    let from_hand = cards["kind"] == "Zone" && cards["args"]["scope"]["Players"]["zone"] == "Hand";
    let unconstrained = choose["constraints"]
        .as_array()
        .is_none_or(|constraints| constraints.is_empty());
    if choose["player"] != *decider
        || !from_hand
        || !unconstrained
        || choose["count"] != json!({"AsManyAsCan": constant_one()})
    {
        return None;
    }
    let chosen = choice["args"]["result"].as_u64()?;
    let movement = operation(moved, "Move")?;
    let plain = movement["qualifiers"]
        .as_array()
        .is_none_or(|qualifiers| qualifiers.is_empty());
    if movement["objects"] != json!({"kind": "Selected", "args": chosen})
        || movement["destination"].get("Battlefield").is_none()
        || !plain
    {
        return None;
    }
    Some(Entry {
        player: decider.clone(),
        cards: cards.clone(),
        destination: movement["destination"].clone(),
        chosen,
        moved: moved["args"]["result"].as_u64()?,
    })
}

fn paid(chosen: u64) -> Value {
    json!({"kind": "CostObjects", "args": {"payment": {"Resolution": chosen}, "component": [0],
        "view": "Performed", "phase": "After"}})
}

fn entered(moved: u64) -> Value {
    json!({"kind": "Transition", "args": {"result": moved, "phase": "After"}})
}

fn entered_one(moved: u64, index: &Value) -> Value {
    json!({"kind": "Result", "args": {"result": moved, "phase": "After", "index": index}})
}

/// How a sentence uses the entered card: whether it names the card at all, and whether
/// it acts on the card outside any condition. `None` when it reads the entry in a way
/// the payment does not name.
fn uses(instruction: &Value, entry: &Entry) -> Option<(bool, bool)> {
    fn walk(value: &Value, entry: &Entry, in_condition: bool, seen: &mut (bool, bool)) -> bool {
        match value {
            Value::Object(fields) => {
                let kind = fields.get("kind").and_then(Value::as_str);
                let args = &value["args"];
                let result = args["result"].as_u64();
                if matches!(kind, Some("Result" | "Transition")) && result == Some(entry.moved) {
                    seen.0 = true;
                    seen.1 |= !in_condition;
                    return args["phase"] == "After";
                }
                let names_choice = matches!(kind, Some("Selected" | "PaymentChosen"))
                    && args.as_u64() == Some(entry.chosen);
                let recorded = kind == Some("Recorded") && result == Some(entry.moved);
                if names_choice || recorded {
                    return false;
                }
                fields.iter().all(|(key, child)| {
                    walk(child, entry, in_condition || key == "condition", seen)
                })
            }
            Value::Array(items) => items
                .iter()
                .all(|item| walk(item, entry, in_condition, seen)),
            _ => true,
        }
    }
    let mut seen = (false, false);
    walk(instruction, entry, false, &mut seen).then_some(seen)
}

/// An optional payment of one cost part, and the instructions run if it was paid.
fn payment(player: &Value, result: u64, part: Value, dependents: Vec<Value>) -> [Value; 2] {
    [
        json!({"kind": "Effect", "args": {"result": result, "operation": {"kind": "PayCost",
            "args": {"player": player, "cost": {"parts": [part]}, "optional": true}}}}),
        json!({"kind": "If", "args": {"condition": {"kind": "PaymentChosen", "args": result},
            "then": {"instructions": dependents}, "otherwise": {"instructions": []}}}),
    ]
}

fn guard(entry: &Entry, mut dependents: Vec<Value>) -> [Value; 2] {
    let paid = paid(entry.chosen);
    for dependent in &mut dependents {
        rewrite(dependent, &mut |node| {
            if node["args"]["result"].as_u64() != Some(entry.moved) {
                return Visit::Children;
            }
            if node["kind"] == "Result" {
                let index = node["args"]["index"].clone();
                *node = json!({"kind": "Query", "args": {"objects": paid, "index": index}});
            } else if node["kind"] == "Transition" {
                *node = paid.clone();
            }
            Visit::Skip
        });
    }
    let selection = json!({"objects": entry.cards, "choose": {"Exactly": constant_one()}});
    let part = json!({"Move": {"selections": [selection], "destination": entry.destination}});
    payment(&entry.player, entry.chosen, part, dependents)
}

/// The action an operation performs once its selections are made.
fn action(operation: &Value) -> &Value {
    if operation["kind"] == "WithSelections" {
        action(&operation["args"]["action"])
    } else {
        operation
    }
}

/// Whether an instruction puts objects onto the battlefield, by moving or creating them.
fn enters(instruction: &Value) -> bool {
    let action = action(&instruction["args"]["operation"]);
    instruction["kind"] == "Effect"
        && match action["kind"].as_str() {
            Some("Move") => action["args"]["destination"].get("Battlefield").is_some(),
            Some("CreateTokens") => true,
            _ => false,
        }
}

/// Whether a set is the objects result `entered` put onto the battlefield.
fn follows(objects: &Value, entered: u64) -> bool {
    let args = &objects["args"];
    match objects["kind"].as_str() {
        Some("Created") => args.as_u64() == Some(entered),
        Some("One") => {
            args["kind"] == "Result"
                && args["args"]["result"].as_u64() == Some(entered)
                && args["args"]["phase"] == "After"
        }
        Some("Transition") => args["result"].as_u64() == Some(entered) && args["phase"] == "After",
        _ => false,
    }
}

/// Whether an instruction changes, moves or sacrifices exactly the entered objects, or
/// creates an unconditional delayed trigger that only does so.
fn dependent(instruction: &Value, entered: u64) -> bool {
    let operation = &instruction["args"]["operation"];
    let args = &operation["args"];
    instruction["kind"] == "Effect"
        && match operation["kind"].as_str() {
            Some("CreateContinuousEffect") => args["affected"]
                .get("Objects")
                .is_some_and(|objects| follows(objects, entered)),
            Some("Move" | "Sacrifice") => follows(&args["objects"], entered),
            Some("CreateDelayedTrigger") => {
                args["intervening_if"].is_null()
                    && args["body"]["instructions"].as_array().is_some_and(|body| {
                        !body.is_empty() && body.iter().all(|i| dependent(i, entered))
                    })
            }
            _ => false,
        }
}

/// The optional entry performed as a payment, when everything after it depends on it.
fn performed(optional: &Value, result: u64) -> Option<[Value; 2]> {
    let decider = &optional["args"]["decider"];
    let [entry, dependents @ ..] = optional["args"]["body"]["instructions"]
        .as_array()?
        .as_slice()
    else {
        return None;
    };
    let entered = entry["args"]["result"].as_u64()?;
    if !enters(entry) || dependents.is_empty() || !dependents.iter().all(|i| dependent(i, entered))
    {
        return None;
    }
    let part = json!({"PlayerAction": {"player": decider, "body": {"instructions": [entry]}}});
    Some(payment(decider, result, part, dependents.to_vec()))
}

/// The guarded form of the optional entry at `at`, and how many instructions it replaces.
fn lowered(instructions: &[Value], at: usize) -> Option<([Value; 2], usize)> {
    let optional = &instructions[at];
    if optional["kind"] != "Optional" {
        return None;
    }
    let body = optional["args"]["body"]["instructions"].as_array()?;
    let entry = hand_entry(&optional["args"]["decider"], body)?;
    // Each sentence inside the optional action acts on the card; later sentences join
    // while they do, and none after the first independent one may name the card.
    let mut dependents: Vec<Value> = body[2..].to_vec();
    if dependents
        .iter()
        .any(|dependent| uses(dependent, &entry) != Some((true, true)))
    {
        return None;
    }
    let mut taken = 1;
    let mut independent = false;
    for sibling in &instructions[at + 1..] {
        match uses(sibling, &entry)? {
            (true, true) if !independent => {
                dependents.push(sibling.clone());
                taken += 1;
            }
            (true, _) => return None,
            (false, _) => independent = true,
        }
    }
    (!dependents.is_empty()).then(|| (guard(&entry, dependents), taken))
}

/// Rewrite every instruction list, innermost lists included.
fn each_list(value: &mut Value, apply: &mut dyn FnMut(&mut Vec<Value>) -> bool) -> bool {
    let mut changed = false;
    if let Some(instructions) = value.get_mut("instructions").and_then(Value::as_array_mut) {
        changed |= apply(instructions);
    }
    match value {
        Value::Object(fields) => {
            for child in fields.values_mut() {
                changed |= each_list(child, apply);
            }
        }
        Value::Array(items) => {
            for child in items {
                changed |= each_list(child, apply);
            }
        }
        _ => {}
    }
    changed
}

pub(super) fn lower(ability: &mut Value) -> bool {
    let mut next = last_result(ability) + 1;
    each_list(ability, &mut |instructions| {
        let mut changed = false;
        let mut at = 0;
        while at < instructions.len() {
            if let Some((guarded, taken)) = lowered(instructions, at) {
                instructions.splice(at..at + taken, guarded);
                changed = true;
            } else if let Some(paid) = (instructions[at]["kind"] == "Optional")
                .then(|| performed(&instructions[at], next))
                .flatten()
            {
                instructions.splice(at..=at, paid);
                next += 1;
                changed = true;
            }
            at += 1;
        }
        changed
    })
}

/// The largest result an ability names, so a restored move gets an unused one.
fn last_result(value: &Value) -> u64 {
    match value {
        Value::Object(fields) => fields
            .iter()
            .map(|(key, child)| match (key.as_str(), child.as_u64()) {
                ("result", Some(result)) if result < u64::from(u32::MAX) - 16 => result,
                _ => last_result(child),
            })
            .max()
            .unwrap_or(0),
        Value::Array(items) => items.iter().map(last_result).max().unwrap_or(0),
        _ => 0,
    }
}

/// The optional entry a guarded payment at `at` restores, with its dependents.
fn surfaced(instructions: &[Value], at: usize, moved: u64) -> Option<Vec<Value>> {
    let pay = operation(&instructions[at], "PayCost")?;
    let chosen = instructions[at]["args"]["result"].as_u64()?;
    let guarded = instructions.get(at + 1)?;
    let [part] = pay["cost"]["parts"].as_array()?.as_slice() else {
        return None;
    };
    let [selection] = part["Move"]["selections"].as_array()?.as_slice() else {
        return None;
    };
    let cards = &selection["objects"];
    let condition = json!({"kind": "PaymentChosen", "args": chosen});
    if pay["optional"] != true
        || selection["choose"] != json!({"Exactly": constant_one()})
        || cards["args"]["scope"]["Players"]["zone"] != "Hand"
        || part["Move"]["destination"].get("Battlefield").is_none()
        || guarded["kind"] != "If"
        || guarded["args"]["condition"] != condition
        || guarded["args"]["otherwise"]["instructions"] != json!([])
    {
        return None;
    }
    let paid = paid(chosen);
    let mut dependents = guarded["args"]["then"]["instructions"].as_array()?.clone();
    for dependent in &mut dependents {
        rewrite(dependent, &mut |node| {
            if *node == paid {
                *node = entered(moved);
                return Visit::Skip;
            }
            if node["kind"] == "Query" && node["args"]["objects"] == paid {
                *node = entered_one(moved, &node["args"]["index"]);
                return Visit::Skip;
            }
            Visit::Children
        });
    }
    let player = &pay["player"];
    let choice = json!({"kind": "Effect", "args": {"result": chosen, "operation": {
        "kind": "ChooseObjects", "args": {"player": player, "objects": cards,
        "count": {"AsManyAsCan": constant_one()}, "constraints": []}}}});
    let movement = json!({"kind": "Effect", "args": {"result": moved, "operation": {
        "kind": "Move", "args": {"objects": {"kind": "Selected", "args": chosen},
        "destination": part["Move"]["destination"], "qualifiers": []}}}});
    let body = json!({"instructions": [choice, movement]});
    let mut restored = vec![json!({"kind": "Optional", "args": {"decider": player, "body": body}})];
    restored.extend(dependents);
    Some(restored)
}

/// The optional entry a performed payment at `at` restores, with its dependents inside.
fn unperformed(instructions: &[Value], at: usize) -> Option<Value> {
    let pay = operation(&instructions[at], "PayCost")?;
    let result = instructions[at]["args"]["result"].as_u64()?;
    let guarded = instructions.get(at + 1)?;
    let [part] = pay["cost"]["parts"].as_array()?.as_slice() else {
        return None;
    };
    let player = &pay["player"];
    let [entry] = part["PlayerAction"]["body"]["instructions"]
        .as_array()?
        .as_slice()
    else {
        return None;
    };
    let dependents = guarded["args"]["then"]["instructions"].as_array()?;
    let entered = entry["args"]["result"].as_u64()?;
    if pay["optional"] != true
        || part["PlayerAction"]["player"] != *player
        || !enters(entry)
        || guarded["kind"] != "If"
        || guarded["args"]["condition"] != json!({"kind": "PaymentChosen", "args": result})
        || guarded["args"]["otherwise"]["instructions"] != json!([])
        || dependents.is_empty()
        || !dependents.iter().all(|i| dependent(i, entered))
    {
        return None;
    }
    let mut body = vec![entry.clone()];
    body.extend(dependents.iter().cloned());
    Some(json!({"kind": "Optional", "args": {"decider": player,
        "body": {"instructions": body}}}))
}

/// The optional-action spelling of each guarded entry. Whether any changed.
pub(super) fn surface(ability: &mut Value) -> bool {
    let mut next = last_result(ability) + 1;
    each_list(ability, &mut |instructions| {
        let mut changed = false;
        let mut at = 0;
        while at < instructions.len() {
            if let Some(restored) = surfaced(instructions, at, next) {
                let len = restored.len();
                instructions.splice(at..at + 2, restored);
                next += 1;
                changed = true;
                at += len;
            } else if let Some(restored) = unperformed(instructions, at) {
                instructions.splice(at..at + 2, [restored]);
                changed = true;
                at += 1;
            } else {
                at += 1;
            }
        }
        changed
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controller() -> Value {
        json!({"kind": "AbilityController"})
    }

    fn hand() -> Value {
        json!({"kind":"Zone","args":{"scope":{"Players":{"zone":"Hand","players":{"kind":"One",
            "args":controller()}}},"kind":"Card","filter":{"kind":"CardType","args":"Planeswalker"}}})
    }

    fn battlefield() -> Value {
        json!({"Battlefield":{"controller":{"Player":controller()},"arrival":{"controller":null,
            "tapped":false,"counters":[],"states":[]}}})
    }

    fn optional_entry() -> Value {
        json!({"kind":"Optional","args":{"decider":controller(),"body":{"instructions":[
            {"kind":"Effect","args":{"result":0,"operation":{"kind":"ChooseObjects","args":{
                "player":controller(),"objects":hand(),"count":{"AsManyAsCan":constant_one()},
                "constraints":[]}}}},
            {"kind":"Effect","args":{"result":1,"operation":{"kind":"Move","args":{
                "objects":{"kind":"Selected","args":0},"destination":battlefield(),
                "qualifiers":[]}}}}]}}})
    }

    fn sacrifice(objects: Value) -> Value {
        json!({"kind":"Effect","args":{"result":u32::MAX,"operation":{"kind":"Sacrifice",
            "args":{"objects":{"kind":"One","args":objects}}}}})
    }

    fn body(instructions: Value) -> Value {
        json!({"kind":"Activated","args":{"body":{"instructions":instructions}}})
    }

    #[test]
    fn a_dependent_sentence_guards_the_entry_as_a_payment_and_back() {
        let printed = body(json!([
            optional_entry(),
            sacrifice(entered_one(1, &json!(0)))
        ]));
        let mut ability = printed.clone();
        assert!(lower(&mut ability));
        let instructions = &ability["args"]["body"]["instructions"];
        assert_eq!(instructions[0]["args"]["operation"]["kind"], "PayCost");
        assert_eq!(
            instructions[1]["args"]["then"]["instructions"][0],
            sacrifice(json!({"kind":"Query","args":{"objects":paid(0),"index":0}}))
        );
        let mut restored = ability.clone();
        assert!(surface(&mut restored));
        let mut again = restored.clone();
        assert!(lower(&mut again));
        assert_eq!(again, ability);
    }

    #[test]
    fn a_return_its_followers_act_on_is_performed_as_a_payment_and_back() {
        let returned = json!({"kind":"Effect","args":{"result":0,"operation":{"kind":"Move",
            "args":{"objects":{"kind":"One","args":{"kind":"Target","args":{"slot":0,"index":0}}},
            "destination":battlefield(),"qualifiers":[]}}}});
        let optional = |after: Value| {
            body(json!([{"kind":"Optional","args":{"decider":controller(),
                "body":{"instructions":[returned, after]}}}]))
        };
        let printed = optional(sacrifice(entered_one(0, &json!(0))));
        let mut ability = printed.clone();
        assert!(lower(&mut ability));
        let instructions = &ability["args"]["body"]["instructions"];
        let part = &instructions[0]["args"]["operation"]["args"]["cost"]["parts"][0];
        assert_eq!(part["PlayerAction"]["body"]["instructions"][0], returned);
        assert_eq!(instructions[1]["args"]["condition"]["args"], 1);
        assert_eq!(
            instructions[1]["args"]["then"]["instructions"][0],
            sacrifice(entered_one(0, &json!(0)))
        );
        let mut restored = ability.clone();
        assert!(surface(&mut restored));
        assert_eq!(restored, printed);
        // "Return this Equipment ..., then attach it to that creature" stays optional.
        let attached = json!({"kind":"Effect","args":{"result":1,"operation":{"kind":"Attach",
            "args":{"attachments":{"kind":"One","args":entered_one(0, &json!(0))}}}}});
        let mut ability = optional(attached);
        assert!(!lower(&mut ability));
    }

    #[test]
    fn a_test_of_the_card_or_independent_work_keeps_the_optional_action() {
        let gain = json!({"kind":"Effect","args":{"operation":{"kind":"GainLife","args":{}}}});
        let tested = json!({"kind":"If","args":{"condition":{"kind":"Compare","args":{
            "left":entered(1)}},"then":{"instructions":[gain]}}});
        for after in [json!([tested]), json!([gain]), json!([])] {
            let mut instructions = vec![optional_entry()];
            instructions.extend(after.as_array().unwrap().iter().cloned());
            let mut ability = body(Value::Array(instructions));
            let unchanged = ability.clone();
            assert!(!lower(&mut ability));
            assert_eq!(ability, unchanged);
        }
        let later = body(json!([
            optional_entry(),
            sacrifice(entered_one(1, &json!(0))),
            gain,
            sacrifice(entered_one(1, &json!(0)))
        ]));
        let mut ability = later.clone();
        assert!(!lower(&mut ability));
    }
}
