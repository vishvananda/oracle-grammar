//! One literal linked-card action has two distinct, positively proved contexts.
//! Lexical sites are external metadata; properties and explicit queries stay scalar.
use crate::model::ir::Ability;
use serde_json::{Value, json};
use std::collections::BTreeSet;
const MARKER: &str = "linked_return_literal";

#[derive(Clone, Copy)]
pub(crate) struct Site {
    ability: usize,
    returned: usize,
    producer: Option<usize>,
    optional: bool,
    plural: bool,
}
fn controller() -> Value {
    json!({"kind":"AbilityController"})
}
fn linked_zone() -> Value {
    json!({"kind":"Zone","args":{"scope":{"Shared":"Exile"},"kind":"Card",
        "filter":{"kind":"ExiledWith","args":{"kind":"Source"}}}})
}
fn scalar() -> Value {
    json!({"kind":"One","args":{"kind":"Query","args":{"objects":linked_zone(),"index":0}}})
}
fn guarded(objects: Value) -> Value {
    json!({"kind":"InZone","args":{"objects":objects,"zone":"Exile"}})
}
fn items(value: &Value) -> Option<&[Value]> {
    value["args"]["body"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
}
fn operation(instruction: &Value) -> Option<&Value> {
    if instruction["kind"] != "Effect" {
        return None;
    }
    let operation = &instruction["args"]["operation"];
    if super::symbol_name(operation) == Some("owner_return") {
        operation["args"].get(0)
    } else {
        Some(operation)
    }
}
fn returned<'a>(card: &'a Value, site: &Site) -> Option<&'a Value> {
    let ability = super::presentation::core(card.get(site.ability)?);
    let mut instruction = items(ability)?.get(site.returned)?;
    if site.optional {
        let [only] = items(instruction)? else {
            return None;
        };
        instruction = only;
    }
    let op = operation(instruction)?;
    if site.producer.is_none() {
        return (op["kind"] == "Move").then_some(op);
    }
    if op["kind"] != "CreateDelayedTrigger"
        || op["args"]["event"] != json!({"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}})
        || op["args"]["start"] != "Immediately"
        || !op["args"]["expires"].is_null()
        || !op["args"]["intervening_if"].is_null()
    {
        return None;
    }
    let [only] = items(op)? else { return None };
    operation(only).filter(|op| op["kind"] == "Move")
}
fn raw_literal(mut objects: &Value) -> bool {
    if super::symbol_name(objects) == Some("zone_members") {
        let Some([inner]) = objects["args"].as_array().map(Vec::as_slice) else {
            return false;
        };
        objects = inner;
    }
    if super::symbol_name(objects) == Some(MARKER) {
        return objects["args"] == json!([guarded(scalar())]);
    }
    if objects["kind"] != "One" {
        return false;
    }
    let reference = &objects["args"];
    if super::symbol_name(reference) != Some("remember_object") {
        return false;
    }
    let Some([literal]) = reference["args"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let mut literal = literal;
    if super::symbol_name(literal) == Some(super::activation_exile::NAME) {
        let Some([inner]) = literal["args"].as_array().map(Vec::as_slice) else {
            return false;
        };
        literal = inner;
    }
    *literal == scalar()["args"]
}
fn source_leaves(event: &Value, lexical: bool) -> bool {
    let objects = if event["Object"]["event"] == "LeavesBattlefield" {
        &event["Object"]["objects"]
    } else if event["ZoneChange"]["from"] == "Battlefield"
        && event["ZoneChange"]["to"] == "Graveyard"
    {
        &event["ZoneChange"]["objects"]
    } else {
        return false;
    };
    objects["kind"] == "SameObject"
        && (objects["args"] == json!({"kind":"Source"})
            || lexical
                && matches!(
                    super::symbol_name(&objects["args"]),
                    Some("self" | "named_source")
                ))
}

fn leaving_return(ability: usize, value: &Value) -> Option<Site> {
    if value["kind"] != "Triggered" || !source_leaves(&value["args"]["event"], true) {
        return None;
    }
    let [instruction] = items(value)? else {
        return None;
    };
    let optional =
        instruction["kind"] == "Optional" && instruction["args"]["decider"] == controller();
    let move_card = if optional {
        let [only] = items(instruction)? else {
            return None;
        };
        operation(only)?
    } else {
        operation(instruction)?
    };
    (move_card["kind"] == "Move"
        && move_card["args"]["destination"].get("OwnersHand").is_some()
        && move_card["args"]["qualifiers"] == json!([]))
    .then_some(Site {
        ability,
        returned: 0,
        producer: None,
        optional,
        plural: false,
    })
}

fn spell_return(ability: usize, value: &Value) -> Option<Site> {
    if value["kind"] != "Spell" {
        return None;
    }
    let body = items(value)?;
    let (exile, returned) = match body {
        [exile, returned] => (exile, returned),
        [exile, investigate, returned]
            if operation(investigate).is_some_and(|op| {
                op["kind"] == "Keyword" && op["args"]["action"].get("Investigate").is_some()
            }) =>
        {
            (exile, returned)
        }
        _ => return None,
    };
    if !direct_exile(exile) || operation(returned)?["kind"] != "CreateDelayedTrigger" {
        return None;
    }
    Some(Site {
        ability,
        returned: body.len() - 1,
        producer: Some(0),
        optional: false,
        plural: true,
    })
}

fn candidates(card: &Value) -> Result<Vec<Site>, String> {
    let Some(abilities) = card.as_array() else {
        return Ok(Vec::new());
    };
    if abilities.len() > 4096 {
        return Err("linked card action ability limit".into());
    }
    let mut out = Vec::new();
    for (ability, value) in abilities.iter().enumerate() {
        let value = super::presentation::core(value);
        if let Some(site) = leaving_return(ability, value).or_else(|| spell_return(ability, value))
        {
            crate::functions::branch_bounds(value)?;
            out.push(site);
            continue;
        }
        if value["kind"] != "Activated" {
            continue;
        }
        let Some(body) = items(value) else { continue };
        let candidate = if body.len() == 1 {
            operation(&body[0]).is_some_and(|op| {
                op["kind"] == "Move" && op["args"]["destination"].get("OwnersHand").is_some()
            })
        } else if (2..=4096).contains(&body.len()) {
            operation(&body[body.len() - 1]).is_some_and(|op| op["kind"] == "CreateDelayedTrigger")
                && operation(&body[body.len() - 2]).is_some_and(|op| {
                    op["kind"] == "Move"
                        && super::movement::zone(&op["args"]["destination"]) == Some("Exile")
                })
        } else {
            false
        };
        if candidate {
            crate::functions::branch_bounds(value)?;
            out.push(Site {
                ability,
                returned: body.len() - 1,
                producer: (body.len() > 1).then_some(body.len().saturating_sub(2)),
                optional: false,
                plural: false,
            });
        }
    }
    Ok(out)
}
fn has_literal(card: &Value, site: &Site) -> bool {
    let Some(op) = returned(card, site) else {
        return false;
    };
    let objects = &op["args"]["objects"];
    if !site.plural {
        return raw_literal(objects);
    }
    let objects = if super::symbol_name(objects) == Some("zone_members") {
        &objects["args"][0]
    } else {
        objects
    };
    super::symbol_name(objects) == Some(MARKER) && objects["args"] == json!([linked_zone()])
}

/// Capture noun provenance and erase no-focus recipe annotations before binding.
pub(crate) fn prepare(card: &mut Value) -> Result<Vec<Site>, String> {
    let sites = candidates(card)?
        .into_iter()
        .filter(|site| has_literal(card, site))
        .collect();
    strip(card, 0, &mut 131_072)?;
    Ok(sites)
}

/// Read-only fast check: returns `Ok(true)` when `prepare` would return zero sites
/// and make zero edits, avoiding a full-card clone before per-ability cache lookup.
pub(crate) fn is_plain(card: &Value) -> Result<bool, String> {
    let has_site = candidates(card)?.iter().any(|site| has_literal(card, site));
    if has_site {
        return Ok(false);
    }
    fn check(
        value: &Value,
        depth: usize,
        work: &mut usize,
        local_exile: bool,
    ) -> Result<bool, String> {
        if depth > 128 || *work == 0 {
            return Err("linked literal work/depth limit".into());
        }
        *work -= 1;
        let local_exile = local_exile
            && !matches!(
                value["kind"].as_str(),
                Some("Triggered" | "Activated" | "Spell")
            );
        if local_exile && value["kind"] == "Move" && raw_literal(&value["args"]["objects"]) {
            return Err("the exiled card in this resolution needs its local exile receipt".into());
        }
        if super::symbol_name(value) == Some(MARKER) {
            return Ok(false);
        }
        if let Some(instructions) = value.get("instructions").and_then(Value::as_array) {
            let mut local_exile = local_exile;
            for instruction in instructions {
                let produced = optional_exile(instruction);
                if !check(instruction, depth + 1, work, local_exile)? {
                    return Ok(false);
                }
                local_exile |= produced;
            }
            return Ok(true);
        }
        match value {
            Value::Object(fields) => {
                for child in fields.values() {
                    if !check(child, depth + 1, work, local_exile)? {
                        return Ok(false);
                    }
                }
            }
            Value::Array(items) => {
                for child in items {
                    if !check(child, depth + 1, work, local_exile)? {
                        return Ok(false);
                    }
                }
            }
            _ => {}
        }
        Ok(true)
    }
    check(card, 0, &mut 131_072, false)
}
fn strip(value: &mut Value, depth: usize, work: &mut usize) -> Result<(), String> {
    strip_context(value, depth, work, false)
}

fn optional_exile(instruction: &Value) -> bool {
    let Some(op) = operation(instruction) else {
        return false;
    };
    if op["kind"] != "PayCost" || op["args"]["optional"] != true {
        return false;
    }
    op["args"]["cost"]["parts"].as_array().is_some_and(|parts| {
        parts.iter().any(|part| {
            part["Move"]["destination"].get("Exile").is_some()
                || part["PlayerAction"]["body"]["instructions"]
                    .as_array()
                    .is_some_and(|body| {
                        matches!(body.as_slice(), [only] if operation(only).is_some_and(|op|
                            op["kind"] == "Move" && op["args"]["destination"].get("Exile").is_some()))
                    })
        })
    })
}

fn strip_context(
    value: &mut Value,
    depth: usize,
    work: &mut usize,
    local_exile: bool,
) -> Result<(), String> {
    if depth > 128 || *work == 0 {
        return Err("linked literal work/depth limit".into());
    }
    *work -= 1;
    let local_exile = local_exile
        && !matches!(
            value["kind"].as_str(),
            Some("Triggered" | "Activated" | "Spell")
        );
    if local_exile && value["kind"] == "Move" && raw_literal(&value["args"]["objects"]) {
        return Err("the exiled card in this resolution needs its local exile receipt".into());
    }
    if super::symbol_name(value) == Some(MARKER) {
        let args = value["args"]
            .as_array_mut()
            .ok_or("linked literal arguments")?;
        if args.len() != 1 {
            return Err("linked literal needs one ObjectSet".into());
        }
        let inner = args[0].take();
        *value = inner;
        return strip_context(value, depth + 1, work, local_exile);
    }
    if let Some(instructions) = value.get_mut("instructions").and_then(Value::as_array_mut) {
        let mut local_exile = local_exile;
        for instruction in instructions {
            let produced = optional_exile(instruction);
            strip_context(instruction, depth + 1, work, local_exile)?;
            local_exile |= produced;
        }
        return Ok(());
    }
    match value {
        Value::Object(fields) => {
            for child in fields.values_mut() {
                strip_context(child, depth + 1, work, local_exile)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                strip_context(child, depth + 1, work, local_exile)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn empty_announcement(value: &Value) -> bool {
    value["args"]["announcement"] == json!({"targets":[],"modes":null})
}
fn hand_entry(value: &Value) -> bool {
    if value["kind"] != "Triggered"
        || value["args"]["event"]
            != json!({"ZoneChange":{"objects":{"kind":"SameObject","args":{"kind":"Source"}},"from":null,"to":"Battlefield"}})
        || !empty_announcement(value)
        || !value["args"]["intervening_if"].is_null()
        || value["args"]["limits"] != json!({"per_turn":null,"total":null})
    {
        return false;
    }
    let Some([optional]) = items(value) else {
        return false;
    };
    if optional["kind"] != "Optional" || optional["args"]["decider"] != controller() {
        return false;
    }
    let Some([choice, moved]) = items(optional) else {
        return false;
    };
    let (Some(chosen), Some(moved)) = (operation(choice), operation(moved)) else {
        return false;
    };
    let a = &chosen["args"];
    chosen["kind"] == "ChooseObjects"
        && a["player"] == controller()
        && a["objects"]["kind"] == "Zone"
        && a["objects"]["args"]["scope"]
            == json!({"Players":{"zone":"Hand","players":{"kind":"One","args":controller()}}})
        && a["objects"]["args"]["kind"] == "Card"
        && ["Exactly", "AsManyAsCan", "UpTo"]
            .iter()
            .any(|key| a["count"][*key] == json!({"kind":"Constant","args":1}))
        && a["constraints"] == json!([])
        && choice["args"]["result"] != optional["args"]["body"]["instructions"][1]["args"]["result"]
        && moved["kind"] == "Move"
        && super::movement::zone(&moved["args"]["destination"]) == Some("Exile")
        && moved["args"]["objects"] == json!({"kind":"Selected","args":choice["args"]["result"]})
}
fn mana_only(program: &Value) -> bool {
    let Some(items) = program["instructions"].as_array() else {
        return false;
    };
    items.iter().all(|item| match item["kind"].as_str() {
        Some("Effect") => item["args"]["operation"]["kind"] == "AddMana",
        Some("If") => mana_only(&item["args"]["then"]) && mana_only(&item["args"]["otherwise"]),
        _ => false,
    })
}
fn linked_entry(card: &Value, returned: usize) -> bool {
    let Some(abilities) = card.as_array() else {
        return false;
    };
    let mut found = false;
    for (index, value) in abilities.iter().enumerate() {
        if index == returned {
            continue;
        }
        let value = super::presentation::core(value);
        if hand_entry(value) && !found {
            found = true;
        } else if value["kind"] != "Activated"
            || value["args"]["origin"] != "Ordinary"
            || !cost(value, true)
            || !empty_announcement(value)
            || !mana_only(&value["args"]["body"])
        {
            return false;
        }
    }
    found
}
fn cost(value: &Value, linked: bool) -> bool {
    let Some(parts) = value["args"]["cost"]["parts"].as_array() else {
        return false;
    };
    !parts.is_empty()
        && parts.len() <= 32
        && parts.iter().all(|part| {
            part.get("Mana").is_some()
                || part == "TapSymbol"
                || !linked
                    && part["Sacrifice"]
                        == json!({"objects":{"kind":"One","args":{"kind":"Source"}},"choose":null})
        })
        && (!linked || parts.iter().any(|part| part == "TapSymbol"))
}
fn unique_results(body: &[Value], delay: &Value) -> bool {
    let mut ids = BTreeSet::new();
    body.iter()
        .chain(items(delay).unwrap_or_default())
        .all(|effect| {
            effect["kind"] == "Effect"
                && effect["args"]["result"]
                    .as_u64()
                    .is_some_and(|id| u32::try_from(id).is_ok() && ids.insert(id))
        })
}
fn actual_group(value: &Value, site: &Site) -> Option<Value> {
    let index = site.producer?;
    let body = items(value)?;
    if body[..index].iter().any(|effect| {
        !operation(effect).is_some_and(|op| {
            matches!(
                op["kind"].as_str(),
                Some("Draw" | "GainLife" | "LoseLife" | "Mill" | "SetTapped")
            )
        })
    }) {
        return None;
    }
    let producer = operation(body.get(index)?)?;
    let delay = operation(body.get(site.returned)?)?;
    if producer["kind"] != "Move"
        || super::movement::zone(&producer["args"]["destination"]) != Some("Exile")
        || !unique_results(body, delay)
        || !value["args"]["announcement"]["modes"].is_null()
    {
        return None;
    }
    let objects = &producer["args"]["objects"];
    let slot = if objects["kind"] == "One"
        && objects["args"]["kind"] == "Target"
        && objects["args"]["args"]["index"] == 0
    {
        objects["args"]["args"]["slot"].as_u64()?
    } else if objects["kind"] == "Targets" {
        objects["args"].as_u64()?
    } else {
        return None;
    };
    let specs = value["args"]["announcement"]["targets"].as_array()?;
    let mut targets = specs.iter().filter(|spec| spec["slot"] == slot);
    let target = targets.next()?;
    if targets.next().is_some()
        || target["kind"] != json!({"Object":{"kind":"Permanent","zones":["Battlefield"]}})
        || !["Exactly", "UpTo"]
            .iter()
            .any(|key| target["count"][*key] == json!({"kind":"Constant","args":1}))
    {
        return None;
    }
    Some(guarded(
        json!({"kind":"Transition","args":{"result":body[index]["args"]["result"],"phase":"After"}}),
    ))
}
fn direct_exile(instruction: &Value) -> bool {
    operation(instruction).is_some_and(|op| {
        op["kind"] == "Move" && super::movement::zone(&op["args"]["destination"]) == Some("Exile")
    })
}

fn entry_produces_link(value: &Value) -> bool {
    if value["kind"] != "Triggered"
        || value["args"]["event"]
            != json!({"ZoneChange":{
                "objects":{"kind":"SameObject","args":{"kind":"Source"}},"from":null,"to":"Battlefield"
            }})
    {
        return false;
    }
    let Some(body) = items(value) else {
        return false;
    };
    body.iter()
        .filter(|instruction| {
            direct_exile(instruction)
                || (instruction["kind"] == "Optional"
                    && instruction["args"]["decider"] == controller()
                    && items(instruction)
                        .is_some_and(|body| body.iter().filter(|i| direct_exile(i)).count() == 1))
        })
        .count()
        == 1
}

fn explicit_producer(value: &Value) -> bool {
    match value["kind"].as_str() {
        Some("Triggered") => entry_produces_link(value),
        Some("Mechanic") => value["args"]["mechanic"] == "Hideaway",
        Some("Casting") => value["args"]["AdditionalCost"]["cost"]["parts"]
            .as_array()
            .is_some_and(
                |parts| matches!(parts.as_slice(), [part] if part.get("BeholdAndExile").is_some()),
            ),
        _ => false,
    }
}

fn unambiguous_departure_producer(card: &Value) -> bool {
    card.as_array().is_some_and(|abilities| {
        abilities
            .iter()
            .filter(|ability| explicit_producer(super::presentation::core(ability)))
            .take(2)
            .count()
            == 1
    })
}

fn spell_exile_group(card: &Value, ability: &Value, site: &Site) -> Option<Value> {
    let movement = returned(card, site)?;
    if movement["args"]["destination"]["Battlefield"]["controller"] != "Owner"
        || movement["args"]["qualifiers"] != json!([])
        || !ability["args"]["announcement"]["modes"].is_null()
    {
        return None;
    }
    let body = items(ability)?;
    let delay = operation(body.get(site.returned)?)?;
    if !unique_results(body, delay) {
        return None;
    }
    let result = body.first()?["args"]["result"].as_u64()?;
    Some(guarded(
        json!({"kind":"Transition","args":{"result":result,"phase":"After"}}),
    ))
}

fn group(card: &Value, site: &Site, departure_producer: bool) -> Option<Value> {
    let value = super::presentation::core(card.get(site.ability)?);
    if site.plural {
        return spell_exile_group(card, value, site);
    }
    if value["kind"] == "Triggered" {
        return (departure_producer
            && source_leaves(&value["args"]["event"], false)
            && empty_announcement(value)
            && value["args"]["intervening_if"].is_null())
        .then(linked_zone);
    }
    if value["kind"] != "Activated"
        || value["args"]["origin"] != "Ordinary"
        || !cost(value, site.producer.is_none())
    {
        return None;
    }
    let returned = returned(card, site)?;
    if site.producer.is_none() {
        (items(value)?.len() == 1
            && empty_announcement(value)
            && returned["args"]["destination"].get("OwnersHand").is_some()
            && linked_entry(card, site.ability))
        .then(linked_zone)
    } else {
        (returned["args"]["destination"].get("Battlefield").is_some())
            .then(|| actual_group(value, site))
            .flatten()
    }
}
fn typed(card: &Value) -> Result<bool, String> {
    crate::functions::branch_bounds(card)?;
    Ok(serde_json::from_value::<Vec<Ability>>(card.clone()).is_ok())
}
fn objects_mut<'a>(card: &'a mut Value, site: &Site) -> Option<&'a mut Value> {
    let ability = super::presentation::core_mut(card.get_mut(site.ability)?);
    let items = ability["args"]["body"]["instructions"].as_array_mut()?;
    let mut instruction = items.get_mut(site.returned)?;
    if site.optional {
        instruction = instruction["args"]["body"]["instructions"].get_mut(0)?;
    }
    let op = &mut instruction["args"]["operation"];
    if site.producer.is_none() {
        Some(&mut op["args"]["objects"])
    } else {
        Some(&mut op["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"])
    }
}
/// Only pre-binding literal sites can receive either meaning after binding.
pub(crate) fn lower(card: &mut Value, sites: &[Site]) -> Result<(), String> {
    if sites.is_empty() || !typed(card)? {
        return Ok(());
    }
    let departure_producer = unambiguous_departure_producer(card);
    for site in sites {
        let Some(group) = group(card, site, departure_producer) else {
            continue;
        };
        let objects = &returned(card, site).ok_or("linked action return path")?["args"]["objects"];
        let literal = if site.plural {
            *objects == linked_zone()
        } else {
            *objects == scalar() || *objects == guarded(scalar())
        };
        if !literal {
            continue;
        }
        *objects_mut(card, site).ok_or("linked action mutation path")? = group;
    }
    Ok(())
}
/// Project only the same proved parent contexts, preserving the original view.
/// Forward whole-card verification must reconstruct these exact current operands.
pub(crate) fn projection(card: &Value) -> Result<Option<Value>, String> {
    let candidates = candidates(card)?;
    if candidates.is_empty() || !typed(card)? {
        return Ok(None);
    }
    let mut projected = None;
    let departure_producer = unambiguous_departure_producer(card);
    for site in candidates {
        let Some(group) = group(card, &site, departure_producer) else {
            continue;
        };
        if returned(card, &site).is_some_and(|op| op["args"]["objects"] == group) {
            let copy = projected.get_or_insert_with(|| card.clone());
            *objects_mut(copy, &site).ok_or("linked action projection path")? = if site.plural {
                linked_zone()
            } else if site.producer.is_some() || card[site.ability]["kind"] == "Triggered" {
                guarded(scalar())
            } else {
                scalar()
            };
        }
    }
    Ok(projected)
}

#[cfg(test)]
mod tests;
