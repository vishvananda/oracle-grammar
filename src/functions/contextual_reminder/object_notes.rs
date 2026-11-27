use super::evidence::{in_resolution, step_operation, target_spec};
use serde_json::Value;

pub(super) fn mentions(ability: &Value, mut visit: impl FnMut(&'static str) -> bool) -> bool {
    let args = &ability["args"];
    if ability["kind"] == "Spell"
        && let Some(epic) = epic_announcement(args)
        && visit(epic)
    {
        return true;
    }
    if ability["kind"] == "Static"
        && args["affected"]["Objects"].is_object()
        && makes_creature(&args["changes"], false)
        && visit("SummoningSickness")
    {
        return true;
    }
    in_resolution(&args["body"], args, 0, &mut 512, &mut |step, frame, _| {
        let Some(op) = step_operation(step) else {
            return false;
        };
        match op["kind"].as_str() {
            Some("Attach") => {
                attaches_equipment(&op["args"]["attachments"], &frame["announcement"])
                    && visit("EquipmentKeepsController")
            }
            Some("Keyword") => {
                phases_out_for_controller(&op["args"], frame) && visit("PhaseOutUntilYourTurn")
            }
            Some("CreateTokens") => {
                creature_tokens_without_haste(&op["args"]["tokens"]) && visit("SummoningSickness")
            }
            _ => false,
        }
    })
}

fn epic_announcement(args: &Value) -> Option<&'static str> {
    let step = args["body"]["instructions"].as_array()?.last()?;
    let operation = step_operation(step)?;
    if operation["kind"] != "Keyword" || operation["args"]["action"] != "Epic" {
        return None;
    }
    let targets = args["announcement"]["targets"].as_array()?;
    Some(if targets.is_empty() {
        "EpicWithoutTargets"
    } else {
        "EpicWithTargets"
    })
}

fn filter_proves(
    filter: &Value,
    leaf: &impl Fn(&Value) -> bool,
    depth: usize,
    remaining: &mut usize,
) -> bool {
    if depth == 8 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    let terms = filter["args"]
        .as_array()
        .filter(|items| !items.is_empty() && items.len() <= 8);
    let meets = |term: &Value| filter_proves(term, leaf, depth + 1, remaining);
    match filter["kind"].as_str() {
        Some("AllOf") => terms.is_some_and(|terms| terms.iter().any(meets)),
        Some("AnyOf") => terms.is_some_and(|terms| terms.iter().all(meets)),
        _ => leaf(filter),
    }
}

fn attaches_equipment(attachments: &Value, announcement: &Value) -> bool {
    let args = &attachments["args"];
    let filter = match attachments["kind"].as_str() {
        Some("Zone") => &args["filter"],
        Some("One" | "Targets") => {
            let object = if attachments["kind"] == "One" {
                args
            } else {
                attachments
            };
            let Some(target) = target_spec(object, announcement) else {
                return false;
            };
            &target["filter"]
        }
        _ => return false,
    };
    filter_proves(
        filter,
        &|term| term["kind"] == "Subtype" && term["args"] == "Equipment",
        0,
        &mut 256,
    )
}

fn is_controller(players: &Value) -> bool {
    players["kind"] == "One" && players["args"]["kind"] == "AbilityController"
}

fn phases_out_for_controller(args: &Value, frame: &Value) -> bool {
    let phase = &args["action"]["PhaseOut"];
    if !phase.is_object() || !phase["until"].is_null() || !is_controller(&args["actor"]) {
        return false;
    }
    let object = &phase["object"];
    let controlled = |term: &Value| {
        term["kind"] == "SameObject" && term["args"]["kind"] == "Source"
            || term["kind"] == "ControlledBy" && is_controller(&term["args"])
    };
    match object["kind"].as_str() {
        Some("Source") => true,
        Some("EventSubject") => {
            let event = &frame["event"];
            ["BecomesTarget", "Object"]
                .iter()
                .any(|kind| filter_proves(&event[*kind]["objects"], &controlled, 0, &mut 256))
        }
        Some("Target") => target_spec(object, &frame["announcement"])
            .is_some_and(|target| filter_proves(&target["filter"], &controlled, 0, &mut 256)),
        _ => false,
    }
}

fn makes_creature(changes: &Value, copy: bool) -> bool {
    let Some(items) = changes.as_array().filter(|items| items.len() <= 32) else {
        return false;
    };
    let adds_creature = items.iter().any(|change| {
        if copy {
            change["kind"] == "AddCardTypes" && has_creature(&change["args"])
        } else {
            change["kind"] == "AddCardType" && change["args"] == "Creature"
        }
    });
    adds_creature
        && !items.iter().any(|change| {
            change["kind"] == "GrantKeyword" && change["args"]["kind"] == "Haste"
                || change["kind"] == "AddAbilities" && has_haste(&change["args"])
        })
}

fn creature_tokens_without_haste(tokens: &Value) -> bool {
    tokens.as_array().is_some_and(|tokens| {
        !tokens.is_empty()
            && tokens.len() <= 16
            && tokens.iter().all(|token| {
                let recipe = &token["recipe"];
                let defined = &recipe["Defined"];
                if defined.is_object() {
                    return has_creature(&defined["card_types"])
                        && defined["abilities_of"].is_null()
                        && !has_haste(&defined["abilities"]);
                }
                makes_creature(&recipe["Copy"]["exceptions"], true)
            })
    })
}

fn has_creature(types: &Value) -> bool {
    types
        .as_array()
        .is_some_and(|types| types.len() <= 16 && types.iter().any(|ty| ty == "Creature"))
}

fn has_haste(abilities: &Value) -> bool {
    abilities.as_array().is_some_and(|abilities| {
        abilities.len() > 32
            || abilities.iter().any(|ability| {
                ability["kind"] == "Keyword"
                    && (ability["args"]["kind"] == "Haste" || ability["args"] == "Haste")
            })
    })
}
