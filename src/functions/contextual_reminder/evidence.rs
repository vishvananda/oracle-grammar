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

pub(super) fn mention(value: &Value) -> Option<&'static str> {
    let args = &value["args"];
    match value["kind"].as_str() {
        Some("Shadow") => Some("Shadow"),
        Some("Intimidate") => Some("Intimidate"),
        Some("Wither") => Some("Wither"),
        Some("Infect") => Some("Infect"),
        Some("Defender") => Some("Defender"),
        Some("Horsemanship") => Some("Horsemanship"),
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
    if ability["kind"] != "Triggered" {
        return false;
    }
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
    if cast["event"] != "Cast" || !copies_event_spell(&args["body"]) {
        return false;
    }
    visit("PermanentSpellCopyRule")
        || permanent_spell_filter(&cast["spells"], 0) && visit("PermanentSpellCopy")
}

fn copies_event_spell(program: &Value) -> bool {
    program["instructions"].as_array().is_some_and(|steps| {
        steps.iter().take(64).any(|step| {
            let op = &step["args"]["operation"];
            step["kind"] == "Effect"
                && op["kind"] == "CopyOnStack"
                && op["args"]["object"]["kind"] == "EventSubject"
        })
    })
}

fn permanent_spell_filter(filter: &Value, depth: usize) -> bool {
    if depth == 12 {
        return false;
    }
    match filter["kind"].as_str() {
        Some("CardType") => matches!(
            filter["args"].as_str(),
            Some("Artifact" | "Battle" | "Creature" | "Enchantment" | "Planeswalker")
        ),
        Some("AllOf" | "AnyOf") => filter["args"].as_array().is_some_and(|items| {
            if items.is_empty() || items.len() > 32 {
                return false;
            }
            if filter["kind"] == "AllOf" {
                items
                    .iter()
                    .any(|item| permanent_spell_filter(item, depth + 1))
            } else {
                items
                    .iter()
                    .all(|item| permanent_spell_filter(item, depth + 1))
            }
        }),
        _ => false,
    }
}
