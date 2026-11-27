use super::evidence::{effects, unscoped};
use serde_json::Value;

pub(super) fn mention(value: &Value) -> Option<&'static str> {
    let args = &value["args"];
    match value["kind"].as_str()? {
        "Landwalk" if varying_land_type(args) => Some("VariableLandwalk"),
        "CreatureTypes"
            if args["mode"] == "Set"
                && args["types"]["Listed"]
                    .as_array()
                    .is_some_and(|types| !types.is_empty()) =>
        {
            Some("ReplaceCreatureTypes")
        }
        "ChooseValue" if args["kind"] == "CardType" => Some("CardTypes"),
        "Protection"
            if args["Objects"]["filter"]["kind"] == "Shares"
                && args["Objects"]["filter"]["args"]["property"] == "CardTypes" =>
        {
            Some("CardTypes")
        }
        _ => None,
    }
}

fn varying_land_type(filter: &Value) -> bool {
    (filter["kind"] == "Chosen" && filter["args"] == "LandType")
        || (filter["kind"] == "SharesWithAny" && filter["args"]["property"] == "BasicLandTypes")
}

pub(super) fn root_mentions(ability: &Value, mut visit: impl FnMut(&'static str) -> bool) -> bool {
    let args = &ability["args"];
    let event = &args["event"];
    let adventure_filter = match ability["kind"].as_str() {
        Some("Triggered") if event["Spell"]["event"] == "Cast" => &event["Spell"]["spells"],
        Some("Replacement") => &event["Enter"]["objects"],
        _ => &Value::Null,
    };
    adventure_creatures(adventure_filter) && visit("HasAdventure")
        || matches!(ability["kind"].as_str(), Some("Static" | "Spell"))
            && controller_shroud(ability)
            && visit("PlayerShroud")
        || ability["kind"] == "Spell" && colors_only(ability) && visit("RetainManaSymbols")
        || matches!(ability["kind"].as_str(), Some("Spell" | "Activated"))
            && effects(&args["body"]).any(|op| match op["kind"].as_str() {
                Some("Keyword") => op["args"]["action"].get("BecomeBlocked").is_some(),
                Some("KeywordEach") => op["args"]["action"] == "BecomeBlocked",
                _ => false,
            })
            && visit(if ability["kind"] == "Spell" {
                "SpellBecomesBlocked"
            } else {
                "AbilityBecomesBlocked"
            })
}

fn adventure_creatures(filter: &Value) -> bool {
    let Some(terms) = filter["args"].as_array() else {
        return false;
    };
    filter["kind"] == "AllOf"
        && matches!(terms.len(), 2 | 3)
        && terms
            .iter()
            .any(|t| t["kind"] == "State" && t["args"] == "HasAdventure")
        && terms
            .iter()
            .any(|t| t["kind"] == "CardType" && t["args"] == "Creature")
        && terms.iter().all(|t| {
            matches!(
                t["kind"].as_str(),
                Some("State" | "CardType" | "ControlledBy")
            )
        })
        && terms
            .iter()
            .all(|t| t["kind"] != "State" || t["args"] == "HasAdventure")
}

fn controller_shroud(ability: &Value) -> bool {
    let shroud = |args: &Value| {
        let players = &args["affected"]["Players"];
        players["kind"] == "One"
            && players["args"]["kind"] == "AbilityController"
            && args["changes"].as_array().is_some_and(|changes| {
                changes.iter().take(32).any(|change| {
                    change["kind"] == "PlayerRule" && change["args"]["GrantKeyword"] == "Shroud"
                })
            })
    };
    match ability["kind"].as_str() {
        Some("Static") => shroud(&ability["args"]),
        Some("Spell") => effects(&ability["args"]["body"])
            .any(|op| op["kind"] == "CreateContinuousEffect" && shroud(&op["args"])),
        _ => false,
    }
}

fn colors_only(ability: &Value) -> bool {
    let args = &ability["args"];
    let Some([first, second]) = args["announcement"]["modes"]["options"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return false;
    };
    let same_operation = |option: &Value, operation: &str| {
        let option = unscoped(option);
        let Some([step]) = option["body"]["instructions"].as_array().map(Vec::as_slice) else {
            return false;
        };
        let op = &step["args"]["operation"];
        step["kind"] == "Effect"
            && op["kind"] == operation
            && matches!(op["args"]["changes"].as_array().map(Vec::as_slice),
                Some([change]) if change["kind"] == "Colors" && change["args"]["mode"] == "Set")
    };
    args["body"]["instructions"]
        .as_array()
        .is_none_or(Vec::is_empty)
        && (same_operation(first, "ChangeSpell")
            && same_operation(second, "CreateContinuousEffect")
            || same_operation(second, "ChangeSpell")
                && same_operation(first, "CreateContinuousEffect"))
}
