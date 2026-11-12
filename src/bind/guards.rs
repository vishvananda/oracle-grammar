//! Semantic refusals independent of presentation and reference assignment.
use super::{ability_kind, symbol_name};
use serde_json::{Value, json};

/// Rule 603.4: ordinary and reflexive `When X, if Y, Z` check Y as part
/// of the trigger. An `otherwise`
/// makes it an instruction, since an intervening condition has no other branch.
fn opens_with_if(ability: &Value, literal: bool) -> bool {
    let args = &ability["args"];
    let first = &args["body"]["instructions"][0];
    let leading = symbol_name(first) == Some("leading_if");
    let first = if leading { &first["args"][0] } else { first };
    let condition = match ability_kind(ability) {
        Some("Triggered") => &args["intervening_if"],
        Some("ReflexiveTrigger") => &args["condition"],
        _ => return false,
    };
    condition.is_null()
        && (leading || literal)
        && first["kind"] == "If"
        && first["args"]["otherwise"]["instructions"]
            .as_array()
            .is_none_or(Vec::is_empty)
}

/// The inverse cannot put this body condition directly after a trigger. Target
/// choices must already be restored: a written choice before `if` permits the
/// ordinary leading form, even though that choice has no executable instruction.
pub(super) fn resolution_surface(ability: &mut Value) {
    if opens_with_if(ability, true) {
        let first = &mut ability["args"]["body"]["instructions"][0];
        if first["kind"] == "If" {
            *first = json!({"$sym":"resolution", "args":[first.take()], "at":0});
        }
    }
}

/// Source placement belongs to the production, not the shape of its IR. Strip
/// these transparent markers before references/results are bound: they do not
/// introduce scopes or affect the order of anaphors and announcements.
pub(super) fn condition_placement(value: &mut Value) -> Result<(), String> {
    fn walk(value: &mut Value, literal: bool) -> Result<(), String> {
        super::loop_card_focus::mark(value);
        if opens_with_if(value, literal) {
            return Err("an `if` right after a trigger is its intervening condition".into());
        }
        if matches!(symbol_name(value), Some("resolution" | "leading_if")) {
            let args = value["args"]
                .as_array_mut()
                .ok_or("condition placement needs an instruction")?;
            let [inner] = args.as_mut_slice() else {
                return Err("condition placement needs one instruction".into());
            };
            if inner["kind"] != "If" {
                return Err("condition placement needs an If instruction".into());
            }
            *value = inner.take();
        }
        match value {
            Value::Object(fields) => fields.values_mut().try_for_each(|v| walk(v, false)),
            Value::Array(items) => items.iter_mut().try_for_each(|v| walk(v, false)),
            _ => Ok(()),
        }
    }
    // Preserve the existing conservative refusal of unmarked literal top-level
    // templates. Nested literals retain their previous contract; newly composed
    // leading-if clauses carry explicit evidence even inside granted abilities.
    walk(value, true)
}

/// Rule 614.1a: "you may <action> instead" replaces the event only when the
/// action is chosen. A replacement that cancels its event before an optional
/// action would lose the event even when the action is declined.
pub(super) fn cancels_before_choice(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            let cancel = json!({"kind": "ModifyEvent", "args": {"change": "Cancel"}});
            let lost = value["kind"] == "Replacement"
                && value["args"]["body"]["instructions"]
                    .as_array()
                    .is_some_and(|items| {
                        items.windows(2).any(|pair| {
                            pair[0]["kind"] == "Effect"
                                && pair[0]["args"]["operation"] == cancel
                                && pair[1]["kind"] == "Optional"
                        })
                    });
            lost || object.values().any(cancels_before_choice)
        }
        Value::Array(items) => items.iter().any(cancels_before_choice),
        _ => false,
    }
}

/// A printed static ability on one fixed object names that object in its pump
/// amounts; `Affected` is the per-object form for a population. Token and
/// granted statics are not held to this form.
pub(super) fn single_holder_affected(ability: &Value) -> bool {
    let args = &ability["args"];
    let affected = json!({"kind": "Affected"});
    ability["kind"] == "Static"
        && args["affected"]["Objects"]["kind"] == "One"
        && args["changes"].as_array().is_some_and(|changes| {
            changes
                .iter()
                .any(|change| change["kind"] == "Pump" && mentions(change, &affected))
        })
}

fn mentions(value: &Value, wanted: &Value) -> bool {
    value == wanted
        || match value {
            Value::Object(fields) => fields.values().any(|v| mentions(v, wanted)),
            Value::Array(items) => items.iter().any(|v| mentions(v, wanted)),
            _ => false,
        }
}

/// A creature cannot be required to block itself (CR 509.1a), so the object
/// a block requirement names is never the object the requirement affects.
pub(super) fn blocks_itself(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            let args = &value["args"];
            let affected = &args["affected"]["Objects"];
            let own = value["kind"] == "CreateContinuousEffect"
                && affected["kind"] == "One"
                && args["changes"].as_array().is_some_and(|changes| {
                    changes.iter().any(|change| {
                        change["args"]["RequireCombat"]["action"]["Block"]
                            == json!({"kind": "SameObject", "args": affected["args"]})
                    })
                });
            own || object.values().any(blocks_itself)
        }
        Value::Array(items) => items.iter().any(blocks_itself),
        _ => false,
    }
}

/// An activation restriction adds the margin to the starting life total; any
/// other life-margin condition adds the starting life total to the margin.
pub(super) fn life_margin_order(value: &Value, activation: bool) -> bool {
    match value {
        Value::Object(object) => {
            let parts = value["args"].as_array().filter(|_| value["kind"] == "Add");
            let wrong = parts.is_some_and(|parts| {
                let starting = |part: &Value| part["kind"] == "StartingLife";
                parts.len() == 2
                    && starting(&parts[0]) != starting(&parts[1])
                    && starting(&parts[1]) == activation
            });
            if wrong {
                return true;
            }
            if value["kind"] == "Activated" {
                return object.iter().any(|(key, child)| {
                    if key == "args" {
                        child.as_object().is_some_and(|args| {
                            args.iter()
                                .any(|(field, inner)| life_margin_order(inner, field == "limits"))
                        })
                    } else {
                        life_margin_order(child, activation)
                    }
                });
            }
            object
                .values()
                .any(|child| life_margin_order(child, activation))
        }
        Value::Array(items) => items.iter().any(|item| life_margin_order(item, activation)),
        _ => false,
    }
}

/// Protection from objects described by a controller or counters is protection
/// from permanents; any other described quality names objects in every zone.
pub(super) fn protection_domain_mismatch(value: &Value) -> bool {
    fn top(filter: &Value) -> Vec<&Value> {
        match filter["kind"].as_str() {
            Some("AllOf") => filter["args"]
                .as_array()
                .map(|parts| parts.iter().collect()),
            _ => None,
        }
        .unwrap_or_else(|| vec![filter])
    }
    match value {
        Value::Object(object) => {
            let objects = &value["args"]["Objects"];
            let wrong = value["kind"] == "Protection" && {
                let parts = top(&objects["filter"]);
                let has = |kinds: &[&str]| {
                    parts.iter().any(|part| {
                        part["kind"]
                            .as_str()
                            .is_some_and(|kind| kinds.contains(&kind))
                    })
                };
                match objects["kind"].as_str() {
                    Some("Any") => has(&["ControlledBy", "Counters"]),
                    Some("Permanent") => !has(&["ControlledBy", "Counters", "State", "Condition"]),
                    _ => false,
                }
            };
            wrong || object.values().any(protection_domain_mismatch)
        }
        Value::Array(items) => items.iter().any(protection_domain_mismatch),
        _ => false,
    }
}

/// A simple self-payment uses its native cost part. A different player's action
/// remains PlayerAction: replacing it with Draw/Discard would change who acts.
/// References must be bound before comparing the deciding and performing players.
pub(super) fn has_cost_as_action(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            let redundant = value.pointer("/args/cost").is_some_and(|cost| {
                let controller = json!({"kind":"AbilityController"});
                let payer = value.pointer("/args/player").unwrap_or(&controller);
                redundant_action(cost, payer)
            });
            redundant || drawn_compound_completion(value) || object.values().any(has_cost_as_action)
        }
        Value::Array(items) => items.iter().any(has_cost_as_action),
        _ => false,
    }
}

fn redundant_action(cost: &Value, payer: &Value) -> bool {
    if let Some(action) = cost.get("PlayerAction") {
        return action["player"] == *payer && cost_like(action);
    }
    match cost {
        Value::Object(fields) => fields.values().any(|v| redundant_action(v, payer)),
        Value::Array(items) => items.iter().any(|v| redundant_action(v, payer)),
        _ => false,
    }
}

fn cost_like(action: &Value) -> bool {
    if let Some(items) = action["body"]["instructions"].as_array()
        && let [discard, draw] = items.as_slice()
        && discard["kind"] == "Effect"
        && discard["args"]["operation"]["kind"] == "Discard"
        && draw["kind"] == "Effect"
        && draw["args"]["operation"]["kind"] == "Draw"
        && discard["args"]["operation"]["args"]["amount"]
            == json!({"kind":"CountObjects","args":{"kind":"Zone","args":{"scope":{"Players":{"zone":"Hand","players":{"kind":"One","args":action["player"]}}},"kind":"Card","filter":{"kind":"Any"}}}})
        && discard["args"]["operation"]["args"]["players"]
            == json!({"kind":"One","args":action["player"]})
        && draw["args"]["operation"]["args"]["players"]
            == json!({"kind":"One","args":action["player"]})
    {
        return true;
    }
    let [only] = action["body"]["instructions"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
    else {
        return false;
    };
    let operation = &only["args"]["operation"];
    match operation["kind"].as_str() {
        Some("Draw" | "Sacrifice") => true,
        Some("Discard") => operation["args"]["amount"]["kind"] == "Constant",
        Some("SetTapped" | "PutCounters") => true,
        Some("RemoveCounters") => direct_counter_cost(operation, &action["player"]),
        // A resolving move may follow a card into another zone and test whether
        // it moved. It need not satisfy a native movement cost's preconditions.
        _ => false,
    }
}

/// The shared resolving-payment production covers one fixed removal performed
/// by the payer. Other performers, selections and amount policies retain their
/// action contract until a matching native cost production supports them.
fn direct_counter_cost(operation: &Value, payer: &Value) -> bool {
    let Some(removals) = operation["args"]["removals"].as_array() else {
        return false;
    };
    let [removal] = removals.as_slice() else {
        return false;
    };
    payer["kind"] == "AbilityController"
        && removal["player"] == *payer
        && removal["recipients"]["kind"] == "Objects"
        && removal["selection"].is_null()
        && !removal["kind"].is_null()
        && removal["amount"]["Count"]["Exactly"]["kind"] == "Constant"
        && removal["amount"]["Count"]["Exactly"]["args"]
            .as_u64()
            .is_some()
}

/// A `when you do` after the optional whole-hand/draw conjunction observes
/// the compound payment choice. An actual Draw receipt loses replaced draws.
/// Reject this obsolete competing producer; the EBNF payment form supplies
/// both components and the exact chosen-payment receipt instead.
fn drawn_compound_completion(value: &Value) -> bool {
    if value["kind"] != "Optional" {
        return false;
    }
    let mut items = value["args"]["body"]["instructions"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    while let [only] = items {
        if only["kind"] != "Group" {
            break;
        }
        items = only["args"]["instructions"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
    }
    let [discard, draw, trigger, ..] = items else {
        return false;
    };
    let payer = &value["args"]["decider"];
    let players = json!({"kind":"One","args":payer});
    discard["args"]["operation"]["kind"] == "Discard"
        && discard["args"]["operation"]["args"]["players"] == players
        && discard["args"]["operation"]["args"]["amount"]
            == json!({"kind":"CountObjects","args":{"kind":"Zone","args":{"scope":{"Players":{"zone":"Hand","players":players}},"kind":"Card","filter":{"kind":"Any"}}}})
        && draw["args"]["operation"]["kind"] == "Draw"
        && draw["args"]["operation"]["args"]["players"] == players
        && trigger["args"]["operation"]["kind"] == "ReflexiveTrigger"
        && trigger["args"]["operation"]["args"]["performed"] == draw["args"]["result"]
        && trigger["args"]["operation"]["args"]["condition"].is_null()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reflexive(marker: &str, otherwise: Value) -> Value {
        let condition = json!({"kind":"Compare","args":{"left":{"kind":"Constant","args":1},"test":{"kind":"Compare","args":{"comparison":"Equal","value":{"kind":"Constant","args":1}}}}});
        json!({"kind":"Spell","args":{"body":{"instructions":[{
            "kind":"Effect","args":{"operation":{"$sym":"reflexive","args":[{
                "kind":"ReflexiveTrigger","args":{"condition":null,"body":{"instructions":[{
                    "$sym":marker,"args":[{"kind":"If","args":{
                        "condition":condition,"then":{"instructions":[]},"otherwise":otherwise
                    }}]
                }]}}
            }]}}
        }]}}})
    }

    #[test]
    fn reflexive_leading_if_requires_a_native_intervening_condition() {
        let mut value = reflexive("leading_if", json!({"instructions":[]}));
        assert!(condition_placement(&mut value).is_err());
    }

    #[test]
    fn reflexive_resolution_and_otherwise_keep_resolution_time_conditions() {
        let mut resolution = reflexive("resolution", json!({"instructions":[]}));
        condition_placement(&mut resolution).unwrap();
        let mut otherwise = reflexive(
            "leading_if",
            json!({"instructions":[{"kind":"Group","args":{"instructions":[]}}]}),
        );
        condition_placement(&mut otherwise).unwrap();
    }
}
