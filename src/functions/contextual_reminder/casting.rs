use serde_json::{Value, json};

pub(super) fn permission_notes(value: &Value, mut visit: impl FnMut(&'static str) -> bool) -> bool {
    let args = &value["args"];
    let kind = value["kind"].as_str();
    if matches!(kind, Some("PlayCards" | "PlayWith")) {
        let actor = &args["actor"];
        if actor["kind"] != "One" || actor["args"]["kind"] != "AbilityController" {
            return false;
        }
        // Casting during a resolving ability can override ordinary timing.
        let cost = if kind == Some("PlayCards") {
            &args["cost"]
        } else {
            &args["terms"]["cost"]
        };
        return match cost.as_str() {
            Some("Normal") => visit("PayCosts"),
            Some("Free") => visit("PayAdditionalCosts"),
            _ => false,
        };
    }
    if !matches!(kind, Some("Static" | "CreateContinuousEffect")) {
        return false;
    }
    let players = &args["affected"]["Players"];
    if players["kind"] != "One" || players["args"]["kind"] != "AbilityController" {
        return false;
    }
    args["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .take(32)
        .filter_map(play_rule_action)
        .any(|action| {
            action == "Either" && visit("LandPlayNormalTiming")
                || matches!(action, "Either" | "CastSpell")
                    && (visit("PayCostsAndTiming") || visit("PayCosts"))
        })
}

fn play_rule_action(change: &Value) -> Option<&str> {
    if change["kind"] != "PlayerRule" {
        return None;
    }
    let rule = &change["args"];
    let permission = if let Some(extended) = rule.get("PlayPermissionWith") {
        let terms = &extended["terms"];
        let cost = &terms["cost"];
        if !(cost == "Normal" || cost.get("Additional").is_some()) || terms["flash"] != false {
            return None;
        }
        &extended["permission"]["PlayPermission"]
    } else {
        &rule["PlayPermission"]
    };
    permission["action"].as_str()
}

/// The two effects each have their own required color of mana spent on this
/// spell. The explanation says both effects apply if both colors were spent.
pub(super) fn spent_pair_claims(ability: &Value) -> Vec<Value> {
    if ability["kind"] != "Spell" || !ability["args"]["announcement"]["modes"].is_null() {
        return Vec::new();
    }
    let Some([first, second]) = ability["args"]["body"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return Vec::new();
    };
    let (Some(first), Some(second)) = (spent_color(first), spent_color(second)) else {
        return Vec::new();
    };
    if first == second {
        return Vec::new();
    }
    [(first, second), (second, first)]
        .into_iter()
        .map(|(a, b)| json!({"kind":"Cost","args":{"parts":[{"Mana":[{"Color":a},{"Color":b}]}]}}))
        .collect()
}

fn spent_color(instruction: &Value) -> Option<&str> {
    // These placement markers preserve an If's scope and are consumed by binding.
    let instruction = match instruction["$sym"].as_str() {
        Some("leading_if" | "resolution") => {
            let [inner] = instruction["args"].as_array()?.as_slice() else {
                return None;
            };
            inner
        }
        _ => instruction,
    };
    if instruction["kind"] != "If" {
        return None;
    }
    let args = &instruction["args"];
    if !args["then"]["instructions"]
        .as_array()
        .is_some_and(|steps| !steps.is_empty())
        || !args["otherwise"]["instructions"]
            .as_array()
            .is_some_and(Vec::is_empty)
    {
        return None;
    }
    let condition = &args["condition"];
    let spent = &condition["args"]["left"];
    let test = &condition["args"]["test"];
    let source = &spent["args"]["object"];
    let filter = &spent["args"]["filter"];
    if condition["kind"] != "Compare"
        || spent["kind"] != "ManaSpent"
        || !(source["kind"] == "Source" || source["$sym"] == "self")
        || spent["args"]["aggregation"] != "Count"
        || filter["snow"].as_bool() != Some(false)
        || test["kind"] != "Compare"
        || test["args"]["comparison"] != "GreaterOrEqual"
        || test["args"]["value"]["kind"] != "Constant"
        || test["args"]["value"]["args"] != 1
    {
        return None;
    }
    let [color] = filter["colors"].as_array()?.as_slice() else {
        return None;
    };
    color.as_str()
}
