//! Printed Station boxes are source inputs, separate from unconditional characteristics.
use super::{CardNote, CardReading, Characteristics, Engine};
use crate::parse::{Parser, RuleSet};
use crate::tokens::{self, Names};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StationAnimationBox {
    pub minimum: u32,
    pub power: i32,
    pub toughness: i32,
}

/// Availability of original printed fields is distinct from an absent box.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum PrintedPowerToughness {
    Unavailable,
    Absent,
    Known { power: i32, toughness: i32 },
    Unsupported,
}
impl PrintedPowerToughness {
    pub fn values(&self) -> Option<(i32, i32)> {
        match *self {
            Self::Known { power, toughness } => Some((power, toughness)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardInputContext {
    pub characteristics: Characteristics,
    pub printed_power_toughness: PrintedPowerToughness,
    pub station_nonanimating: bool,
    pub station_animation: Option<StationAnimationBox>,
    pub station_present: bool,
}

impl Engine {
    /// Read layout evidence with the same EBNF recognizer as card text. Values
    /// come from the caller's original printed fields, never reference abilities.
    pub fn card_input_context(
        &self,
        name: &str,
        raw: &str,
        characteristics: &Characteristics,
        printed_power_toughness: PrintedPowerToughness,
    ) -> Result<CardInputContext, String> {
        if raw.len() > 65_536 || name.len() > 4096 {
            return Err("card source byte limit".into());
        }
        let root = *self
            .grammar
            .names
            .get("station_source")
            .ok_or("no station_source rule")?;
        let mut station_animation = None;
        let mut station_present = false;
        let mut names = None;
        let anchored = anchored_source(&self.grammar, root, 0);
        let mut quoted = false;
        let mut work = 0u64;
        let mut unverified_quotes = false;
        for line in raw.lines() {
            let head = line.trim_start();
            let candidate = head
                .get(..7)
                .is_some_and(|word| word.eq_ignore_ascii_case("station"));
            // This shortcut is enabled only after checking every grammar start.
            if anchored && !candidate && !line.contains('"') {
                continue;
            }
            if unverified_quotes && (!anchored || candidate) {
                return Err("cannot verify Station source across an unlexable quoted line".into());
            }
            let names = names.get_or_insert_with(|| Names::of(name));
            let tokens = match tokens::tokenize(line, names, &self.grammar.vocabulary) {
                Ok(tokens) => tokens,
                Err(_) if line.contains('"') => {
                    if station_present || candidate {
                        return Err("cannot verify quoted Station source".into());
                    }
                    unverified_quotes = true;
                    continue;
                }
                Err(_) => continue,
            };
            let inside = quoted;
            for token in &tokens {
                if token == &tokens::Token::Mark('"') {
                    quoted = !quoted;
                }
            }
            if inside || anchored && !candidate {
                continue;
            }
            let mut parser = Parser::new(&self.grammar, &tokens, self.limits);
            let values = parser.whole(root)?;
            work = work.saturating_add(parser.counters.steps);
            if work > self.limits.steps {
                return Err("Station source work limit".into());
            }
            for value in values {
                station_present = true;
                if value["minimum"].is_null() {
                    continue;
                }
                let minimum = value["minimum"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or("station source minimum")?;
                let (power, toughness) = printed_power_toughness
                    .values()
                    .ok_or("Station animation requires numeric printed P/T")?;
                let declared = StationAnimationBox {
                    minimum,
                    power,
                    toughness,
                };
                if station_animation
                    .as_ref()
                    .is_some_and(|prior| prior != &declared)
                {
                    return Err("conflicting Station animation declarations".into());
                }
                station_animation = Some(declared);
            }
        }
        Ok(CardInputContext {
            characteristics: characteristics.clone(),
            station_nonanimating: station_present
                && printed_power_toughness == PrintedPowerToughness::Absent,
            printed_power_toughness,
            station_animation,
            station_present,
        })
    }

    pub fn parse_card_with_source(
        &self,
        name: &str,
        text: &str,
        context: &CardInputContext,
    ) -> Result<super::ParsedCard, super::Refusal> {
        self.parse_card_without_source(name, text, context, &RuleSet::default())
    }

    pub fn parse_card_without_source(
        &self,
        name: &str,
        text: &str,
        context: &CardInputContext,
        off: &RuleSet,
    ) -> Result<super::ParsedCard, super::Refusal> {
        self.parse_card_impl(name, text, &context.characteristics, Some(context), off)
    }
}

pub(super) fn station_stage(ability: &Value) -> bool {
    ability["kind"] == "CounterStages"
        && ability["args"]["counter"] == json!({"Named":"ChargeCounter"})
}

pub(super) fn materialize(
    surface: &mut Value,
    context: Option<&CardInputContext>,
) -> Result<Vec<CardNote>, String> {
    let contextual = surface["station"].as_bool().unwrap_or(false);
    let explicit = surface["station_explicit"].as_bool().unwrap_or(false);
    let abilities = surface["abilities"]
        .as_array_mut()
        .ok_or("card abilities")?;
    let has_stages = abilities.iter().any(station_stage);
    let top_count = abilities.iter().filter(|a| station_stage(a)).count();
    if abilities.iter().map(stage_count).sum::<usize>() != top_count {
        return Err("a nested Station cannot borrow its source card's printed box".into());
    }
    if explicit && top_count != 1 {
        return Err("the explicit Station surface requires its one spelled box".into());
    }
    let mut notes = Vec::new();
    let declared = context.and_then(|c| c.station_animation.as_ref());
    if context.is_some_and(|c| {
        c.station_nonanimating
            && (c.printed_power_toughness != PrintedPowerToughness::Absent
                || c.station_animation.is_some())
    }) {
        return Err("nonanimating Station context conflicts with its printed source".into());
    }
    if let Some(box_) = declared
        && context.and_then(|c| c.printed_power_toughness.values())
            != Some((box_.power, box_.toughness))
    {
        return Err("Station declaration disagrees with printed P/T".into());
    }
    if contextual
        && declared.is_none()
        && context
            .and_then(|c| c.printed_power_toughness.values())
            .is_some()
    {
        return Err("omitted Station P/T requires explicit source layout context".into());
    }
    if contextual && context.is_none() {
        return Err("Station context is missing".into());
    }
    if has_stages && !explicit {
        let c = context.ok_or("Station stages require available source context")?;
        if !c.station_present
            || c.printed_power_toughness.values().is_none() && !c.station_nonanimating
        {
            return Err("Station box presence is unavailable".into());
        }
        if c.station_nonanimating
            && (c.printed_power_toughness != PrintedPowerToughness::Absent || declared.is_some())
        {
            return Err("nonanimating Station declaration conflicts with printed source".into());
        }
    }
    let mut matched = 0;
    for (ability, value) in abilities.iter_mut().enumerate() {
        if !station_stage(value) {
            continue;
        }
        let stages = value["args"]["stages"]
            .as_array_mut()
            .ok_or("Station stages")?;
        for (stage, row) in stages.iter_mut().enumerate() {
            let selected =
                declared.is_some_and(|b| row["minimum"].as_u64() == Some(u64::from(b.minimum)));
            if contextual && selected {
                let b = declared.unwrap();
                row["power_toughness"] = json!([b.power, b.toughness]);
                row["animates"] = json!(true);
            }
            if !row["animates"].as_bool().unwrap_or(false) {
                continue;
            }
            if explicit && context.is_none() {
                continue;
            }
            let context = context.ok_or("Station animation requires printed P/T context")?;
            let (power, toughness) = context
                .printed_power_toughness
                .values()
                .ok_or("Station animation requires numeric printed P/T")?;
            if row["power_toughness"] != json!([power, toughness])
                || declared.is_some() && !selected
            {
                return Err("Station stage disagrees with current printed box".into());
            }
            matched += 1;
            if !explicit {
                notes.push(CardNote::StationPowerToughness { ability, stage });
            }
        }
    }
    if has_stages
        && context.is_some_and(|c| c.printed_power_toughness.values().is_some())
        && !explicit
        && matched != 1
    {
        return Err("printed Station box requires one legacy animation row".into());
    }
    if declared.is_none() && matched > 1 {
        return Err("ambiguous legacy Station box association".into());
    }
    if declared.is_some() && matched != 1 {
        return Err("Station source box requires one matching animation stage".into());
    }
    Ok(notes)
}

pub(super) fn checked_surfaces(
    reading: &CardReading,
    context: Option<&CardInputContext>,
    mut surfaces: Vec<Value>,
) -> Result<Vec<Value>, String> {
    if requires_context(&reading.abilities) {
        let mut check = json!({"abilities":reading.abilities,"station":false});
        materialize(&mut check, context)?;
    }
    for note in &reading.notes {
        let CardNote::StationPowerToughness { ability, stage } = *note else {
            continue;
        };
        let c = context.ok_or("Station printing requires current source context")?;
        let (p, t) = c
            .printed_power_toughness
            .values()
            .ok_or("Station printing requires printed P/T")?;
        let a = &reading.abilities[ability];
        let s = &a["args"]["stages"][stage];
        if !station_stage(a)
            || s["animates"] != true
            || s["power_toughness"] != json!([p, t])
            || c.station_animation
                .as_ref()
                .is_some_and(|b| s["minimum"] != b.minimum || (b.power, b.toughness) != (p, t))
        {
            return Err("Station note disagrees with current stage or printed context".into());
        }
    }
    if requires_context(&reading.abilities)
        && !reading
            .notes
            .iter()
            .any(|n| matches!(n, CardNote::StationPowerToughness { .. }))
    {
        let mut explicit = surfaces.clone();
        for surface in &mut explicit {
            surface["station_explicit"] = json!(true);
        }
        surfaces.extend(explicit);
    }
    if context.is_some_and(|c| c.station_present) {
        let mut contextual = surfaces.clone();
        for surface in &mut contextual {
            surface["station"] = json!(true);
            let reminded = reading.abilities.as_array().is_some_and(|abilities| {
                abilities.iter().any(|a| {
                    a["kind"] == "WithPresentation"
                        && a["args"]["reminder"] == true
                        && a["args"]["ability"]["args"]["mechanic"] == "Station"
                })
            });
            if reminded {
                surface["station_declaration"] = json!({"minimum":context.and_then(|c|c.station_animation.as_ref()).map(|b|b.minimum)});
            }
            for note in &reading.notes {
                if let CardNote::StationPowerToughness { ability, stage } = *note {
                    surface["abilities"][ability]["args"]["stages"][stage]["power_toughness"] =
                        Value::Null;
                    surface["abilities"][ability]["args"]["stages"][stage]["animates"] =
                        json!(false);
                }
            }
        }
        surfaces.extend(contextual);
    }
    Ok(surfaces)
}

pub(crate) fn requires_context(value: &Value) -> bool {
    if station_stage(value) {
        return true;
    }
    match value {
        Value::Array(items) => items.iter().any(requires_context),
        Value::Object(fields) => fields.values().any(requires_context),
        _ => false,
    }
}

pub(crate) fn empty_characteristics() -> Characteristics {
    Characteristics {
        supertypes: vec![],
        card_types: vec![],
        subtypes: vec![],
        mana_cost: None,
        color_indicator: vec![],
        power_toughness: None,
        loyalty: None,
        defense: None,
    }
}

fn anchored_source(grammar: &crate::grammar::Grammar, root: u32, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    grammar.rules[root as usize]
        .alternatives
        .iter()
        .all(|alternative| match alternative.syms.first() {
            Some(crate::grammar::Sym::Word(word)) => word == "station",
            Some(crate::grammar::Sym::Rule(next)) => anchored_source(grammar, *next, depth + 1),
            _ => false,
        })
}

fn stage_count(value: &Value) -> usize {
    let own = usize::from(station_stage(value));
    own + match value {
        Value::Array(items) => items.iter().map(stage_count).sum::<usize>(),
        Value::Object(fields) => fields.values().map(stage_count).sum::<usize>(),
        _ => 0,
    }
}
