use super::evidence::{effects, unscoped};
use super::object_notes::{filter_proves, is_controller};
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
        "Triggered" => counters_on_damage_recipient(args),
        "PutCounters" => keyword_counter_reminder(args),
        "TowardPlayer"
            if matches!(
                args["action"].as_str(),
                Some("ControlNextTurn" | "ControlNextCombat")
            ) && is_controller(&args["actor"]) =>
        {
            Some("PlayerControlDecisions")
        }
        "PlayerRule" if args["AdditionalVote"]["optional"].is_boolean() => {
            Some("AdditionalVoteChoices")
        }
        "Protection"
            if args["Objects"]["kind"] == "Any"
                && args["Objects"]["filter"]["kind"] == "Chosen"
                && args["Objects"]["filter"]["args"] == "Color" =>
        {
            Some("ProtectionChosenColor")
        }
        "Protection"
            if args["Players"]["kind"] == "OpponentsOf"
                && args["Players"]["args"]["kind"] == "AbilityController" =>
        {
            Some("ProtectionOpponents")
        }
        "Protection"
            if args["Objects"]["filter"]["kind"] == "Shares"
                && args["Objects"]["filter"]["args"]["property"] == "CardTypes" =>
        {
            Some("CardTypes")
        }
        _ => None,
    }
}

fn keyword_counter_reminder(operation: &Value) -> Option<&'static str> {
    let [placement] = operation["placements"].as_array()?.as_slice() else {
        return None;
    };
    let recipients = &placement["recipients"];
    if recipients["kind"] != "Objects" {
        return None;
    }
    match placement["kind"]["Named"].as_str()? {
        "ShadowCounter" => Some("Shadow"),
        "MenaceCounter"
            if recipients["args"]["kind"] == "One"
                || placement["selection"]["count"]["AsManyAsCan"]
                    == serde_json::json!({"kind":"Constant","args":1}) =>
        {
            Some("Menace")
        }
        _ => None,
    }
}

fn counters_on_damage_recipient(trigger: &Value) -> Option<&'static str> {
    let damage = &trigger["event"]["Damage"];
    let recipients = &damage["recipients"]["Objects"];
    if damage["grouping"] != "Recipient"
        || damage["subject"] != "Recipient"
        || !recipients.is_object()
    {
        return None;
    }
    let mut operations = effects(&trigger["body"]);
    let counter = operations.next()?;
    let Some([placement]) = counter["args"]["placements"].as_array().map(Vec::as_slice) else {
        return None;
    };
    let objects = &placement["recipients"]["args"];
    let referent = &objects["args"];
    let same_recipient = counter["kind"] == "PutCounters"
        && operations.next().is_none()
        && placement["recipients"]["kind"] == "Objects"
        && objects["kind"] == "One"
        && (referent["kind"] == "EventSubject"
            || referent["$sym"] == "it"
            || referent["$sym"] == "member_count_it" && referent["args"][0]["$sym"] == "it"
            || referent["kind"] == "Source"
                && recipients["kind"] == "SameObject"
                && recipients["args"]["kind"] == "Source");
    if !same_recipient {
        return None;
    }
    let amount = &placement["amount"];
    if amount["$sym"] == "that_much" {
        return Some("DamageCountersSurvival");
    }
    match amount["kind"].as_str()? {
        "Constant" if amount["args"] == 1 => Some("DamageCounterSurvival"),
        "Constant" if amount["args"].as_u64().is_some_and(|n| n > 1) => {
            Some("DamageCountersSurvival")
        }
        "EventAmount" => Some("DamageCountersSurvival"),
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
        || ability["kind"] == "Backup"
            && args["abilities"]
                .as_array()
                .is_some_and(|abilities| match abilities.len() {
                    1 => visit("BackupSingle"),
                    2..=64 => visit("BackupMany"),
                    _ => false,
                })
        || ability["kind"] == "Static" && granted_mechanic_notes(args, &mut visit)
        || ability["kind"] == "Triggered"
            && matches!(
                event["Player"]["event"].as_str(),
                Some("Clashes" | "WinsClash")
            )
            && visit("ClashFinished")
        || source_cycling_trigger(ability) && visit("CyclingBeforeDraw")
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

fn granted_mechanic_notes(args: &Value, visit: &mut impl FnMut(&'static str) -> bool) -> bool {
    let objects = &args["affected"]["Objects"];
    if objects["kind"] != "Zone" {
        return false;
    }
    let zone = &objects["args"];
    let Some(changes) = args["changes"].as_array() else {
        return false;
    };
    changes.iter().take(32).any(|change| {
        let ability = unscoped(&change["args"]);
        if change["kind"] != "GrantAbility" || ability["kind"] != "Mechanic" {
            return false;
        }
        let mechanic = &ability["args"];
        match mechanic["mechanic"].as_str() {
            Some("Affinity") => {
                zone["scope"]["Shared"] == "Stack"
                    && zone["kind"] == "Spell"
                    && card_type(&mechanic["filter"], "Artifact")
                    && filter_proves(&zone["filter"], &cast_by_controller, 0, &mut 256)
                    && visit("GrantedArtifactAffinity")
            }
            Some("Scavenge") => {
                let graveyard = &zone["scope"]["Players"];
                graveyard["zone"] == "Graveyard"
                    && is_controller(&graveyard["players"])
                    && zone["kind"] == "Card"
                    && card_type(&zone["filter"], "Creature")
                    && matches!(mechanic["cost"]["parts"].as_array().map(Vec::as_slice),
                        Some([mana]) if mana["ManaCostOf"]["kind"] == "Source")
                    && visit("GrantedScavenge")
            }
            _ => false,
        }
    })
}

fn card_type(filter: &Value, name: &str) -> bool {
    filter["kind"] == "CardType" && filter["args"] == name
}

fn cast_by_controller(filter: &Value) -> bool {
    filter["kind"] == "Cast" && is_controller(&filter["args"]["by"])
}

fn source_cycling_trigger(ability: &Value) -> bool {
    let scoped = unscoped(ability);
    let trigger = unscoped(&scoped["args"]["ability"]);
    let discard = &trigger["args"]["event"]["Discard"];
    scoped["kind"] == "Scoped"
        && trigger["kind"] == "Triggered"
        && discard["kind"] == "Cycling"
        && discard["source_only"] == true
        && discard["players"]["kind"] == "One"
        && discard["players"]["args"]["kind"] == "AbilityController"
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
