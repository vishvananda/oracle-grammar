use serde_json::{Value, json};

pub(super) fn unscoped(value: &Value) -> &Value {
    if value["$sym"] == "scope" {
        &value["args"][0]
    } else {
        value
    }
}

pub(super) fn cycling_claim(value: &Value) -> Option<Value> {
    let scope = &value["args"];
    if value["kind"] != "Scoped" || scope["zones"] != json!(["Hand"]) {
        return None;
    }
    let activation = unscoped(&scope["ability"]);
    if activation["kind"] != "Activated" {
        return None;
    }
    let search = activation["args"]["origin"]["Cycling"].get("search")?;
    if search.is_null() {
        return None;
    }
    let cost = activation["args"].get("cost")?;
    Some(
        json!({"kind":"List","args":[{"kind":"Cost","args":cost},{"kind":"Filter","args":search}]}),
    )
}

pub(super) fn cost_symbols(ability: &Value) -> impl Iterator<Item = &Value> {
    let mut ability = unscoped(ability);
    if ability["kind"] == "Scoped" {
        ability = unscoped(&ability["args"]["ability"]);
    }
    let args = &ability["args"];
    let cost = match ability["kind"].as_str() {
        Some("Activated" | "Mechanic" | "CostBranch") => &args["cost"],
        Some("Casting") if args.get("Permission").is_some() => &args["Permission"]["cost"],
        Some("Casting") => &args["AdditionalCost"]["cost"],
        _ => &Value::Null,
    };
    cost["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .take(32)
        .flat_map(|part| part["Mana"].as_array().into_iter().flatten().take(32))
}

pub(super) fn awaken_claim(ability: &Value) -> Option<Value> {
    let ability = unscoped(ability);
    let args = &ability["args"];
    if ability["kind"] != "CostBranch" || args["keyword"] != "Awaken" {
        return None;
    }
    let paid = unscoped(&args["paid"]);
    if paid["kind"] != "Spell" {
        return None;
    }
    let op = effects(&paid["args"]["body"]).last()?;
    let actor = &op["args"]["actor"];
    if op["kind"] != "Keyword"
        || actor["kind"] != "One"
        || actor["args"]["kind"] != "AbilityController"
    {
        return None;
    }
    let amount = op["args"]["action"]["Awaken"].get("amount")?;
    Some(
        json!({"kind":"List","args":[{"kind":"Cost","args":args["cost"]},{"kind":"Number","args":amount}]}),
    )
}

pub(super) fn mention(value: &Value) -> Option<&'static str> {
    let args = &value["args"];
    match value["kind"].as_str() {
        Some("Shadow") => Some("Shadow"),
        Some("Intimidate") => Some("Intimidate"),
        Some("Wither") => Some("Wither"),
        Some("Infect") => Some("Infect"),
        Some("Defender") => Some("Defender"),
        Some("Horsemanship") => Some("Horsemanship"),
        Some("Changeling") => Some("Changeling"),
        Some("Landwalk") if args["kind"] == "Subtype" => match args["args"].as_str()? {
            "Plains" => Some("Plainswalk"),
            "Island" => Some("Islandwalk"),
            "Swamp" => Some("Swampwalk"),
            "Mountain" => Some("Mountainwalk"),
            "Forest" => Some("Forestwalk"),
            _ => None,
        },
        Some("Landwalk")
            if args["kind"] == "Not"
                && args["args"]["kind"] == "Supertype"
                && args["args"]["args"] == "Basic" =>
        {
            Some("Nonbasicwalk")
        }
        Some("Manifest") if args["cloak"] == false => Some("Manifest"),
        Some("PhaseOut") => Some("PhaseOut"),
        Some("Rule") if args["GoadedBy"]["kind"] == "AbilityController" => Some("Goad"),
        _ if value["Named"] == "PoisonCounter" => Some("PoisonCounter"),
        _ => None,
    }
}

pub(super) fn measurement(value: &Value) -> Option<(&'static str, &Value)> {
    let cost = &value["Keyword"];
    if cost["action"] == "CollectEvidence" {
        return Some(("CollectEvidence", cost.get("amount")?));
    }
    let expends = value.get("NthEachTurn")?;
    let event = &expends["event"]["Player"];
    let test = &expends["test"];
    if event["event"] == "Expends"
        && event["players"]["kind"] == "One"
        && event["players"]["args"]["kind"] == "AbilityController"
        && test["kind"] == "Compare"
        && test["args"]["comparison"] == "Equal"
    {
        return Some(("Expend", test["args"].get("value")?));
    }
    None
}

pub(super) fn root_mentions(ability: &Value, mut visit: impl FnMut(&'static str) -> bool) -> bool {
    let ability = unscoped(ability);
    let args = &ability["args"];
    let encounter = &args["event"]["Player"];
    if encounter["event"] == "EncountersPhenomenon"
        && encounter["objects"]["kind"] == "SameObject"
        && encounter["objects"]["args"]["kind"] == "Source"
        && visit("EncounteredPhenomenon")
    {
        return true;
    }
    let cast = &args["event"]["Spell"];
    effects(&args["body"]).any(|op| {
        if op["kind"] != "CopyOnStack" {
            return false;
        }
        let copy = &op["args"];
        let object = &copy["object"];
        let filter = if cast["event"] == "Cast" && object["kind"] == "EventSubject" {
            &cast["spells"]
        } else if let Some(target) = spell_target(object, &args["announcement"]) {
            &target["filter"]
        } else {
            return false;
        };
        visit("PermanentSpellCopyRule")
            || copy["times"]["kind"] == "Constant"
                && copy["times"]["args"] == 1
                && permanent_spell_filter(filter, 0, &mut 256)
                && visit("PermanentSpellCopy")
    })
}

fn effects(program: &Value) -> impl Iterator<Item = &Value> {
    program["instructions"]
        .as_array()
        .into_iter()
        .flatten()
        .take(64)
        .filter(|step| step["kind"] == "Effect")
        .map(|step| &step["args"]["operation"])
}

fn spell_target<'a>(object: &'a Value, announcement: &'a Value) -> Option<&'a Value> {
    if object["kind"] != "Target" || object["args"]["index"] != 0 {
        return None;
    }
    let slot = &object["args"]["slot"];
    let target = if slot["$sym"] == "target" {
        slot["args"].get(0)?
    } else {
        let slot = slot.as_u64()?;
        announcement["targets"]
            .as_array()?
            .iter()
            .take(64)
            .find(|target| target["slot"] == slot)?
    };
    let domain = &target["kind"]["Object"];
    (domain["kind"] == "Spell").then_some(target)
}

fn permanent_spell_filter(filter: &Value, depth: usize, remaining: &mut usize) -> bool {
    if depth == 12 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    match filter["kind"].as_str() {
        Some("CardType") => matches!(
            filter["args"].as_str(),
            Some("Artifact" | "Battle" | "Creature" | "Enchantment" | "Land" | "Planeswalker")
        ),
        Some("AllOf" | "AnyOf") => filter["args"].as_array().is_some_and(|items| {
            if items.is_empty() || items.len() > 32 {
                return false;
            }
            if filter["kind"] == "AllOf" {
                items
                    .iter()
                    .any(|item| permanent_spell_filter(item, depth + 1, remaining))
            } else {
                items
                    .iter()
                    .all(|item| permanent_spell_filter(item, depth + 1, remaining))
            }
        }),
        _ => false,
    }
}
