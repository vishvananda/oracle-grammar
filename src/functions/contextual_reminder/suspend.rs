//! A granted Suspend reminder uses the exiled card's owner. "Your" requires
//! a linked arrival from the ability controller's own zone.
use super::evidence::{step_operation, target_spec};
use super::object_notes::{filter_proves, is_controller};
use serde_json::Value;

pub(super) fn mentions(ability: &Value, mut visit: impl FnMut(&'static str) -> bool) -> bool {
    let frame = &ability["args"];
    let Some(steps) = instructions(&frame["body"]) else {
        return false;
    };
    for (i, step) in steps.iter().enumerate() {
        if step_operation(step).is_some_and(|op| {
            matches!(op["kind"].as_str(), Some("Counter" | "Move"))
                && arrival_with_time(&op["args"]["destination"]).is_some_and(grants_on_arrival)
        }) && visit("GrantedSuspendOwner")
        {
            return true;
        }
        let Some(subject) = granted_subject(step) else {
            continue;
        };
        let prefix = &steps[..i];
        let arrival = after_result(subject)
            .and_then(|result| producer(prefix, result))
            .filter(|op| op["kind"] == "Move" && op["args"]["destination"]["Exile"].is_object());
        let cost = owned_cost_arrival(subject, frame);
        if arrival.is_none() && !cost {
            continue;
        }
        if !arrival.is_some_and(|op| arrival_with_time(&op["args"]["destination"]).is_some())
            && !prefix.iter().any(|step| places_time_on(step, subject))
        {
            continue;
        }
        if visit("GrantedSuspendOwner")
            || (cost || arrival.is_some_and(|op| owned_nonland_move(op, subject, prefix, frame)))
                && visit("GrantedSuspendController")
        {
            return true;
        }
    }
    false
}

fn instructions(program: &Value) -> Option<&[Value]> {
    let steps = program["instructions"].as_array()?;
    (steps.len() <= 64).then_some(steps)
}

fn arrival_with_time(destination: &Value) -> Option<&Value> {
    let arrival = &destination["Exile"]["arrival"];
    arrival["counters"]
        .as_array()?
        .iter()
        .take(16)
        .any(|pair| pair[0]["Named"] == "TimeCounter" && positive(&pair[1]))
        .then_some(arrival)
}

fn positive(amount: &Value) -> bool {
    amount["kind"] == "Constant" && amount["args"].as_u64().is_some_and(|n| n > 0)
}

fn basic_suspend(ability: &Value) -> bool {
    ability["kind"] == "Mechanic" && ability["args"]["mechanic"] == "BasicSuspend"
}

fn grants_on_arrival(arrival: &Value) -> bool {
    arrival["states"].as_array().is_some_and(|states| {
        states.iter().take(16).any(|state| {
            state["Changes"].as_array().is_some_and(|changes| {
                changes.iter().take(16).any(|change| {
                    change["kind"] == "GrantAbility" && basic_suspend(&change["args"])
                })
            })
        })
    })
}

fn granted_subject(step: &Value) -> Option<&Value> {
    let step = if step["kind"] == "If" {
        let [grant] = instructions(&step["args"]["then"])? else {
            return None;
        };
        grant
    } else {
        step
    };
    let op = step_operation(step)?;
    if op["kind"] != "EffectOn" {
        return None;
    }
    let args = &op["args"];
    args["effects"]
        .as_array()?
        .iter()
        .take(16)
        .any(|effect| {
            let term = &effect["args"];
            effect["kind"] == "Term"
                && term["family"] == "ExiledCardEffect"
                && term["form"] == "AddAbility"
                && term["operands"][0]["kind"] == "Ability"
                && basic_suspend(&term["operands"][0]["args"])
        })
        .then(|| &args["subject"]["Objects"])
}

fn places_time_on(step: &Value, objects: &Value) -> bool {
    step_operation(step).is_some_and(|op| {
        op["kind"] == "PutCounters"
            && op["args"]["placements"]
                .as_array()
                .is_some_and(|placements| {
                    placements.iter().take(16).any(|placement| {
                        let recipients = &placement["recipients"];
                        placement["kind"]["Named"] == "TimeCounter"
                            && positive(&placement["amount"])
                            && recipients["kind"] == "Objects"
                            && recipients["args"] == *objects
                    })
                })
    })
}

fn producer<'a>(steps: &'a [Value], result: &Value) -> Option<&'a Value> {
    steps.iter().find_map(|step| {
        if step["args"]["result"] == *result {
            step_operation(step)
        } else {
            None
        }
    })
}

fn subject_query(subject: &Value) -> Option<&Value> {
    let query = &subject["args"];
    (subject["kind"] == "One" && query["kind"] == "Query" && query["args"]["index"] == 0)
        .then(|| &query["args"]["objects"])
}

fn after_result(subject: &Value) -> Option<&Value> {
    let object = if subject["kind"] == "One" && subject["args"]["kind"] == "Result" {
        &subject["args"]
    } else if let Some(query) = subject_query(subject) {
        if query["kind"] == "Matching" {
            &query["args"]["objects"]
        } else {
            query
        }
    } else {
        subject
    };
    let reference = &object["args"];
    (matches!(object["kind"].as_str(), Some("Result" | "Transition"))
        && reference["phase"] == "After")
        .then(|| reference.get("result"))
        .flatten()
}

fn nonland(filter: &Value) -> bool {
    filter["kind"] == "Not"
        && filter["args"]["kind"] == "CardType"
        && filter["args"]["args"] == "Land"
}

fn own_zone(filter: &Value, zone: &str) -> bool {
    filter["zone"] == zone && is_controller(&filter["players"])
}

fn owned_cost_arrival(subject: &Value, frame: &Value) -> bool {
    let Some(objects) = subject_query(subject) else {
        return false;
    };
    let receipt = &objects["args"];
    if objects["kind"] != "CostObjects"
        || receipt["payment"] != "Activation"
        || receipt["view"] != "Performed"
        || receipt["phase"] != "After"
    {
        return false;
    }
    let Some([part]) = receipt["component"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let Some(index) = part.as_u64().filter(|&n| n < 32) else {
        return false;
    };
    let movement = &frame["cost"]["parts"][index as usize]["Move"];
    let Some([selection]) = movement["selections"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let objects = &selection["objects"];
    movement["destination"]["Exile"].is_object()
        && objects["kind"] == "Zone"
        && own_zone(&objects["args"]["scope"]["Players"], "Hand")
        && filter_proves(&objects["args"]["filter"], &nonland, 0, &mut 256)
}

fn owned_nonland_move(op: &Value, subject: &Value, steps: &[Value], frame: &Value) -> bool {
    let objects = &op["args"]["objects"];
    match objects["kind"].as_str() {
        Some("One") => {
            target_spec(&objects["args"], &frame["announcement"]).is_some_and(|target| {
                let filter = &target["filter"];
                filter_proves(filter, &nonland, 0, &mut 256)
                    && filter_proves(
                        filter,
                        &|term| {
                            term["kind"] == "InZone"
                                && own_zone(&term["args"]["Players"], "Graveyard")
                        },
                        0,
                        &mut 256,
                    )
            })
        }
        Some("Observed") => {
            subject_query(subject).is_some_and(|query| {
                query["kind"] == "Matching"
                    && filter_proves(&query["args"]["filter"], &nonland, 0, &mut 256)
            }) && producer(steps, &objects["args"]).is_some_and(|producer| {
                producer["kind"] == "RevealUntil"
                    && producer["args"]["library"]["kind"] == "AbilityController"
            })
        }
        _ => false,
    }
}
