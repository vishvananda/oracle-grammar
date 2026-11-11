//! Full-card text binds characteristic notes independently of native abilities.
use super::{Engine, Refusal, bind_card, canonical_abilities, validate_abilities};
use crate::model::ir::{Characteristics, ManaSymbol};
use crate::model::ir_schema::Schema;
use crate::parse::{Counters, Parser, RuleSet};
use crate::tokens::{self, Names};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod land_mana;
mod printing;
mod source_printing;
pub(crate) mod station;
mod surface;
pub use station::{CardInputContext, PrintedPowerToughness, StationAnimationBox};
use surface::CardSurface;

/// A reference into current characteristics, never a retained mana operand.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", content = "args", deny_unknown_fields)]
pub enum CardNote {
    ManaCost {
        slot: usize,
    },
    /// The mana granted by all current basic land types, with no copied colors.
    LandTypes,
    StationPowerToughness {
        ability: usize,
        stage: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardReading {
    pub abilities: Value,
    pub notes: Vec<CardNote>,
}

#[derive(Debug)]
pub struct ParsedCard {
    pub readings: Vec<CardReading>,
    pub counters: Counters,
}

pub(super) fn schema() -> Result<Schema, String> {
    let mut schema = Schema::of_ir()?;
    let surface = Schema::from_sources(&[include_str!("card/surface.rs")])?;
    for (name, definition) in surface.types {
        if schema.types.insert(name.clone(), definition).is_some() {
            return Err(format!("surface type shadows native IR: {name}"));
        }
    }
    Ok(schema)
}

impl CardReading {
    fn canonical(&self) -> Result<Self, String> {
        validate_abilities(&self.abilities)?;
        Ok(Self {
            abilities: canonical_abilities(&self.abilities)?,
            notes: self.notes.clone(),
        })
    }
}

impl Engine {
    pub fn parse_card(
        &self,
        name: &str,
        text: &str,
        context: &Characteristics,
    ) -> Result<ParsedCard, Refusal> {
        self.parse_card_without(name, text, context, &RuleSet::default())
    }

    pub fn parse_card_without(
        &self,
        name: &str,
        text: &str,
        context: &Characteristics,
        off: &RuleSet,
    ) -> Result<ParsedCard, Refusal> {
        self.parse_card_impl(name, text, context, None, off)
    }

    fn parse_card_impl(
        &self,
        name: &str,
        text: &str,
        context: &Characteristics,
        source: Option<&CardInputContext>,
        off: &RuleSet,
    ) -> Result<ParsedCard, Refusal> {
        // Check before tokenization/serialization, including externally supplied
        // vectors whose lengths do not contribute to the text's token budget.
        if text.len() > 65_536 || name.len() > 4096 {
            return Err(Refusal::Limit("card text byte limit".into()));
        }
        self.card_context(context).map_err(Refusal::Limit)?;
        if let Some(source) = source {
            let visible = self
                .card_input_context(name, text, context, source.printed_power_toughness.clone())
                .map_err(Refusal::Binding)?;
            if visible.station_animation.is_some()
                && visible.station_animation != source.station_animation
            {
                return Err(Refusal::Binding(
                    "visible Station reminder disagrees with supplied layout".into(),
                ));
            }
        }
        let root = self.card_root().map_err(Refusal::Binding)?;
        let names = Names::of(name);
        let tokens =
            tokens::tokenize(text, &names, &self.grammar.vocabulary).map_err(Refusal::Lexical)?;
        let mut card_off = off.clone();
        if let Some(wrapper) = self.grammar.names.get("line_station_explicit") {
            card_off.insert(*wrapper);
        }
        let mut parser = Parser::new(&self.grammar, &tokens, self.limits).without(&card_off);
        let values = parser.whole(root).map_err(Refusal::Limit)?;
        if values.is_empty() {
            return Err(Refusal::NoParse {
                furthest: parser.furthest,
                tokens: tokens.len(),
            });
        }
        let mut counters = parser.counters.clone();
        let mut readings = Vec::new();
        let mut failure = None;
        for value in values {
            match self.bind_card_notes(name, &value, context, source, &mut counters) {
                Ok(bound) => {
                    for reading in bound {
                        if !readings.contains(&reading) {
                            if readings.len() >= self.reading_limit {
                                return Err(Refusal::Limit("bound card reading limit".into()));
                            }
                            readings.push(reading);
                        }
                    }
                }
                Err(Refusal::Limit(why)) => return Err(Refusal::Limit(why)),
                Err(why) => failure = Some(why),
            }
        }
        if readings.is_empty() {
            return Err(failure.unwrap_or_else(|| Refusal::Binding("no card reading".into())));
        }
        Ok(ParsedCard { readings, counters })
    }

    pub fn stuck_card_at(&self, name: &str, text: &str) -> String {
        if text.len() > 65_536 || name.len() > 4096 {
            return String::new();
        }
        self.card_root()
            .map(|root| self.stuck_root(name, text, &RuleSet::default(), root))
            .unwrap_or_default()
    }

    fn card_root(&self) -> Result<u32, String> {
        self.grammar
            .names
            .get("card_text")
            .copied()
            .ok_or("no card_text rule".into())
    }

    fn card_context(&self, context: &Characteristics) -> Result<Vec<String>, String> {
        let cost = context.mana_cost.as_deref().unwrap_or_default();
        if [
            cost.len(),
            context.subtypes.len(),
            context.supertypes.len(),
            context.card_types.len(),
        ]
        .iter()
        .any(|n| *n > self.limits.tokens)
            || cost.iter().fold(cost.len(), |size, s| {
                size.saturating_add(match s {
                    ManaSymbol::Phyrexian(colors) => colors.len(),
                    _ => 0,
                })
            }) > self.limits.tokens
        {
            return Err("card context size limit".into());
        }
        Ok(crate::corpus::object_print_context(&json!({
            "supertypes":context.supertypes, "subtypes":context.subtypes,
            "card_types":context.card_types
        })))
    }

    fn bind_card_notes(
        &self,
        name: &str,
        value: &Value,
        context: &Characteristics,
        source: Option<&CardInputContext>,
        counters: &mut Counters,
    ) -> Result<Vec<CardReading>, Refusal> {
        let mut value = value.clone();
        let station_notes = station::materialize(&mut value, source).map_err(Refusal::Binding)?;
        value["abilities"] = bind_card(&value["abilities"], name).map_err(Refusal::Binding)?;
        let station_notes = if station_notes.is_empty() {
            station_notes
        } else {
            let mut bound = json!({"abilities":value["abilities"]});
            station::materialize(&mut bound, source).map_err(Refusal::Binding)?
        };
        let surface: CardSurface = serde_json::from_value(value)
            .map_err(|why| Refusal::Binding(format!("typed card surface: {why}")))?;
        if surface.notes.len() + usize::from(surface.land_mana.is_some()) > self.limits.depth {
            return Err(Refusal::Limit("card note limit".into()));
        }
        let cost: Vec<Value> = context
            .mana_cost
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|symbol| serde_json::to_value(symbol).expect("native mana symbol serializes"))
            .collect();
        let mut choices = vec![station_notes];
        for note in surface.notes {
            let note = serde_json::to_value(note).map_err(|e| Refusal::Binding(e.to_string()))?;
            let mut slots = Vec::new();
            for (slot, symbol) in cost.iter().enumerate() {
                counters.steps += 1;
                if counters.steps > self.limits.steps {
                    return Err(Refusal::Limit("card note work limit".into()));
                }
                if *symbol == note {
                    slots.push(CardNote::ManaCost { slot });
                }
            }
            if slots.is_empty() {
                return Err(Refusal::Binding(
                    "mana explanation has no matching mana-cost slot".into(),
                ));
            }
            let mut next = Vec::new();
            for prior in &choices {
                for slot in &slots {
                    if next.len() >= self.reading_limit {
                        return Err(Refusal::Limit("card note candidate limit".into()));
                    }
                    counters.steps = counters.steps.saturating_add(prior.len() as u64 + 1);
                    if counters.steps > self.limits.steps {
                        return Err(Refusal::Limit("card note work limit".into()));
                    }
                    let mut notes = prior.clone();
                    notes.push(slot.clone());
                    next.push(notes);
                }
            }
            choices = next;
        }
        if let Some(header) = &surface.land_mana {
            let colors = &header.colors;
            counters.steps = counters.steps.saturating_add(
                (context.subtypes.len() + context.card_types.len() + colors.len()) as u64,
            );
            if counters.steps > self.limits.steps {
                return Err(Refusal::Limit("land mana note work limit".into()));
            }
            let expected = land_mana::colors(context).map_err(Refusal::Binding)?;
            if *colors != expected {
                return Err(Refusal::Binding(
                    "mana header disagrees with the current basic land types".into(),
                ));
            }
            for notes in &mut choices {
                notes.push(CardNote::LandTypes);
            }
        }
        let abilities =
            serde_json::to_value(surface.abilities).map_err(|e| Refusal::Binding(e.to_string()))?;
        Ok(choices
            .into_iter()
            .map(|notes| CardReading {
                abilities: abilities.clone(),
                notes,
            })
            .collect())
    }

    fn card_surfaces_inner(
        &self,
        reading: &CardReading,
        context: &Characteristics,
    ) -> Result<Vec<Value>, String> {
        self.card_context(context)?;
        if reading.notes.len() > self.limits.depth {
            return Err("card note limit".into());
        }
        let mut notes = Vec::new();
        let mut land_colors = None;
        for note in &reading.notes {
            match note {
                CardNote::ManaCost { slot } => {
                    let symbol = context
                        .mana_cost
                        .as_ref()
                        .and_then(|cost| cost.get(*slot))
                        .ok_or("mana explanation references a missing mana-cost slot")?;
                    notes.push(serde_json::to_value(symbol).map_err(|e| e.to_string())?);
                }
                CardNote::StationPowerToughness { .. } => {}
                CardNote::LandTypes => {
                    if land_colors.is_some() {
                        return Err("duplicate inherent mana note".into());
                    }
                    land_colors = Some(land_mana::colors(context)?);
                }
            }
        }
        let land_mana = land_colors.map(|colors| json!({"colors":colors}));
        Ok(Self::surfaces(&reading.abilities)?
            .into_iter()
            .map(|abilities| json!({"notes":notes,"land_mana":land_mana,"station":false,"station_explicit":false,"station_declaration":null,"abilities":abilities}))
            .collect())
    }
    fn card_surfaces(
        &self,
        reading: &CardReading,
        context: &Characteristics,
    ) -> Result<Vec<Value>, String> {
        station::checked_surfaces(reading, None, self.card_surfaces_inner(reading, context)?)
    }
    fn card_surfaces_with_source(
        &self,
        reading: &CardReading,
        context: &CardInputContext,
    ) -> Result<Vec<Value>, String> {
        station::checked_surfaces(
            reading,
            Some(context),
            self.card_surfaces_inner(reading, &context.characteristics)?,
        )
    }
}
