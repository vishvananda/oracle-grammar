use serde_json::{Value, json};

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
