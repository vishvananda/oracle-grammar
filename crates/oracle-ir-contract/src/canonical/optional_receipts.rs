//! Normalize continuations limited to the actual result of an optional action.
use super::{
    trace,
    walk::{Uses, Visit, Walker, lossless},
};
use crate::{ir::Instruction, ir_schema::Ty};
use serde_json::{Value, json};

fn controller() -> Value {
    json!({"kind":"One","args":{"kind":"AbilityController"}})
}

fn exile_permission(operation: &Value) -> bool {
    operation["kind"] == "CreateContinuousEffect"
        && operation["args"]["changes"][0]["args"]
            .get("PlayPermission")
            .is_some()
}

fn optional_exile(optional: &Value, uses: &Uses, tracing: bool) -> bool {
    if optional["kind"] != "Optional" || optional["args"]["decider"] != controller()["args"] {
        return false;
    }
    let Some([movement, permission]) = optional["args"]["body"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return false;
    };
    let moved = &movement["args"]["operation"];
    let granted = &permission["args"]["operation"];
    if movement["kind"] != "Effect"
        || moved["kind"] != "Move"
        || moved["args"]["destination"]
            != json!({"Exile":{"visibility":"FaceUp","arrival":{"counters":[],"states":[]}}})
        || moved["args"]["qualifiers"] != json!([])
        || permission["kind"] != "Effect"
        || !exile_permission(granted)
        || lossless::<Instruction>(optional, tracing).is_none()
    {
        return false;
    }
    let Some(move_id) = trace::number(&movement["args"]["result"]) else {
        return false;
    };
    let Some(permission_id) = trace::number(&permission["args"]["result"]) else {
        return false;
    };
    if uses.definitions.get(&move_id) != Some(&1)
        || uses.definitions.get(&permission_id) != Some(&1)
        || uses.total.get(&permission_id) != Some(&1)
    {
        return false;
    }
    let set = &granted["args"]["changes"][0]["args"]["PlayPermission"]["cards"];
    let receipt = match set["kind"].as_str() {
        Some("One") if set["args"]["kind"] == "Result" && set["args"]["args"]["index"] == 0 => {
            &set["args"]["args"]
        }
        Some("Transition") => &set["args"],
        _ => return false,
    };
    receipt["phase"] == "After"
        && trace::number(&receipt["result"]) == Some(move_id)
        && granted["args"]
            == json!({"source":{"kind":"Source"},"affected":{"Players":controller()},
            "changes":[{"kind":"PlayerRule","args":{"PlayPermission":{
                "cards":set,"from":"Exile","action":"Either","filter":{"kind":"Any"}
            }}}],"duration":"EndOfTurn","evaluation":"LiveObjects"})
}

fn one_card_for_hand(operation: &Value) -> bool {
    operation["kind"] == "ChooseObjects"
        && operation["args"]["objects"]["kind"] == "Zone"
        && operation["args"]["objects"]["args"]["scope"]["Players"]["zone"] == "Graveyard"
}

fn graveyard_choice(operation: &Value) -> bool {
    one_card_for_hand(operation)
        && operation["args"]["count"] == json!({"Exactly":{"kind":"Constant","args":1}})
}

fn optional_return(optional: &Value, uses: &Uses, tracing: bool) -> bool {
    let controller = json!({"kind":"AbilityController"});
    if optional["kind"] != "Optional" || optional["args"]["decider"] != controller {
        return false;
    }
    let Some([selection, movement]) = optional["args"]["body"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return false;
    };
    let selected = &selection["args"]["operation"];
    let moved = &movement["args"]["operation"];
    let Some(id) = trace::number(&selection["args"]["result"]) else {
        return false;
    };
    let Some(move_id) = trace::number(&movement["args"]["result"]) else {
        return false;
    };
    selection["kind"] == "Effect"
        && movement["kind"] == "Effect"
        && graveyard_choice(selected)
        && selected["args"]["objects"]["args"]["scope"]
            == json!({"Players":{"zone":"Graveyard","players":{"kind":"One","args":controller}}})
        && selected["args"]["objects"]["args"]["kind"] == "Card"
        && selected["args"]["player"] == controller
        && selected["args"]["constraints"] == json!([])
        && moved["kind"] == "Move"
        && moved["args"]["qualifiers"] == json!([])
        && moved["args"]["destination"] == json!({"OwnersHand":{"counters":[],"states":[]}})
        && moved["args"]["objects"]["kind"] == "Selected"
        && trace::number(&moved["args"]["objects"]["args"]) == Some(id)
        && uses.definitions.get(&id) == Some(&1)
        && uses.total.get(&id) == Some(&2)
        && uses.definitions.get(&move_id) == Some(&1)
        && uses.total.get(&move_id) == Some(&1)
        && lossless::<Instruction>(optional, tracing).is_some()
}

// These two tests ask for a successful transition even when they sit outside
// the option. Skipping the producer leaves no matching object.
fn successful_transition(condition: &Value, producer: &Value, id: u64) -> bool {
    let args = &condition["args"];
    if producer["kind"] == "Destroy"
        && condition["kind"] == "Compare"
        && args["test"]
            == json!({"kind":"Compare","args":{"comparison":"Greater","value":{"kind":"Constant","args":0}}})
        && args["left"]["kind"] == "CountObjects"
        && args["left"]["args"]["kind"] == "Matching"
    {
        let matched = &args["left"]["args"]["args"];
        let transition = &matched["objects"];
        return transition["kind"] == "Transition"
            && transition["args"]["phase"] == "After"
            && trace::number(&transition["args"]["result"]) == Some(id)
            && matched["filter"]
                == json!({"kind":"InZone","args":{"Players":{"zone":"Graveyard","players":{"kind":"All"}}}});
    }
    let object = &args["object"];
    producer["kind"] == "Move"
        && producer["args"]["destination"].get("Battlefield").is_some()
        && condition["kind"] == "ObjectMatches"
        && object["kind"] == "Result"
        && object["args"]["phase"] == "After"
        && object["args"]["index"] == 0
        && trace::number(&object["args"]["result"]) == Some(id)
        && args["filter"]["kind"] == "Subtype"
}

fn optional_success(optional: &Value, uses: &Uses, tracing: bool) -> bool {
    if optional["kind"] != "Optional" || optional["args"]["decider"]["kind"] != "AbilityController"
    {
        return false;
    }
    let Some(items) = optional["args"]["body"]["instructions"].as_array() else {
        return false;
    };
    let (producer, followup) = match items.as_slice() {
        [destroy, followup] if destroy["args"]["operation"]["kind"] == "Destroy" => {
            (destroy, followup)
        }
        [choose, movement, followup]
            if choose["kind"] == "Effect"
                && choose["args"]["operation"]["kind"] == "ChooseObjects"
                && movement["args"]["operation"]["kind"] == "Move" =>
        {
            (movement, followup)
        }
        _ => return false,
    };
    let Some(id) = trace::number(&producer["args"]["result"]) else {
        return false;
    };
    producer["kind"] == "Effect"
        && followup["kind"] == "If"
        && followup["args"]["otherwise"]["instructions"] == json!([])
        && uses.definitions.get(&id) == Some(&1)
        && successful_transition(
            &followup["args"]["condition"],
            &producer["args"]["operation"],
            id,
        )
        && lossless::<Instruction>(optional, tracing).is_some()
}

/// Testing an announced target's power and offering a single free action do
/// not change state. This offer exports no result when its test fails.
fn conditional_target(optional: &mut Value, uses: &Uses, tracing: bool) -> bool {
    if optional["kind"] != "Optional"
        || optional["args"]["decider"] != json!({"kind":"AbilityController"})
    {
        return false;
    }
    let Some([guard]) = optional["args"]["body"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return false;
    };
    let condition = &guard["args"]["condition"];
    let target = &condition["args"]["object"];
    let filter = &condition["args"]["filter"];
    let test = &filter["args"]["test"];
    let comparison = &test["args"]["value"];
    let stat = &comparison["args"];
    let pure_threshold = comparison["kind"] == "Constant"
        || comparison["kind"] == "ObjectProperty"
            && stat["object"]["kind"] == "Source"
            && stat["property"] == "Power"
            && stat["information"] == "CurrentOrLastKnown";
    if guard["kind"] != "If"
        || condition["kind"] != "ObjectMatches"
        || target["kind"] != "Target"
        || filter["kind"] != "Property"
        || filter["args"]["property"] != "Power"
        || test["kind"] != "Compare"
        || !pure_threshold
        || guard["args"]["otherwise"]["instructions"] != json!([])
    {
        return false;
    }
    let Some([result]) = guard["args"]["then"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return false;
    };
    let action = &result["args"]["operation"];
    let objects = &action["args"]["objects"];
    let single_target_action = action["kind"] == "Destroy"
        || action["kind"] == "Move" && action["args"]["destination"].get("OwnersHand").is_some();
    if result["kind"] != "Effect"
        || !single_target_action
        || objects["kind"] != "One"
        || !trace::same(&objects["args"], target, tracing)
        || !trace::number(&result["args"]["result"]).is_some_and(|id| {
            uses.total.get(&id) == Some(&1) && uses.definitions.get(&id) == Some(&1)
        })
        || lossless::<Instruction>(optional, tracing).is_none()
    {
        return false;
    }
    let mut guard = guard.clone();
    let mut choice = optional.take();
    choice["args"]["body"] = guard["args"]["then"].take();
    guard["args"]["then"] = json!({"instructions":[choice]});
    *optional = guard;
    true
}

pub(super) fn may_need(instruction: &Value) -> bool {
    if instruction["kind"] != "Optional" {
        return false;
    }
    let Some(items) = instruction["args"]["body"]["instructions"].as_array() else {
        return false;
    };
    let operation = |at: usize| &items[at]["args"]["operation"];
    match items.len() {
        1 => items[0]["kind"] == "If",
        2 => {
            graveyard_choice(operation(0))
                || exile_permission(operation(1))
                || operation(0)["kind"] == "Destroy" && items[1]["kind"] == "If"
        }
        3 => {
            operation(0)["kind"] == "ChooseObjects"
                && operation(1)["kind"] == "Move"
                && items[2]["kind"] == "If"
        }
        _ => false,
    }
}

pub(super) fn program(items: &mut Vec<Value>, uses: &Uses, tracing: bool) -> bool {
    let mut at = 0;
    let mut changed = false;
    while at < items.len() {
        let item = &mut items[at];
        if optional_exile(item, uses, tracing) || optional_success(item, uses, tracing) {
            // The continuation reaches only objects actually moved by this
            // option. If the player declines, there are no such objects.
            let continuation = item["args"]["body"]["instructions"]
                .as_array_mut()
                .expect("proved optional transition")
                .pop()
                .expect("proved continuation");
            items.insert(at + 1, continuation);
            at += 1;
            changed = true;
        } else if conditional_target(item, uses, tracing) {
            changed = true;
        } else if optional_return(item, uses, tracing) {
            // An eligible card must be chosen after accepting. With no eligible
            // card, either form produces no card to return.
            item["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["count"] =
                json!({"AsManyAsCan":{"kind":"Constant","args":1}});
            changed = true;
        }
        at += 1;
    }
    changed
}

const PRINT_SITES: usize = 4;

fn single_return(optional: &Value) -> bool {
    let Some([choice, movement]) = optional["args"]["body"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return false;
    };
    let operation = &choice["args"]["operation"];
    optional["kind"] == "Optional"
        && one_card_for_hand(operation)
        && operation["args"]["count"] == json!({"AsManyAsCan":{"kind":"Constant","args":1}})
        && movement["args"]["operation"]["kind"] == "Move"
        && movement["args"]["operation"]["args"]["destination"]
            .get("OwnersHand")
            .is_some()
}

fn after_transition(optional: &Value, followup: &Value) -> bool {
    if optional["kind"] != "Optional"
        || followup["kind"] != "If"
        || followup["args"]["otherwise"]["instructions"] != json!([])
    {
        return false;
    }
    let Some(producer) = optional["args"]["body"]["instructions"]
        .as_array()
        .and_then(|items| items.last())
    else {
        return false;
    };
    trace::number(&producer["args"]["result"]).is_some_and(|result| {
        successful_transition(
            &followup["args"]["condition"],
            &producer["args"]["operation"],
            result,
        )
    })
}

fn mentions(value: &Value, depth: usize, work: &mut usize) -> bool {
    if depth > 128 || *work == 0 {
        return false;
    }
    *work -= 1;
    if single_return(value) {
        return true;
    }
    match value {
        Value::Object(fields) => fields
            .values()
            .any(|value| mentions(value, depth + 1, work)),
        Value::Array(items) => {
            items
                .windows(2)
                .any(|pair| after_transition(&pair[0], &pair[1]))
                || items.iter().any(|value| mentions(value, depth + 1, work))
        }
        _ => false,
    }
}

#[derive(Default)]
struct PrintSites(Vec<(String, Value)>);
impl Visit for PrintSites {
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        _: bool,
        path: &str,
        _: &mut Walker<'_>,
    ) -> Result<(), String> {
        if self.0.len() >= PRINT_SITES {
            return Ok(());
        }
        if matches!(ty, Ty::Named(name) if name == "Instruction") && single_return(value) {
            self.0.push((
                format!("{path}/args/body/instructions/0/args/operation/args/count"),
                json!({"Exactly":{"kind":"Constant","args":1}}),
            ));
        }
        if matches!(ty, Ty::Vec(inner) if matches!(inner.as_ref(), Ty::Named(name) if name == "Instruction"))
            && let Some(items) = value.as_array()
        {
            for (at, pair) in items.windows(2).enumerate() {
                if self.0.len() == PRINT_SITES {
                    break;
                }
                if after_transition(&pair[0], &pair[1]) {
                    let mut projected = items.clone();
                    let continuation = projected.remove(at + 1);
                    projected[at]["args"]["body"]["instructions"]
                        .as_array_mut()
                        .expect("proved optional transition")
                        .push(continuation);
                    self.0.push((path.into(), Value::Array(projected)));
                }
            }
        }
        Ok(())
    }
}

/// Reconstruct an optional choice or its dependent sentence from the current
/// operands. Only semantically verified complete alternatives may print.
pub fn print_views(ability: &Value) -> Result<Vec<Value>, String> {
    if !mentions(ability, 0, &mut 131_072) {
        return Ok(Vec::new());
    }
    let mut original = ability.clone();
    let mut sites = PrintSites::default();
    Walker::new(super::schema()?).walk(
        &mut original,
        &Ty::Named("Ability".into()),
        false,
        "",
        0,
        &mut sites,
    )?;
    super::set_views::verified_views(original, &sites.0)
}
