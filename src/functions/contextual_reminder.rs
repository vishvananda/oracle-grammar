//! A closed explanatory sentence needs a matching operand in its own ability.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};
use std::sync::LazyLock;

mod cascade_notes;
mod casting;
mod casting_help;
mod evidence;
mod mechanic_notes;
mod object_notes;
mod plot;
mod receipts;
mod suspend;
mod token_notes;

static PRINTED_X: LazyLock<Value> = LazyLock::new(|| json!({"kind":"AnnouncedX"}));

pub(crate) fn is_symbol(name: &str) -> bool {
    matches!(
        name,
        "contextual_reminder"
            | "measured_reminder"
            | "cycling_reminder"
            | "cycling_pair_reminder"
            | "cycling_grant_reminder"
            | "chapter_reminder"
            | "cost_symbol_reminder"
            | "awaken_reminder"
            | "ward_grant_reminder"
            | "equip_payment_reminder"
            | "unearth_grant_reminder"
            | "spent_pair_reminder"
            | "single_token_description"
            | "plural_token_description"
    )
}

fn historic_filter(value: &Value) -> bool {
    value.as_array().is_some_and(|items| {
        items.len() == 3
            && items
                .iter()
                .any(|i| i["kind"] == "CardType" && i["args"] == "Artifact")
            && items
                .iter()
                .any(|i| i["kind"] == "Supertype" && i["args"] == "Legendary")
            && items
                .iter()
                .any(|i| i["kind"] == "Subtype" && i["args"] == "Saga")
    })
}

fn action_name(action: &Value) -> Option<&str> {
    action.as_str().or_else(|| {
        let object = action.as_object()?;
        (object.len() == 1)
            .then(|| object.keys().next().map(String::as_str))
            .flatten()
    })
}

fn mechanic(value: &Value) -> Option<&'static str> {
    match value.as_str()? {
        "Improvise" => Some("Improvise"),
        "LivingWeapon" => Some("LivingWeapon"),
        "ForMirrodin" => Some("ForMirrodin"),
        "JobSelect" => Some("JobSelect"),
        "Evolve" => Some("Evolve"),
        "Mentor" => Some("Mentor"),
        "Myriad" => Some("Myriad"),
        "SplitSecond" => Some("SplitSecond"),
        "Miracle" => Some("Miracle"),
        "Retrace" => Some("Retrace"),
        "Unleash" => Some("Unleash"),
        "Storm" => Some("Storm"),
        "Gravestorm" => Some("Gravestorm"),
        "Cascade" => Some("Cascade"),
        "Rebound" => Some("Rebound"),
        "Undying" => Some("Undying"),
        "UmbraArmor" => Some("UmbraArmor"),
        "BattleCry" => Some("BattleCry"),
        "Extort" => Some("Extort"),
        "Surge" | "SurgeX" => Some("Surge"),
        "Freerunning" | "FreerunningX" => Some("Freerunning"),
        "Prowl" => Some("Prowl"),
        "Enlist" => Some("Enlist"),
        "LivingMetal" => Some("LivingMetal"),
        "Riot" => Some("Riot"),
        "Melee" => Some("Melee"),
        "Training" => Some("Training"),
        "Ravenous" => Some("Ravenous"),
        "Conspire" => Some("Conspire"),
        "JumpStart" => Some("JumpStart"),
        "Spectacle" => Some("Spectacle"),
        "Phasing" => Some("Phasing"),
        "Sunburst" => Some("Sunburst"),
        "Dethrone" => Some("Dethrone"),
        "Decayed" => Some("Decayed"),
        "Emerge" => Some("Emerge"),
        "Flashback" => Some("Flashback"),
        "Harmonize" => Some("Harmonize"),
        "Banding" => Some("Banding"),
        "Persist" => Some("Persist"),
        "Exalted" => Some("Exalted"),
        "Escape" => Some("Escape"),
        "CumulativeUpkeep" => Some("CumulativeUpkeep"),
        "Demonstrate" => Some("Demonstrate"),
        _ => None,
    }
}

fn rules_token(value: &Value) -> Option<&'static str> {
    let definition = &value["recipe"]["Defined"];
    // Rules words designate the predefined token recipe. Custom tokens and
    // copies can have the same subtype without having that recipe.
    if value["words"] != "Rules"
        || definition["name"] != "FromSubtypes"
        || !matches!(definition["card_types"].as_array().map(Vec::as_slice),
            Some([kind]) if kind == "Artifact")
    {
        return None;
    }
    let [subtype] = definition["subtypes"].as_array()?.as_slice() else {
        return None;
    };
    match subtype.as_str()? {
        "Treasure" => Some("TreasureToken"),
        "Powerstone" => Some("PowerstoneToken"),
        "Clue" => Some("ClueToken"),
        "Food" => Some("FoodToken"),
        "Gold" => Some("GoldToken"),
        "Blood" => Some("BloodToken"),
        "Lander" => Some("LanderToken"),
        "Junk" => Some("JunkToken"),
        "Map" => Some("MapToken"),
        "Mutagen" => Some("MutagenToken"),
        _ => None,
    }
}

fn outlaws(value: &Value) -> bool {
    value.as_array().is_some_and(|items| {
        items.len() == 5
            && ["Assassin", "Mercenary", "Pirate", "Rogue", "Warlock"]
                .iter()
                .all(|subtype| {
                    items
                        .iter()
                        .any(|i| i["kind"] == "Subtype" && i["args"] == *subtype)
                })
    })
}

fn devotion(args: &Value) -> Option<&'static str> {
    let objects = &args["objects"];
    let filter = &objects["args"]["filter"];
    if objects["kind"] != "Zone"
        || objects["args"]["scope"]["Shared"] != "Battlefield"
        || objects["args"]["kind"] != "Permanent"
        || filter["kind"] != "ControlledBy"
        || filter["args"]["kind"] != "One"
        || filter["args"]["args"]["kind"] != "AbilityController"
    {
        return None;
    }
    let [color] = args["colors"].as_array()?.as_slice() else {
        return None;
    };
    match color.as_str()? {
        "White" => Some("DevotionWhite"),
        "Blue" => Some("DevotionBlue"),
        "Black" => Some("DevotionBlack"),
        "Red" => Some("DevotionRed"),
        "Green" => Some("DevotionGreen"),
        "Chosen" => Some("DevotionChosen"),
        _ => None,
    }
}

fn marker(value: &Value) -> Option<&'static str> {
    if value["CopyAtRandom"]["cards"].is_object()
        || value["kind"] == "ChooseOracleDefinitionAtRandom"
    {
        return Some("RandomOracleSearch");
    }
    if value["Keyword"]["action"] == "Forage" {
        return Some("Forage");
    }
    let args = &value["args"];
    match value["kind"].as_str() {
        Some("Modified") if value.as_object()?.len() == 1 => Some("Modified"),
        Some("AnyOf") if historic_filter(args) => Some("Historic"),
        Some("AnyOf") if outlaws(args) => Some("Outlaw"),
        Some("IsPrime") if args.is_object() => Some("PrimeNumbers"),
        Some("AnyOf")
            if let Some(types) = args.as_array()
                && types.len() == 4
                && ["Cleric", "Rogue", "Warrior", "Wizard"].iter().all(|ty| {
                    types
                        .iter()
                        .any(|term| term["kind"] == "Subtype" && term["args"] == *ty)
                }) =>
        {
            Some("PartyCardOrSpell")
        }
        Some("Indestructible") => Some("Indestructible"),
        Some("Menace") => Some("Menace"),
        Some("Shroud") => Some("Shroud"),
        Some("Hexproof") if args.is_null() => Some("Hexproof"),
        Some("Trample") if args.is_null() => Some("Trample"),
        Some("Fear") => Some("Fear"),
        Some("Deathtouch") => Some("Deathtouch"),
        Some("Flying") => Some("Flying"),
        Some("Haste") => Some("Haste"),
        Some("Scry") => Some("Scry"),
        Some("Lifelink") => Some("Lifelink"),
        Some("Prowess") => Some("Prowess"),
        Some("Vigilance") => Some("Vigilance"),
        Some("Reach") => Some("Reach"),
        Some("Vocabulary")
            if args["family"] == "CounterType" && args["form"] == "FinalityCounter" =>
        {
            Some("Finality")
        }
        Some("FirstStrike") => Some("FirstStrike"),
        Some("DoubleStrike") => Some("DoubleStrike"),
        Some("Keyword" | "KeywordEach") => match action_name(&args["action"]) {
            Some("Venture") => Some("Venture"),
            Some("Learn") => Some("Learn"),
            Some("Explore") => Some("Explore"),
            Some("Investigate") => Some("Investigate"),
            Some("Populate") => Some("Populate"),
            Some("Connive") => Some("Connive"),
            Some("Fight") => Some("Fight"),
            Some("OpenAttraction") => Some("OpenAttraction"),
            Some("Cipher") => Some("Cipher"),
            Some("Suspect") => Some("Suspect"),
            Some("Manifest") => Some("Manifest"),
            Some("ManifestDread") => Some("ManifestDread"),
            Some("Proliferate") => Some("Proliferate"),
            Some("Forage") => Some("Forage"),
            Some("Goad") if args["action"]["Goad"]["until"].is_null() => Some("Goad"),
            Some("Exert") => Some("Exert"),
            Some("Airbend") if value["kind"] == "KeywordEach" => Some("AirbendMany"),
            Some("Airbend") => Some("AirbendOne"),
            Some("TimeTravel") => Some("TimeTravel"),
            Some("Discover") => Some("Discover"),
            Some("Detain") => Some("Detain"),
            Some("EndTheTurn") => Some("EndTheTurn"),
            Some("ReverseTurnOrder") => Some("ReverseTurnOrder"),
            Some("TurnFaceDown") if value["kind"] == "KeywordEach" => Some("FaceDownGroup"),
            Some("AdditionalPhase")
                if args["action"]["AdditionalPhase"]["phases"] == "Beginning" =>
            {
                Some("BeginningPhase")
            }
            _ => None,
        },
        Some("Populate") => Some("Populate"),
        Some("Mill") => Some("Mill"),
        Some("PartySize") => Some("Party"),
        Some("CountManaSymbols") => devotion(args),
        Some("Clash") => Some("Clash"),
        Some("CreateContinuousEffect") if args["duration"] == "Indefinite" => Some("Indefinite"),
        Some("Exchange") if args["kind"] == "Control" && args["until"].is_null() => {
            Some("Indefinite")
        }
        Some("CreateReplacement") if mechanic_notes::indefinite_mana_change(args) => {
            Some("Indefinite")
        }
        Some("CostBranch") if args["keyword"] == "Overload" => Some("Overload"),
        Some("Mechanic") if args["mechanic"] == "Bargain" => Some("Bargain"),
        Some("Mechanic")
            if args["mechanic"] == "Toxic"
                && args["amount"]["kind"] == "Constant"
                && args["amount"]["args"] == 1 =>
        {
            Some("Toxic")
        }
        Some("Mechanic")
            if args["mechanic"] == "SpliceOnto"
                && args["filter"]["kind"] == "Subtype"
                && args["filter"]["args"] == "Arcane" =>
        {
            Some("SpliceArcane")
        }
        Some("Term") if cascade_notes::spell_effect(value, "Cascade") => Some("Cascade"),
        Some("Term") if cascade_notes::spell_effect(value, "Conspire") => Some("Conspire"),
        Some("Prototype") => Some("Prototype"),
        Some("Mechanic")
            if args["mechanic"] == "BloodthirstX"
                || args["mechanic"] == "Bloodthirst" && args["amount"] == *PRINTED_X =>
        {
            Some("BloodthirstX")
        }
        Some("Mechanic") => mechanic(&args["mechanic"]).or_else(|| evidence::mention(value)),
        Some("Fight") => Some("Fight"),
        Some("Companion") => Some("Companion"),
        Some("Enchant")
            if args["hosts"]["Permanents"]["kind"] == "CardType"
                && args["hosts"]["Permanents"]["args"] == "Creature" =>
        {
            Some("EnchantCreature")
        }
        Some("Enchant")
            if args["hosts"]["Permanents"]["kind"] == "CardType"
                && args["hosts"]["Permanents"]["args"] == "Land" =>
        {
            Some("EnchantLand")
        }
        Some("CreateRegenerationShields")
            if args["objects"]["kind"] == "One" && args["objects"]["args"]["kind"] == "Source" =>
        {
            Some("RegenerateSource")
        }
        Some("CreateRegenerationShields") => Some("RegenerationShield"),
        Some("Activated")
            if args["origin"] == "Equip"
                && args["cost"]["parts"] == json!([{"Mana":[{"Generic":2}]}]) =>
        {
            Some("EquipTwo")
        }
        Some("DeckRule")
            if args["Partner"]["NamedGroup"].is_string()
                || matches!(
                    args["Partner"].as_str(),
                    Some("FriendsForever" | "CharacterSelect" | "FatherAndSon")
                ) =>
        {
            Some("SharedPartner")
        }
        Some("Activated") if args["limits"]["keyword"] == "Boast" => Some("Boast"),
        Some("Activated") if args["limits"]["keyword"] == "Forecast" => Some("Forecast"),
        Some("Activated")
            if args["limits"]["keyword"] == "Exhaust" && args["limits"]["once"] == true =>
        {
            Some("ExhaustOnce")
        }
        Some("Activated")
            if args["limits"]["keyword"] == "PowerUp" && args["limits"]["once"] == true =>
        {
            Some("PowerUp")
        }
        Some("PlayerState") if args["state"] == "Descended" => Some("Descend"),
        Some("PlayerState") if args["state"] == "CommittedCrime" => Some("Crime"),
        Some("Casting") if args["If"]["rule"].get("Flash").is_some() => Some("FlashPermission"),
        Some("Casting") if args.get("Flash").is_some() => Some("FlashPermission"),
        _ if value["event"] == "CommitsCrime" => Some("Crime"),
        _ if matches!(value["event"].as_str(), Some("LoseLife" | "LosesLife")) => Some("LifeLoss"),
        _ if value["Named"] == "StunCounter" => Some("Stun"),
        _ if value["Named"] == "ShieldCounter" => Some("Shield"),
        _ if value["Named"] == "FinalityCounter" => Some("Finality"),
        _ if value["Named"] == "ExaltedCounter" => Some("Exalted"),
        _ if value["ChangeTapState"]["symbol"] == true
            && value["ChangeTapState"]["tapped"] == false =>
        {
            Some("UntapSymbol")
        }
        _ if value["Amount"]["color"] == "Colorless" => Some("ColorlessMana"),
        _ if value["$sym"] == "overload_branch" => Some("Overload"),
        _ if value["Mill"].is_object() => Some("Mill"),
        _ if !value["Waterbend"].is_null() || !value["WaterbendX"].is_null() => Some("Waterbend"),
        _ if !value["Exert"].is_null() || value.get("MayExert").is_some() => Some("Exert"),
        _ if mechanic_notes::targets_stack_ability(value) => Some("TargetedAbility"),
        _ if mechanic_notes::copies_stack_ability(value) => Some("CopyStackAbility"),
        _ => rules_token(value).or_else(|| evidence::mention(value)),
    }
}

fn entry_keyword_counter(value: &Value, mut visit: impl FnMut(&'static str) -> bool) -> bool {
    let counters = &value["args"]["change"]["Entry"]["CountersOfChoice"]["kinds"];
    value["kind"] == "ModifyEvent"
        && counters.as_array().is_some_and(|kinds| {
            kinds
                .iter()
                .take(16)
                .any(|counter| match counter["Named"].as_str() {
                    Some("HexproofCounter") => visit("EntryHexproofCounter"),
                    Some("MenaceCounter") => visit("EntryMenaceCounter"),
                    _ => false,
                })
        })
}

fn measurement(value: &Value) -> Option<(&'static str, &Value)> {
    let args = &value["args"];
    match value["kind"].as_str() {
        Some("Keyword" | "KeywordEach") => {
            let action = &args["action"];
            let topic = match action_name(action)? {
                "Monstrosity" => "Monstrosity",
                "Discover" => "Discover",
                "Incubate" => "Incubate",
                "Adapt" => "Adapt",
                "Earthbend" => "Earthbend",
                "Endure" => "Endure",
                "Bolster" => "Bolster",
                "CollectEvidence" => "CollectEvidence",
                "Blight" => "Blight",
                "Connive" => "Connive",
                "Amass" => {
                    let amass = &action["Amass"];
                    let topic = match amass["creature_type"].as_str()? {
                        "Zombie" => "AmassZombies",
                        "Orc" => "AmassOrcs",
                        "Goblin" => "AmassGoblins",
                        _ => return None,
                    };
                    return Some((topic, amass.get("amount")?));
                }
                _ => return None,
            };
            Some((topic, action.get(topic)?.get("amount")?))
        }
        Some("Mechanic") => {
            let kind = match args["mechanic"].as_str()? {
                "Toxic" => "Toxic",
                "Renown" => "Renown",
                "Bloodthirst" => "Bloodthirst",
                "Firebending" => "Firebending",
                "Impending" => "Impending",
                _ => return None,
            };
            Some((kind, args.get("amount")?))
        }
        Some("Mill") => Some(("Mill", args.get("amount")?)),
        Some("Scry") => Some(("Scry", args.get("amount")?)),
        Some("Surveil") => Some(("Surveil", args.get("amount")?)),
        _ => evidence::measurement(value),
    }
    .filter(|(_, amount)| !amount.is_null())
}

fn admits_written_x(topic: &str, amount: &Value) -> bool {
    matches!(
        topic,
        "Earthbend"
            | "AmassOrcs"
            | "AmassZombies"
            | "Bolster"
            | "Connive"
            | "Monstrosity"
            | "Surveil"
    ) && !amount.is_null()
        && amount["kind"] != "Constant"
}

fn matches_measurement(topic: &str, amount: Option<&Value>, expected: Option<&Value>) -> bool {
    amount == expected
        || matches!((amount, expected), (Some(amount), Some(expected))
            if super::number_condition::was_written_x(amount)
                && (admits_written_x(topic, &PRINTED_X) && *expected == *PRINTED_X
                    || super::number_condition::same_unmarked_amount(amount, expected)))
}

fn mill_recipient(value: &Value) -> Option<(&'static str, &Value)> {
    let args = &value["args"];
    if value["kind"] != "Mill" {
        return None;
    }
    let players = &args["players"];
    let topic = if players["kind"] == "One" && players["args"]["kind"] == "AbilityController" {
        "ControllerMill"
    } else {
        "OtherMill"
    };
    Some((topic, args.get("amount")?))
}

fn optional_mill(value: &Value) -> Option<(&'static str, &Value)> {
    let args = &value["args"];
    if value["kind"] != "Optional" || args["decider"]["kind"] != "AbilityController" {
        return None;
    }
    let [step] = args["body"]["instructions"].as_array()?.as_slice() else {
        return None;
    };
    if step["kind"] != "Effect" {
        return None;
    }
    let ("ControllerMill", amount) = mill_recipient(&step["args"]["operation"])? else {
        return None;
    };
    Some(("OptionalControllerMill", amount))
}

fn ordinary_activation(ability: &Value) -> bool {
    let activation = if ability["kind"] == "Scoped" {
        &ability["args"]["ability"]
    } else {
        ability
    };
    activation["kind"] == "Activated"
        && activation["args"]["origin"] == "Ordinary"
        && activation["args"]["limits"]["sorcery"] == false
        && activation["args"]["limits"]["keyword"].is_null()
}

fn visit_mentions<'a>(
    ability: &'a Value,
    measured: bool,
    mut visit: impl FnMut(&'static str, Option<&'a Value>) -> bool,
) -> bool {
    // Direct keyword reminders already have a native keyword-level placement.
    let ability = evidence::unscoped(ability);
    let ability = if ability["kind"] == "Saga" {
        let Some(chapter) = ability["args"]["chapters"]
            .as_array()
            .and_then(|items| items.last())
        else {
            return false;
        };
        evidence::unscoped(chapter)
    } else {
        ability
    };
    let mut connive_subject = None;
    ability["kind"] != "Keyword"
        && evidence::visit_nodes(ability, |value, remaining| {
            let Some(object) = value.as_object() else {
                return false;
            };
            if measured {
                return measurement(value)
                    .is_some_and(|(topic, amount)| visit(topic, Some(amount)))
                    || mill_recipient(value)
                        .is_some_and(|(topic, amount)| visit(topic, Some(amount)))
                    || value["kind"] == "Mill"
                        && value["args"]["players"]["kind"] == "All"
                        && visit("EachPlayerMill", value["args"].get("amount"))
                    || optional_mill(value)
                        .is_some_and(|(topic, amount)| visit(topic, Some(amount)));
            }
            marker(value).is_some_and(|topic| {
                visit(topic, None)
                    || matches!(topic, "AirbendOne" | "AirbendMany") && visit("AirbendRule", None)
                    || topic == "RegenerateSource" && visit("RegenerationShield", None)
                    || topic == "Connive" && {
                        let subject = *connive_subject
                            .get_or_insert_with(|| evidence::unique_connive_subject(ability));
                        subject.is_some_and(|subject| visit(subject, None))
                    }
                    || topic == "Connive"
                        && evidence::single_connive(value)
                        && visit("ConniveOne", None)
                    || topic == "EndTheTurn"
                        && ability["kind"] == "Spell"
                        && visit("EndTurnFromSpell", None)
                    || topic == "Storm" && value == ability && visit("StormPermanentCopies", None)
            }) || evidence::flashback_at_mana_cost(value)
                && (visit("FlashbackAtManaCost", None) || visit("FullManaCost", None))
                || casting::permission_notes(value, |topic| visit(topic, None))
                || entry_keyword_counter(value, |topic| visit(topic, None))
                || mechanic_notes::mention(value).is_some_and(|topic| {
                    visit(topic, None)
                        || match topic {
                            "DamageCounterSurvival" => {
                                visit("DamageCounterRecipientSurvives", None)
                            }
                            "DamageCountersSurvival" => {
                                visit("DamageCountersRecipientSurvives", None)
                            }
                            _ => false,
                        }
                })
                || action_duration_note(value).is_some_and(|topic| visit(topic, None))
                || phase_out_reminders(value, |topic| visit(topic, None))
                || object
                    .get("Mana")
                    .and_then(Value::as_array)
                    .is_some_and(|mana| {
                        mana.iter().take(remaining).any(|symbol| {
                            let explanation = if symbol == "Snow" {
                                Some("Snow")
                            } else if symbol["Color"] == "Colorless" {
                                Some("ColorlessMana")
                            } else {
                                None
                            };
                            explanation.is_some_and(|topic| visit(topic, None))
                        })
                    })
        })
        || !measured && ordinary_activation(ability) && visit("InstantActivation", None)
        || !measured && mechanic_notes::root_mentions(ability, |topic| visit(topic, None))
        || !measured && casting_help::grant_note(ability).is_some_and(|topic| visit(topic, None))
        || !measured && cascade_notes::mentions(ability, |topic| visit(topic, None))
        || !measured && object_notes::mentions(ability, |topic| visit(topic, None))
        || !measured && token_notes::prowess_notes(ability, |topic| visit(topic, None))
        || !measured
            && object_notes::regenerates_source(ability)
            && visit("OwnRegenerationShield", None)
        || !measured
            && casting::spells_have_zero_x_when_played(ability)
            && visit("FreeCastXZero", None)
        || !measured && suspend::mentions(ability, |topic| visit(topic, None))
        || !measured && plot::plots_for_controller(ability) && visit("PlottedByController", None)
        || !measured && evidence::root_mentions(ability, |topic| visit(topic, None))
}

fn action_duration_note(value: &Value) -> Option<&'static str> {
    let args = &value["args"];
    let actor = &args["actor"];
    if actor["kind"] != "One" || actor["args"]["kind"] != "AbilityController" {
        return None;
    }
    let action = &args["action"];
    match (value["kind"].as_str()?, action_name(action)?) {
        ("Keyword", "Detain") => Some("DetainOneUntilNextTurn"),
        ("KeywordEach", "Detain") => Some("DetainGroupUntilNextTurn"),
        ("Keyword", "Goad") if action["Goad"]["until"].is_null() => Some("GoadOneUntilNextTurn"),
        ("KeywordEach", "Goad") if action["Goad"]["until"].is_null() => {
            Some("GoadGroupUntilNextTurn")
        }
        _ => None,
    }
}

fn phase_out_reminders(value: &Value, mut visit: impl FnMut(&'static str) -> bool) -> bool {
    let Some(action) = value["args"]["action"].get("PhaseOut") else {
        return false;
    };
    match value["kind"].as_str() {
        Some("Keyword") => {
            visit("PhaseOut") || action["until"].is_null() && visit("PhaseOutNormally")
        }
        Some("KeywordEach") => action["until"].is_null() && visit("PhaseOutGroupNormally"),
        _ => false,
    }
}

fn last_ability(value: &Value) -> Option<&Value> {
    match value.as_array() {
        Some(items) if items.len() <= 16 => items.last(),
        Some(_) => None,
        None => Some(value),
    }
}

fn replace_last(value: &Value, replacement: Value) -> Value {
    let Some((_, prefix)) = value.as_array().and_then(|items| items.split_last()) else {
        return replacement;
    };
    let mut result = prefix.to_vec();
    result.push(replacement);
    Value::Array(result)
}

fn claim(explanation: &Value) -> Option<(&str, Option<&Value>)> {
    if let Some(topic) = explanation.as_str() {
        return Some((topic, None));
    }
    let [topic, amount] = explanation["args"].as_array()?.as_slice() else {
        return None;
    };
    (explanation["kind"] == "List" && topic["kind"] == "Text" && amount["kind"] == "Number")
        .then_some((topic["args"].as_str()?, Some(&amount["args"])))
}

fn bound_claim(name: &str, explanation: &Value) -> bool {
    if name == "contextual_reminder"
        && explanation
            .as_str()
            .is_some_and(casting_help::needs_bound_scope)
    {
        return true;
    }
    matches!(
        (name, claim(explanation).map(|(topic, _)| topic)),
        (
            "contextual_reminder",
            Some(
                "ConniveSelf"
                    | "ConniveOther"
                    | "EncounteredPhenomenon"
                    | "CounterProhibited"
                    | "PreparedSource"
                    | "PermanentSpellCopy"
                    | "PermanentSpellCopyRule"
                    | "CreatureSpellCopy"
                    | "NoncreatureForm"
                    | "BattlefieldReturn"
                    | "ReturnExiledCreature"
                    | "PhaseOutUntilYourTurn"
                    | "EquipmentKeepsController"
                    | "SummoningSickness"
                    | "InfectTokenGroup"
                    | "WitherGroup"
                    | "EpicWithoutTargets"
                    | "EpicWithTargets"
                    | "GrantedArtifactAffinity"
                    | "GrantedScavenge"
                    | "GrantedSuspendOwner"
                    | "GrantedSuspendController"
                    | "SeekWholeLibrary"
                    | "PlottedByController"
                    | "JumpStartFromYourGraveyard"
                    | "DamageCounterRecipientSurvives"
                    | "DamageCountersRecipientSurvives"
                    | "Indefinite"
                    | "OwnSourceLifelink"
                    | "OwnCreatureLifelink"
                    | "OwnGroupLifelink"
                    | "OwnSliverLifelink"
                    | "StormPermanentCopies"
                    | "ControlledGroupPhasesOut"
                    | "ManifestDreadTargetController"
                    | "CascadePlayerNextSpell"
                    | "CascadeFirstSpell"
                    | "CascadeFirstExiledSpell"
                    | "CascadeCommander"
                    | "CascadeEnchantmentFromHand"
                    | "CascadeTwiceOnGrant"
                    | "EscapeFromYourGraveyardThisTurn"
                    | "OwnRegenerationShield"
                    | "SliverRegeneration"
                    | "SourceProwess"
                    | "TokenProwess"
                    | "GroupTokenProwess"
                    | "TargetPlayerGroupPhasesOut"
                    | "AuraSpellCopyRule"
                    | "CopyTriggeredAbility"
                    | "SagaReadAhead"
                    | "SagaReplicate"
                    | "HexproofAndIndestructible"
                    | "CardTypes"
            )
        )
    )
}

const BOUND_CLAIMS: &str = "$bound_reminders";

pub(crate) fn take_bound_claims(ability: &mut Value) -> Result<Vec<Value>, String> {
    if ability["kind"] != "WithPresentation" {
        return Ok(Vec::new());
    }
    let Some(value) = ability["args"]
        .as_object_mut()
        .and_then(|args| args.remove(BOUND_CLAIMS))
    else {
        return Ok(Vec::new());
    };
    match value {
        Value::Array(claims) if claims.len() <= 8 => Ok(claims),
        _ => Err("invalid bound reminder list".into()),
    }
}

pub(crate) fn check_bound_claims(
    ability: &Value,
    claims: &[Value],
    permanent_source: bool,
) -> Result<(), String> {
    for explanation in claims {
        if let Some(symbol) = explanation.get("$cost_symbol") {
            if !evidence::cost_symbols(ability).contains(symbol) {
                return Err("reminder has no matching bound payment symbol".into());
            }
            continue;
        }
        if let Some(description) = explanation.get("$token_description") {
            if !token_notes::agrees(ability, description, explanation["plural"] == true) {
                return Err("reminder disagrees with the created token".into());
            }
            continue;
        }
        let (topic, expected) = claim(explanation).ok_or("invalid bound reminder claim")?;
        if topic == "StormPermanentCopies" && !permanent_source {
            return Err("this storm reminder requires a permanent spell".into());
        }
        if topic == "PermanentSpellCopy"
            && permanent_source
            && evidence::copies_source_spell(ability)
        {
            continue;
        }
        if !visit_mentions(ability, expected.is_some(), |kind, operand| {
            kind == topic && operand == expected
        }) {
            return Err("reminder has no matching bound operand in this ability".into());
        }
    }
    Ok(())
}

pub(crate) fn attach(name: &str, value: &Value, explanation: &Value) -> Result<Value, String> {
    let ability = last_ability(value).ok_or("reminder needs a bounded nonempty ability line")?;
    let inner = if ability["kind"] == "WithPresentation" {
        &ability["args"]["ability"]
    } else {
        ability
    };
    // Spell paragraphs can share a whole-card frame. A claim about the
    // spell's own targets or resolved objects must wait for binding.
    let needs_bound_spell = matches!(
        explanation.as_str(),
        Some(
            "ConniveSelf"
                | "ConniveOther"
                | "EquipmentKeepsController"
                | "SummoningSickness"
                | "InfectTokenGroup"
                | "EpicWithoutTargets"
                | "EpicWithTargets"
                | "GrantedSuspendOwner"
                | "GrantedSuspendController"
                | "SeekWholeLibrary"
                | "PlottedByController"
                | "CreatureSpellCopy"
                | "OwnCreatureLifelink"
                | "OwnGroupLifelink"
                | "ControlledGroupPhasesOut"
                | "ManifestDreadTargetController"
                | "EscapeFromYourGraveyardThisTurn"
                | "TokenProwess"
                | "GroupTokenProwess"
                | "TargetPlayerGroupPhasesOut"
                | "HexproofAndIndestructible"
        )
    );
    let mut deferred =
        (inner["kind"] != "Spell" || needs_bound_spell) && bound_claim(name, explanation);
    let matches = if matches!(
        name,
        "single_token_description" | "plural_token_description"
    ) {
        deferred = true;
        true
    } else if deferred {
        true
    } else if let Some(candidates) = typed_claims(name, value, inner) {
        let matches = candidates.iter().any(|candidate| candidate == explanation);
        deferred = !matches && name == "cost_symbol_reminder";
        matches || deferred
    } else {
        let (topic, amount) = claim(explanation).ok_or("invalid contextual reminder claim")?;
        visit_mentions(inner, amount.is_some(), |found, quantity| {
            found == topic && matches_measurement(topic, quantity, amount)
        })
    };
    if !matches {
        return Err("reminder has no matching operand in this ability".into());
    }
    // The chapter list is a list of triggers. The owning Saga can carry a
    // presentation wrapper, but the individual chapter is kept as a trigger.
    if name == "chapter_reminder" {
        return Ok(value.clone());
    }
    let mut result = if ability["kind"] == "WithPresentation" {
        let mut attached = ability.clone();
        attached["args"]["reminder"] = Value::Bool(true);
        attached
    } else {
        json!({"kind":"WithPresentation","args":{"ability":ability,"label":null,"reminder":true}})
    };
    if deferred {
        let args = result["args"]
            .as_object_mut()
            .ok_or("reminder presentation")?;
        let list = args
            .entry(BOUND_CLAIMS)
            .or_insert_with(|| Value::Array(Vec::new()));
        let list = list.as_array_mut().ok_or("invalid bound reminder list")?;
        if list.len() >= 8 {
            return Err("bound reminder limit".into());
        }
        list.push(match name {
            "cost_symbol_reminder" => json!({"$cost_symbol":explanation}),
            "single_token_description" | "plural_token_description" => {
                json!({"$token_description":explanation, "plural":name == "plural_token_description"})
            }
            _ => explanation.clone(),
        });
    }
    Ok(replace_last(value, result))
}

pub(crate) fn unify<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [ability, explanation] = args else {
        return Vec::new();
    };
    let Some(wrapped) = last_ability(value) else {
        return Vec::new();
    };
    if name == "chapter_reminder" {
        return claims(name, value, wrapped)
            .iter()
            .flat_map(|claim| explanation.unify_made(claim, &bindings))
            .flat_map(|binding| ability.unify(value, binding))
            .collect();
    }
    if wrapped["kind"] != "WithPresentation" || wrapped["args"]["reminder"] != true {
        return Vec::new();
    }
    let inner = &wrapped["args"]["ability"];
    let mut without_reminder = wrapped.clone();
    without_reminder["args"]["reminder"] = Value::Bool(false);
    let source = if wrapped["args"]["label"].is_null() {
        inner
    } else {
        &without_reminder
    };
    let source = replace_last(value, source.clone());
    let mut solutions = Vec::new();
    for claim in claims(name, value, inner) {
        for matched in explanation.unify_made(&claim, &bindings) {
            solutions.extend(ability.unify_made(&source, &matched));
            solutions.extend(ability.unify(value, matched));
        }
    }
    solutions
}

fn claims(name: &str, line: &Value, ability: &Value) -> Vec<Value> {
    if let Some(claims) = typed_claims(name, line, ability) {
        return claims;
    }
    let mut found = Vec::new();
    visit_mentions(ability, name == "measured_reminder", |kind, amount| {
        if !found.contains(&(kind, amount)) {
            found.push((kind, amount));
        }
        if amount.is_some_and(|amount| admits_written_x(kind, amount)) {
            let alias = (kind, Some(&*PRINTED_X));
            if !found.contains(&alias) {
                found.push(alias);
            }
        }
        false
    });
    // Inverse matching sees the ability before the printer supplies its face
    // types. Reparse checks the source type before this wording can verify.
    if name == "contextual_reminder"
        && evidence::copies_source_spell(ability)
        && !found.contains(&("PermanentSpellCopy", None))
    {
        found.push(("PermanentSpellCopy", None));
    }
    found
        .into_iter()
        .map(|(kind, amount)| match amount {
            None => Value::from(kind),
            Some(amount) => {
                json!({"kind":"List","args":[{"kind":"Text","args":kind},{"kind":"Number","args":amount}]})
            }
        })
        .collect()
}

fn typed_claims(name: &str, line: &Value, ability: &Value) -> Option<Vec<Value>> {
    Some(match name {
        "cycling_reminder" => evidence::cycling_claim(ability).into_iter().collect(),
        "cycling_pair_reminder" => evidence::cycling_pair_claim(line, ability),
        "cycling_grant_reminder" => evidence::cycling_grant_claim(ability).into_iter().collect(),
        "cost_symbol_reminder" => evidence::cost_symbols(ability),
        "awaken_reminder" => evidence::awaken_claim(ability).into_iter().collect(),
        "ward_grant_reminder" => evidence::ward_grant_claims(ability),
        "equip_payment_reminder" => evidence::equip_payment_claim(ability).into_iter().collect(),
        "unearth_grant_reminder" => evidence::unearth_grant_claim(ability).into_iter().collect(),
        "spent_pair_reminder" => casting::spent_pair_claims(ability),
        "single_token_description" | "plural_token_description" => {
            token_notes::print_descriptions(ability, name == "plural_token_description")
        }
        _ => return None,
    })
}
