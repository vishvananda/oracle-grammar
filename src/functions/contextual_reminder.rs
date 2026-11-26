//! A closed explanatory sentence needs a matching operand in its own ability.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

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

fn marker(value: &Value) -> Option<&'static str> {
    let args = &value["args"];
    match value["kind"].as_str() {
        Some("Modified") if value.as_object()?.len() == 1 => Some("Modified"),
        Some("AnyOf") if historic_filter(args) => Some("Historic"),
        Some("Indestructible") => Some("Indestructible"),
        Some("Menace") => Some("Menace"),
        Some("Shroud") => Some("Shroud"),
        Some("Hexproof") if args.is_null() => Some("Hexproof"),
        Some("Trample") if args.is_null() => Some("Trample"),
        Some("Fear") => Some("Fear"),
        Some("Deathtouch") => Some("Deathtouch"),
        Some("Flying") => Some("Flying"),
        Some("Haste") => Some("Haste"),
        Some("Lifelink") => Some("Lifelink"),
        Some("Vigilance") => Some("Vigilance"),
        Some("Reach") => Some("Reach"),
        Some("FirstStrike") => Some("FirstStrike"),
        Some("DoubleStrike") => Some("DoubleStrike"),
        Some("Keyword" | "KeywordEach") => match action_name(&args["action"]) {
            Some("Venture") => Some("Venture"),
            Some("Learn") => Some("Learn"),
            Some("Explore") => Some("Explore"),
            Some("ManifestDread") => Some("ManifestDread"),
            Some("Goad") if args["action"]["Goad"]["until"].is_null() => Some("Goad"),
            Some("Exert") => Some("Exert"),
            _ => None,
        },
        Some("Populate") => Some("Populate"),
        Some("Mill") => Some("Mill"),
        Some("PartySize") => Some("Party"),
        Some("Clash") => Some("Clash"),
        Some("CreateContinuousEffect") if args["duration"] == "Indefinite" => Some("Indefinite"),
        Some("CostBranch") if args["keyword"] == "Overload" => Some("Overload"),
        Some("Mechanic") if args["mechanic"] == "Bargain" => Some("Bargain"),
        Some("Mechanic")
            if args["mechanic"] == "SpliceOnto"
                && args["filter"]["kind"] == "Subtype"
                && args["filter"]["args"] == "Arcane" =>
        {
            Some("SpliceArcane")
        }
        Some("Prototype") => Some("Prototype"),
        Some("PlayerState") if args["state"] == "CommittedCrime" => Some("Crime"),
        _ if value["event"] == "CommitsCrime" => Some("Crime"),
        _ if value["Named"] == "StunCounter" => Some("Stun"),
        _ if value["Named"] == "ShieldCounter" => Some("Shield"),
        _ if value["$sym"] == "overload_branch" => Some("Overload"),
        _ if value["Mill"].is_object() => Some("Mill"),
        _ if !value["Waterbend"].is_null() || !value["WaterbendX"].is_null() => Some("Waterbend"),
        _ if !value["Exert"].is_null() || value.get("MayExert").is_some() => Some("Exert"),
        _ if value["kind"]["Object"]["kind"] == "Ability"
            && value["kind"]["Object"]["zones"] == json!(["Stack"]) =>
        {
            Some("TargetedAbility")
        }
        _ => None,
    }
}

fn visit_mentions(ability: &Value, mut visit: impl FnMut(&'static str) -> bool) -> bool {
    fn walk(
        value: &Value,
        depth: usize,
        remaining: &mut usize,
        visit: &mut impl FnMut(&'static str) -> bool,
    ) -> bool {
        if *remaining == 0 {
            return false;
        }
        *remaining -= 1;
        if let Some(object) = value.as_object() {
            if marker(value).is_some_and(&mut *visit) {
                return true;
            }
            if let Some(mana) = object.get("Mana").and_then(Value::as_array) {
                for symbol in mana.iter().take(*remaining) {
                    let explanation = if symbol == "Snow" {
                        Some("Snow")
                    } else if symbol["Color"] == "Colorless" {
                        Some("ColorlessMana")
                    } else {
                        None
                    };
                    if explanation.is_some_and(&mut *visit) {
                        return true;
                    }
                }
            }
        }
        if depth == 80 {
            return false;
        }
        match value {
            Value::Object(fields) => walk_children(fields.values(), depth, remaining, visit),
            Value::Array(items) => walk_children(items.iter(), depth, remaining, visit),
            _ => false,
        }
    }

    fn walk_children<'a>(
        children: impl Iterator<Item = &'a Value>,
        depth: usize,
        remaining: &mut usize,
        visit: &mut impl FnMut(&'static str) -> bool,
    ) -> bool {
        for child in children {
            if *remaining == 0 {
                break;
            }
            if walk(child, depth + 1, remaining, visit) {
                return true;
            }
        }
        false
    }

    // Direct keyword reminders already have a native keyword-level placement.
    ability["kind"] != "Keyword" && walk(ability, 0, &mut 8192, &mut visit)
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

pub(crate) fn attach(value: &Value, explanation: &Value) -> Result<Value, String> {
    let ability = last_ability(value).ok_or("reminder needs a bounded nonempty ability line")?;
    let inner = if ability["kind"] == "WithPresentation" {
        &ability["args"]["ability"]
    } else {
        ability
    };
    if !explanation
        .as_str()
        .is_some_and(|kind| visit_mentions(inner, |found| found == kind))
    {
        return Err("reminder has no matching operand in this ability".into());
    }
    let result = if ability["kind"] == "WithPresentation" {
        let mut attached = ability.clone();
        attached["args"]["reminder"] = Value::Bool(true);
        attached
    } else {
        json!({"kind":"WithPresentation","args":{"ability":ability,"label":null,"reminder":true}})
    };
    Ok(replace_last(value, result))
}

pub(crate) fn unify<'p, 'v>(
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
    let mut found = Vec::new();
    visit_mentions(inner, |kind| {
        if !found.contains(&kind) {
            found.push(kind);
        }
        false
    });
    let source = replace_last(value, source.clone());
    let mut solutions = Vec::new();
    for kind in found {
        for matched in explanation.unify_made(&Value::from(kind), &bindings) {
            solutions.extend(ability.unify_made(&source, &matched));
            solutions.extend(ability.unify(value, matched));
        }
    }
    solutions
}
