//! An immediate random preparation chooses the same object as an AtRandom
//! selection followed by its move. Only remove a result with no other reader.
use super::{
    trace,
    walk::{Uses, lossless},
};
use crate::ir::{Instruction, Operation};
use serde_json::{Value, json};

pub(super) fn may_need(op: &Value) -> bool {
    op["kind"] == "ChooseObjects"
        && op["args"]["count"] == json!({"AsManyAsCan":{"kind":"Constant","args":1}})
        && op["args"]["constraints"] == json!(["AtRandom"])
}

pub(super) fn program(items: &mut Vec<Value>, uses: &Uses, tracing: bool) -> bool {
    let [choose, moved, grant] = items.as_mut_slice() else {
        return false;
    };
    let pick = &choose["args"]["operation"];
    let choice = &pick["args"];
    if !may_need(pick)
        || choice["player"]["kind"] != "AbilityController"
        || choice["objects"]["kind"] != "Zone"
        || choice["objects"]["args"]["scope"]["Players"]["zone"] != "Graveyard"
    {
        return false;
    }
    let Some(selection) = trace::number(&choose["args"]["result"]) else {
        return false;
    };
    let effect = &moved["args"]["operation"];
    let movement = &effect["args"];
    let projected = &movement["objects"];
    let permission = &grant["args"]["operation"];
    let rule = &permission["args"]["changes"][0]["args"]["PlayPermission"];
    let receipt = &rule["cards"]["args"];
    if choose["kind"] != "Effect"
        || moved["kind"] != "Effect"
        || grant["kind"] != "Effect"
        || effect["kind"] != "Move"
        || movement["destination"]["Exile"]["visibility"] != "FaceUp"
        || projected["kind"] != "Selected"
        || trace::number(&projected["args"]) != Some(selection)
        || uses.definitions.get(&selection) != Some(&1)
        || uses.total.get(&selection) != Some(&2)
        || permission["kind"] != "CreateContinuousEffect"
        || permission["args"]["duration"] != "EndOfTurn"
        || rule["cards"]["kind"] != "One"
        || receipt["kind"] != "Result"
        || receipt["args"]["phase"] != "After"
        || receipt["args"]["index"] != 0
        || !trace::same(
            &receipt["args"]["result"],
            &moved["args"]["result"],
            tracing,
        )
        || lossless::<Instruction>(choose, tracing).is_none()
        || lossless::<Operation>(effect, tracing).is_none()
    {
        return false;
    }
    let mut action = effect.clone();
    action["args"]["objects"] = json!({"kind":"Prepared","args":0});
    moved["args"]["operation"] = json!({"kind":"WithSelections","args":{
        "steps":[{"binding":0,"chooser":"Chance","objects":choice["objects"],
            "count":choice["count"],"constraints":[]}],
        "action":action
    }});
    items.remove(0);
    true
}
