use super::*;
fn root(player: Value) -> Value {
    json!({"kind":"Triggered","args":{
    "event":{"Object":{"event":"LeavesBattlefield","objects":{"kind":"SameObject","args":{"kind":"Source"}}}},
    "intervening_if":null,"limits":{"per_turn":null,"total":null},
    "announcement":{"$sym":"announcement","args":[],"at":0},
    "body":{"instructions":[{"kind":"Effect","args":{"result":0,"operation":{"kind":"GainLife",
        "args":{"players":{"kind":"One","args":player},"amount":{"kind":"Constant","args":3}}}}}]}}})
}
fn unresolved() -> Value {
    json!({"$sym":"that_player","args":[],"at":5})
}
#[test]
fn no_player_event_and_no_local_introduction_cannot_mean_source_controller() {
    assert!(prepare(&root(unresolved())).is_err());
}
#[test]
fn explicit_you_controller_prior_and_target_recipients_remain_unmodified() {
    for player in [
        json!({"kind":"AbilityController"}),
        json!({"kind":"ControllerOf","args":{"kind":"Source"}}),
        json!({"kind":"Prior","args":"Remembered"}),
        json!({"kind":"Target","args":{"slot":0,"index":0}}),
    ] {
        let value = root(player);
        let before = value.clone();
        prepare(&value).unwrap();
        assert_eq!(value, before);
    }
}
#[test]
fn explicit_announced_target_prevents_this_guard() {
    let mut value = root(unresolved());
    value["args"]["announcement"] = json!({"targets":[{"slot":0,"kind":"Player",
        "count":{"Exactly":{"kind":"Constant","args":1}},"filter":{"kind":"Any"},"players":{"kind":"All"},
        "distribute":null,"constraints":[],"chooser":null}],"modes":null});
    prepare(&value).unwrap();
}
#[test]
fn extra_instructions_conditions_limits_foreign_events_and_amounts_are_outside_scope() {
    let original = root(unresolved());
    let mut value = original.clone();
    value["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"Effect"}));
    prepare(&value).unwrap();
    let mut value = original.clone();
    value["args"]["intervening_if"] = json!({"kind":"True"});
    prepare(&value).unwrap();
    let mut value = original.clone();
    value["args"]["limits"]["per_turn"] = json!(1);
    prepare(&value).unwrap();
    let mut value = original.clone();
    value["args"]["event"]["Object"]["objects"]["args"] = json!({"kind":"GrantingObject"});
    prepare(&value).unwrap();
    let mut value = original;
    value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"] =
        json!({"kind":"AnnouncedX"});
    prepare(&value).unwrap();
}
#[test]
fn holders_conditionals_branches_and_modes_are_not_traversed() {
    for value in [
        json!({"$sym":"holder","args":[root(unresolved())]}),
        json!({"kind":"Conditional","args":{"then":[root(unresolved())]}}),
        json!({"kind":"CostBranch","args":{"paid":root(unresolved())}}),
        json!({"announcement":{},"weight":1,"body":{"instructions":[root(unresolved())]}}),
    ] {
        prepare(&value).unwrap();
    }
}
