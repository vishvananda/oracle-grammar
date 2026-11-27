//! Use native forms for resolution costs with one proved payment action.
use super::{
    trace,
    walk::{Uses, lossless},
};
use crate::ir::CostPart;
use serde_json::{Value, json};

fn controller() -> Value {
    json!({"kind":"One","args":{"kind":"AbilityController"}})
}

pub fn hand_discard(instruction: &Value) -> bool {
    if instruction["kind"] != "Effect" {
        return false;
    }
    let operation = &instruction["args"]["operation"];
    operation["kind"] == "Discard"
        && operation["args"]["players"] == controller()
        && operation["args"]["random"] == false
        && operation["args"]["amount"]
            == json!({"kind":"CountObjects","args":{"kind":"Zone","args":{
                "scope":{"Players":{"zone":"Hand","players":controller()}},
                "kind":"Card","filter":{"kind":"Any"}
            }}})
}

fn uses_of(uses: &Uses, value: &Value, count: usize) -> bool {
    trace::number(value).is_some_and(|id| {
        uses.total.get(&id) == Some(&count) && uses.definitions.get(&id) == Some(&1)
    })
}

fn singular_selection(objects: &Value) -> bool {
    if objects["kind"] != "One" {
        return false;
    }
    let reference = &objects["args"];
    matches!(
        reference["kind"].as_str(),
        Some("Target" | "Source" | "EventSubject" | "EventDestination")
    ) || *reference
        == json!({"kind":"Query","args":{"index":0,"objects":{
            "kind":"InZone","args":{"objects":{"kind":"One","args":{"kind":"EventDestination"}},"zone":"Graveyard"}
        }}})
}

fn move_part(move_args: &Value, objects: &Value, choose: Value) -> Option<Value> {
    if move_args["qualifiers"] != json!([])
        || !(move_args["destination"]
            == json!({"Exile":{"visibility":"FaceUp","arrival":{"counters":[],"states":[]}}})
            || move_args["destination"] == json!({"OwnersHand":{"counters":[],"states":[]}}))
    {
        return None;
    }
    Some(json!({"Move":{
        "selections":[{"objects":objects,"choose":choose}],
        "destination":move_args["destination"],
    }}))
}

fn chosen_move(choice: &Value, movement: &Value, uses: &Uses) -> Option<Value> {
    let choose = &choice["args"]["operation"];
    let choose_args = &choose["args"];
    let objects = &choose_args["objects"];
    let selector = &movement["args"]["operation"]["args"]["objects"];
    if choice["kind"] != "Effect"
        || choose["kind"] != "ChooseObjects"
        || choose_args["player"] != json!({"kind":"AbilityController"})
        || choose_args["constraints"] != json!([])
        || choose_args["count"] != json!({"AsManyAsCan":{"kind":"Constant","args":1}})
        || objects["kind"] != "Zone"
        || objects["args"]["kind"] != "Card"
        || objects["args"]["scope"]
            != json!({"Players":{"zone":"Graveyard","players":controller()}})
        || movement["kind"] != "Effect"
        || movement["args"]["operation"]["kind"] != "Move"
        || selector["kind"] != "Selected"
        || trace::number(&selector["args"]) != trace::number(&choice["args"]["result"])
        || !uses_of(uses, &choice["args"]["result"], 2)
        || !uses_of(uses, &movement["args"]["result"], 1)
    {
        return None;
    }
    // Within a cost, choosing as many as possible cannot make paying for a
    // missing card optional. Only the following Move reads the selection.
    move_part(
        &movement["args"]["operation"]["args"],
        objects,
        json!({"Exactly":{"kind":"Constant","args":1}}),
    )
}

fn action_part(inner: &Value) -> Option<Value> {
    if hand_discard(inner) {
        return Some(json!("DiscardHand"));
    }
    if inner["kind"] != "Effect" {
        return None;
    }
    let operation = &inner["args"]["operation"];
    let args = &operation["args"];
    if operation["kind"] == "Move" && singular_selection(&args["objects"]) {
        return move_part(args, &args["objects"], Value::Null);
    }
    if operation["kind"] != "Keyword" || args["actor"] != controller() {
        return None;
    }
    let action = args["action"].as_object()?;
    if action.len() != 1 {
        return None;
    }
    let (name, body) = action.iter().next()?;
    let amount = &body["amount"];
    if matches!(name.as_str(), "Blight" | "CollectEvidence") && amount["kind"] == "Constant" {
        return Some(json!({"Keyword":{"action":name,"amount":amount}}));
    }
    None
}

fn part(effect: &Value, check: &Value, uses: &Uses, tracing: bool) -> Option<Value> {
    let pay = &effect["args"]["operation"];
    let controller = json!({"kind":"AbilityController"});
    let condition = &check["args"]["condition"];
    let test = if condition["kind"] == "Not" {
        &condition["args"]
    } else {
        condition
    };
    if effect["kind"] != "Effect"
        || pay["kind"] != "PayCost"
        || pay["args"]["player"] != controller
        || pay["args"]["optional"] != true
        || check["kind"] != "If"
        || test["kind"] != "PaymentChosen"
        || trace::number(&test["args"]) != trace::number(&effect["args"]["result"])
        || !uses_of(uses, &effect["args"]["result"], 2)
    {
        return None;
    }
    let [part] = pay["args"]["cost"]["parts"].as_array()?.as_slice() else {
        return None;
    };
    let action = &part["PlayerAction"];
    if action["player"] != controller {
        return None;
    }
    lossless::<CostPart>(part, tracing)?;
    match action["body"]["instructions"].as_array()?.as_slice() {
        [inner] if uses_of(uses, &inner["args"]["result"], 1) => action_part(inner),
        [choice, movement] => chosen_move(choice, movement, uses),
        _ => None,
    }
}

pub(super) fn may_need(value: &Value) -> bool {
    let action = &value["PlayerAction"];
    action["body"]["instructions"]
        .as_array()
        .is_some_and(|items| match items.as_slice() {
            [inner] => matches!(
                inner["args"]["operation"]["kind"].as_str(),
                Some("Discard" | "Move" | "Keyword")
            ),
            [choice, movement] => {
                choice["args"]["operation"]["kind"] == "ChooseObjects"
                    && movement["args"]["operation"]["kind"] == "Move"
            }
            _ => false,
        })
}

/// The one following test reads only whether the payment was made. No external
/// reader observes the original action result or its selected objects.
pub(super) fn program(items: &mut [Value], uses: &Uses, tracing: bool) -> bool {
    let mut changed = false;
    for at in 1..items.len() {
        if let Some(normalized) = part(&items[at - 1], &items[at], uses, tracing) {
            items[at - 1]["args"]["operation"]["args"]["cost"]["parts"][0] = normalized;
            changed = true;
        }
    }
    changed
}
