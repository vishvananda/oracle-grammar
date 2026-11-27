//! Compare the written characteristics or quoted ability with the last creation
//! in the same resolution. The description never adds an ability to the token.
use super::evidence::{step_operation, unscoped};
use serde_json::{Value, json};

fn last_creation(ability: &Value, follow_attachment: bool) -> Option<(&Value, &Value)> {
    let ability = unscoped(ability);
    let args = &ability["args"];
    let args = match ability["kind"].as_str()? {
        "Saga" => &unscoped(
            args["chapters"]
                .as_array()
                .filter(|items| items.len() <= 12)?
                .last()?,
        )["args"],
        "Static" => {
            let [change] = args["changes"].as_array()?.as_slice() else {
                return None;
            };
            if change["kind"] != "GrantAbility" {
                return None;
            }
            &unscoped(&change["args"])["args"]
        }
        _ => args,
    };
    let mut steps = &args["body"]["instructions"];
    for _ in 0..12 {
        let instructions = steps.as_array().filter(|steps| steps.len() <= 64)?;
        let last = instructions.last()?;
        if last["kind"] == "Group" {
            steps = &last["args"]["instructions"];
            continue;
        }
        let step = match instructions.as_slice() {
            [.., creation, follow_up]
                if goads_created_tokens(creation, follow_up)
                    || follow_attachment && attaches_to_created_token(creation, follow_up) =>
            {
                creation
            }
            _ => last,
        };
        let operation = step_operation(step)?;
        let operation = if operation["$sym"] == "created_result" {
            &operation["args"][0]
        } else {
            operation
        };
        if operation["kind"] != "CreateTokens" {
            return None;
        }
        let [spec] = operation["args"]["tokens"].as_array()?.as_slice() else {
            return None;
        };
        return Some((&operation["args"], spec));
    }
    None
}

fn attaches_to_created_token(creation: &Value, follow_up: &Value) -> bool {
    let result = &creation["args"]["result"];
    let args = &follow_up["args"];
    if !result.is_u64()
        || follow_up["kind"] != "Optional"
        || args["decider"]["kind"] != "AbilityController"
    {
        return false;
    }
    let Some([select, attach]) = args["body"]["instructions"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let (Some(choice), Some(attachment)) = (step_operation(select), step_operation(attach)) else {
        return false;
    };
    let objects = &choice["args"]["objects"];
    let host = &attachment["args"]["host"];
    choice["kind"] == "ChooseObjects"
        && objects["kind"] == "Created"
        && &objects["args"] == result
        && attachment["kind"] == "Attach"
        && host["kind"] == "Result"
        && host["args"]["result"] == select["args"]["result"]
        && host["args"]["phase"] == "After"
        && host["args"]["index"] == 0
}

fn goads_created_tokens(creation: &Value, follow_up: &Value) -> bool {
    let Some(operation) = step_operation(follow_up) else {
        return false;
    };
    let args = &operation["args"];
    let objects = &args["objects"];
    operation["kind"] == "KeywordEach"
        && args["action"]["Goad"]["until"] == "Indefinite"
        && objects["kind"] == "Created"
        && objects["args"].is_u64()
        && objects["args"] == creation["args"]["result"]
}

fn descriptions(ability: &Value, plural: bool) -> Vec<Value> {
    let Some((_, spec)) = last_creation(ability, false) else {
        return Vec::new();
    };
    let amount = &spec["amount"];
    if plural == (amount["kind"] == "Constant" && amount["args"] == 1) {
        return Vec::new();
    }
    let recipe = &spec["recipe"];
    if recipe["Named"].is_string() {
        return vec![recipe.clone()];
    }
    let definition = &recipe["Defined"];
    if !definition.is_object() || super::super::branch_bounds(definition).is_err() {
        return Vec::new();
    }
    let mut shape = json!({
        "name":"FromSubtypes","card_types":definition["card_types"],"subtypes":[],
        "colors":[],"chosen_color":false,"chosen_creature_type":false,
        "supertypes":[],"power_toughness":null,"abilities":[],"abilities_of":null,
    });
    let mut choices = Vec::new();
    if matches!(definition["abilities"].as_array(), Some(abilities) if abilities.len() == 1)
        && definition["abilities_of"].is_null()
    {
        let mut quoted = shape.clone();
        quoted["abilities"] = definition["abilities"].clone();
        choices.push(json!({"Defined":quoted}));
    }
    if !definition["power_toughness"].is_null()
        && definition["chosen_color"] == false
        && definition["chosen_creature_type"] == false
    {
        shape["power_toughness"] = definition["power_toughness"].clone();
        shape["colors"] = definition["colors"].clone();
        shape["subtypes"] = definition["subtypes"].clone();
        choices.push(json!({"Defined":shape}));
    }
    choices
}

pub(super) fn prowess_notes(ability: &Value, mut visit: impl FnMut(&'static str) -> bool) -> bool {
    let Some((creation, spec)) = last_creation(ability, true) else {
        return false;
    };
    let entry_controller = &creation["entry"]["controller"];
    if !(entry_controller["Player"]["kind"] == "AbilityController"
        || entry_controller.is_null() && super::object_notes::is_controller(&creation["players"]))
    {
        return false;
    }
    let definition = &spec["recipe"]["Defined"];
    let [keyword] = definition["abilities"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    else {
        return false;
    };
    if keyword["kind"] != "Keyword" || keyword["args"]["kind"] != "Prowess" {
        return false;
    }
    let amount = &spec["amount"];
    visit("TokenProwess")
        || !(amount["kind"] == "Constant" && amount["args"] == 1) && visit("GroupTokenProwess")
}

pub(super) fn print_descriptions(ability: &Value, plural: bool) -> Vec<Value> {
    let mut choices = descriptions(ability, plural);
    if let Some(quoted) = choices.first()
        && let Some([inner]) = quoted["Defined"]["abilities"].as_array().map(Vec::as_slice)
    {
        // The creation may need a bound recipe to print its name while the
        // quotation needs free target slots to print the same ability.
        let unbound = crate::bind::unbind_ability(unscoped(inner));
        let variants: Vec<_> = unbound
            .into_iter()
            .filter(|surface| surface != inner)
            .take(8)
            .map(|surface| {
                let mut description = quoted.clone();
                description["Defined"]["abilities"][0] = surface;
                description
            })
            .collect();
        choices.extend(variants);
    }
    choices
}

pub(super) fn agrees(ability: &Value, written: &Value, plural: bool) -> bool {
    if super::super::branch_bounds(written).is_err() {
        return false;
    }
    let choices = descriptions(ability, plural);
    if written["Named"].is_string() {
        return choices.contains(written);
    }
    let Ok(expected) = quoted_meaning(&written["Defined"]["abilities"]) else {
        return false;
    };
    let mut written = written.clone();
    written["Defined"]["abilities"] = expected;
    choices.into_iter().any(|mut choice| {
        let Ok(abilities) = crate::engine::semantic_abilities(&choice["Defined"]["abilities"])
        else {
            return false;
        };
        choice["Defined"]["abilities"] = abilities;
        choice == written
    })
}

fn quoted_meaning(abilities: &Value) -> Result<Value, String> {
    match abilities.as_array().map(Vec::as_slice) {
        Some([]) => Ok(json!([])),
        Some([printed]) => {
            let mut ability = if printed["$sym"] == "holder" {
                printed["args"][0].clone()
            } else {
                unscoped(printed).clone()
            };
            // The supported descriptions quote one intrinsic ability. It has
            // its own target announcement and no inherited result references.
            crate::bind::bind_ability(&mut ability, "")?;
            crate::engine::semantic_abilities(&json!([ability]))
        }
        _ => Err("expected one quoted token ability".into()),
    }
}
