//! Full-card text binds characteristic notes independently of native abilities.
use super::{
    Engine, Refusal, bind_card_with_source, canonical_abilities, permanent_spell_source,
    validate_abilities,
};
use crate::model::ir::{Ability, Characteristics, ManaSymbol, Mechanic};
use crate::model::ir_schema::Schema;
use crate::model::mtg_types::{CardType, SubType, SuperType};
use crate::parse::{Counters, Parser, RuleSet};
use crate::tokens::Names;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod land_mana;
mod printed_stats;
mod printing;
mod source_printing;
pub(crate) mod station;
mod surface;
pub use printed_stats::{PrintedPowerToughness, PrintedStat};
pub use station::{CardInputContext, StationAnimationBox};
pub use surface::{AdventureReturnType, LayoutReminder};
use surface::{
    CardSurface, CountedPowerExample, LandMana, MonocoloredHybridNote, check_compleated,
    check_land_play_cost, check_no_mana_cost, check_retained_artifact_type, copy_token_note_types,
    creature_note_types, example_toughness, fixed_identity_color, printed_mana_value,
};

/// A reference into current characteristics, never a retained mana operand.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", content = "args", deny_unknown_fields)]
pub enum CardNote {
    ManaCost {
        slot: usize,
    },
    AssistLimit {
        slot: usize,
    },
    MonocoloredHybrid {
        slot: usize,
    },
    Compleated {
        slot: usize,
    },
    /// The mana granted by all current basic land types, with no copied colors.
    LandTypes,
    LandCreatureRules,
    RetainsArtifactType,
    NoManaCost,
    TransmuteManaValue,
    DeckIdentityColor,
    CountedPowerExample {
        count: u32,
    },
    Prowl,
    Amplify,
    CopyTokenTypes,
    LayoutReminder(LayoutReminder),
    TransformsFrom,
    MeldWith(String),
    StationPowerToughness {
        ability: usize,
        stage: usize,
    },
}

impl LayoutReminder {
    fn check(self, context: &Characteristics) -> Result<(), String> {
        let is_spell = || {
            context
                .card_types
                .iter()
                .any(|kind| matches!(kind, CardType::Instant | CardType::Sorcery))
        };
        let matches = match self {
            Self::Adventure(_) => {
                context
                    .subtypes
                    .iter()
                    .any(|ty| matches!(ty, SubType::Adventure))
                    && is_spell()
            }
            Self::Omen => {
                context
                    .subtypes
                    .iter()
                    .any(|ty| matches!(ty, SubType::Omen))
                    && is_spell()
            }
            Self::Room => {
                context
                    .subtypes
                    .iter()
                    .any(|ty| matches!(ty, SubType::Room))
                    && context
                        .card_types
                        .iter()
                        .any(|ty| matches!(ty, CardType::Enchantment))
            }
            Self::Siege => {
                context
                    .subtypes
                    .iter()
                    .any(|ty| matches!(ty, SubType::Siege))
                    && context
                        .card_types
                        .iter()
                        .any(|ty| matches!(ty, CardType::Battle))
                    && context.defense.is_some_and(|defense| defense >= 0)
            }
            Self::OngoingScheme => {
                context
                    .card_types
                    .iter()
                    .any(|ty| matches!(ty, CardType::Scheme))
                    && context
                        .supertypes
                        .iter()
                        .any(|ty| matches!(ty, SuperType::Ongoing))
            }
            Self::FaceUpConspiracy => context
                .card_types
                .iter()
                .any(|ty| matches!(ty, CardType::Conspiracy)),
            Self::LegendarySorcery | Self::LegendaryInstant => {
                context.card_types.iter().any(|ty| {
                    matches!(
                        (self, ty),
                        (Self::LegendarySorcery, CardType::Sorcery)
                            | (Self::LegendaryInstant, CardType::Instant)
                    )
                }) && context
                    .supertypes
                    .iter()
                    .any(|ty| matches!(ty, SuperType::Legendary))
            }
        };
        matches
            .then_some(())
            .ok_or_else(|| "layout reminder disagrees with the face's type line".into())
    }
}

fn hidden_agenda(ability: &Ability) -> bool {
    let ability = match ability {
        Ability::WithPresentation { ability, .. } => ability.as_ref(),
        _ => ability,
    };
    matches!(
        ability,
        Ability::Mechanic {
            mechanic: Mechanic::HiddenAgenda | Mechanic::DoubleAgenda,
            ..
        }
    )
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

fn bind_source_abilities(
    value: &mut Value,
    name: &str,
    context: &Characteristics,
    source: Option<&CardInputContext>,
    permanent_source: bool,
) -> Result<Vec<CardNote>, Refusal> {
    let notes = station::materialize(value, source).map_err(Refusal::Binding)?;
    value["abilities"] = bind_card_with_source(&value["abilities"], name, permanent_source)
        .map_err(Refusal::Binding)?;
    crate::bind::validate_source_payments(&value["abilities"], context.mana_cost.as_deref())
        .map_err(Refusal::Binding)?;
    if notes.is_empty() {
        return Ok(notes);
    }
    let mut bound = json!({"abilities":value["abilities"]});
    station::materialize(&mut bound, source).map_err(Refusal::Binding)
}

fn subtype_reminder_types(
    context: &Characteristics,
    note: &CardNote,
) -> Result<Vec<Value>, String> {
    match note {
        CardNote::CopyTokenTypes => copy_token_note_types(context),
        CardNote::Amplify => creature_note_types(context, false),
        CardNote::Prowl => creature_note_types(context, true),
        _ => Err("expected a creature subtype reminder".into()),
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
        let types = self.card_context(context).map_err(Refusal::Limit)?;
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
        let names = Names::for_card(name, &types);
        let tokens = self
            .tokenize_oracle(text, &names)
            .map_err(Refusal::Lexical)?;
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
        let permanent_source = permanent_spell_source(&types);
        for value in values {
            match self.bind_card_notes(
                name,
                &value,
                context,
                source,
                &mut counters,
                permanent_source,
            ) {
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
            context.color_indicator.len(),
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
        permanent_source: bool,
    ) -> Result<Vec<CardReading>, Refusal> {
        let mut value = value.clone();
        let station_notes =
            bind_source_abilities(&mut value, name, context, source, permanent_source)?;
        let surface = CardSurface::decode(value, context).map_err(Refusal::Binding)?;
        if let Some(source) = source {
            source
                .printed_power_toughness
                .check_definition(context, &surface.abilities)
                .map_err(Refusal::Binding)?;
        }
        if surface.note_count() > self.limits.depth {
            return Err(Refusal::Limit("card note limit".into()));
        }
        let characteristic_notes = surface
            .characteristic_notes(context, source)
            .map_err(Refusal::Binding)?;
        let mana_cost = context.mana_cost.as_deref().unwrap_or_default();
        if let Some(note) = &surface.monocolored_hybrid
            && (!matches!(note.symbol, ManaSymbol::MonocoloredHybrid(_))
                || printed_mana_value(mana_cost).map_err(Refusal::Binding)? != note.mana_value)
        {
            return Err(Refusal::Binding(
                "hybrid explanation disagrees with printed mana value".into(),
            ));
        }
        if let Some(symbol) = &surface.compleated_symbol {
            check_compleated(context, symbol).map_err(Refusal::Binding)?;
        }
        let cost: Vec<Value> = mana_cost
            .iter()
            .map(|symbol| serde_json::to_value(symbol).expect("native mana symbol serializes"))
            .collect();
        let mut choices = vec![station_notes];
        type NoteFromSlot = fn(usize) -> CardNote;
        let mana_notes = surface
            .notes
            .into_iter()
            .map(|symbol| (symbol, (|slot| CardNote::ManaCost { slot }) as NoteFromSlot))
            .chain(surface.assist_limit.into_iter().map(|symbol| {
                (
                    symbol,
                    (|slot| CardNote::AssistLimit { slot }) as NoteFromSlot,
                )
            }))
            .chain(surface.monocolored_hybrid.into_iter().map(|note| {
                (
                    note.symbol,
                    (|slot| CardNote::MonocoloredHybrid { slot }) as NoteFromSlot,
                )
            }))
            .chain(surface.compleated_symbol.into_iter().map(|symbol| {
                (
                    symbol,
                    (|slot| CardNote::Compleated { slot }) as NoteFromSlot,
                )
            }));
        for (note, from_slot) in mana_notes {
            let note = serde_json::to_value(note).map_err(|e| Refusal::Binding(e.to_string()))?;
            let mut slots = Vec::new();
            for (slot, symbol) in cost.iter().enumerate() {
                counters.steps += 1;
                if counters.steps > self.limits.steps {
                    return Err(Refusal::Limit("card note work limit".into()));
                }
                if *symbol == note {
                    slots.push(from_slot(slot));
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
        for (written, note) in [
            (surface.prowl_types, CardNote::Prowl),
            (surface.amplify_types, CardNote::Amplify),
            (surface.copy_token_types, CardNote::CopyTokenTypes),
        ] {
            let Some(written) = written else { continue };
            let expected = subtype_reminder_types(context, &note).map_err(Refusal::Binding)?;
            let written = serde_json::to_value(written)
                .map_err(|error| Refusal::Binding(error.to_string()))?;
            let written = written
                .as_array()
                .ok_or_else(|| Refusal::Binding("creature type list".into()))?;
            if expected.len() != written.len()
                || expected.iter().any(|kind| !written.contains(kind))
            {
                return Err(Refusal::Binding(
                    "reminder disagrees with the source creature types".into(),
                ));
            }
            for notes in &mut choices {
                notes.push(note.clone());
            }
        }
        for notes in &mut choices {
            notes.extend(characteristic_notes.iter().cloned());
        }
        if surface.no_mana_cost {
            check_no_mana_cost(context).map_err(Refusal::Binding)?;
            for notes in &mut choices {
                notes.push(CardNote::NoManaCost);
            }
        }
        if let Some(header) = &surface.land_mana {
            let colors = &header.colors;
            counters.steps = counters.steps.saturating_add(
                (context.subtypes.len() + context.card_types.len() + colors.len()) as u64,
            );
            if counters.steps > self.limits.steps {
                return Err(Refusal::Limit("land mana note work limit".into()));
            }
            let expected =
                land_mana::colors(context, header.creature_rules).map_err(Refusal::Binding)?;
            if *colors != expected {
                return Err(Refusal::Binding(
                    "mana header disagrees with the current basic land types".into(),
                ));
            }
            for notes in &mut choices {
                notes.push(if header.creature_rules {
                    CardNote::LandCreatureRules
                } else {
                    CardNote::LandTypes
                });
            }
        }
        if surface.retains_artifact_type {
            check_retained_artifact_type(context).map_err(Refusal::Binding)?;
            for notes in &mut choices {
                notes.push(CardNote::RetainsArtifactType);
            }
        }
        if let Some(reminder) = surface.layout_reminder {
            reminder.check(context).map_err(Refusal::Binding)?;
            if reminder == LayoutReminder::FaceUpConspiracy
                && surface.abilities.iter().any(hidden_agenda)
            {
                return Err(Refusal::Binding(
                    "a conspiracy with an agenda starts face down".into(),
                ));
            }
            for notes in &mut choices {
                notes.push(CardNote::LayoutReminder(reminder));
            }
        }
        if let Some(front) = surface.transforms_from {
            let expected = source.and_then(|context| context.transform_front.as_deref());
            if Some(front.as_str()) != expected {
                return Err(Refusal::Binding(
                    "transform reminder disagrees with the original front face".into(),
                ));
            }
            for notes in &mut choices {
                notes.push(CardNote::TransformsFrom);
            }
        }
        if let Some(partner) = surface.meld_with {
            if partner.is_empty() || partner.len() > 4096 {
                return Err(Refusal::Binding("invalid meld partner name".into()));
            }
            for notes in &mut choices {
                notes.push(CardNote::MeldWith(partner.clone()));
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
        transform_front: Option<&str>,
        printed_toughness: Option<i32>,
        written: bool,
    ) -> Result<Vec<Value>, String> {
        self.card_context(context)?;
        let has_land_play_cost = reading.abilities.as_array().is_some_and(|abilities| {
            abilities
                .iter()
                .any(|ability| ability.get("kind").and_then(Value::as_str) == Some("LandPlayCost"))
        });
        check_land_play_cost(context, has_land_play_cost)?;
        if reading.notes.len() > self.limits.depth {
            return Err("card note limit".into());
        }
        let mut notes = Vec::new();
        let mut assist_limit = None;
        let mut no_mana_cost = false;
        let mut monocolored_hybrid = None;
        let mut transmute_mana_value = None;
        let mut deck_identity_color = None;
        let mut counted_power_example = None;
        let mut land_mana = None;
        let mut retains_artifact_type = false;
        let mut layout_reminder = None;
        let mut transforms_from = None;
        let mut meld_with = None;
        let mut prowl = None;
        let mut amplify = None;
        let mut copy_token = None;
        let mut compleated = None;
        for note in &reading.notes {
            match note {
                CardNote::ManaCost { slot } | CardNote::AssistLimit { slot } => {
                    let symbol = context
                        .mana_cost
                        .as_ref()
                        .and_then(|cost| cost.get(*slot))
                        .ok_or("mana explanation references a missing mana-cost slot")?;
                    let symbol = serde_json::to_value(symbol).map_err(|e| e.to_string())?;
                    if matches!(note, CardNote::AssistLimit { .. }) {
                        if assist_limit.replace(symbol).is_some() {
                            return Err("duplicate Assist reminder".into());
                        }
                    } else {
                        notes.push(symbol);
                    }
                }
                CardNote::StationPowerToughness { .. } => {}
                CardNote::Compleated { slot } => {
                    let symbol = context
                        .mana_cost
                        .as_deref()
                        .unwrap_or_default()
                        .get(*slot)
                        .ok_or("Compleated reminder references a missing mana-cost slot")?;
                    check_compleated(context, symbol)?;
                    if compleated.replace(symbol).is_some() {
                        return Err("duplicate Compleated reminder".into());
                    }
                }
                CardNote::MonocoloredHybrid { slot } => {
                    let cost = context.mana_cost.as_deref().unwrap_or_default();
                    let Some(symbol @ ManaSymbol::MonocoloredHybrid(_)) = cost.get(*slot) else {
                        return Err(
                            "hybrid explanation needs a monocolored hybrid mana-cost slot".into(),
                        );
                    };
                    let note = MonocoloredHybridNote {
                        symbol: symbol.clone(),
                        mana_value: printed_mana_value(cost)?,
                    };
                    if monocolored_hybrid.replace(note).is_some() {
                        return Err("duplicate monocolored hybrid reminder".into());
                    }
                }
                CardNote::DeckIdentityColor => {
                    if deck_identity_color
                        .replace(fixed_identity_color(context)?)
                        .is_some()
                    {
                        return Err("duplicate deck identity reminder".into());
                    }
                }
                CardNote::CountedPowerExample { count } => {
                    let example = CountedPowerExample {
                        count: crate::model::ir::Expr::Constant(i64::from(*count)),
                        toughness: example_toughness(printed_toughness)?,
                    };
                    if counted_power_example.replace(example).is_some() {
                        return Err("duplicate power example".into());
                    }
                }
                CardNote::TransmuteManaValue => {
                    let cost = context.mana_cost.as_deref().unwrap_or_default();
                    if transmute_mana_value
                        .replace(printed_mana_value(cost)?)
                        .is_some()
                    {
                        return Err("duplicate Transmute mana value reminder".into());
                    }
                }
                CardNote::NoManaCost => {
                    check_no_mana_cost(context)?;
                    if std::mem::replace(&mut no_mana_cost, true) {
                        return Err("duplicate no-mana-cost reminder".into());
                    }
                }
                CardNote::LandTypes | CardNote::LandCreatureRules => {
                    if land_mana.is_some() {
                        return Err("duplicate inherent mana note".into());
                    }
                    let creature_rules = matches!(note, CardNote::LandCreatureRules);
                    land_mana = Some(LandMana {
                        colors: land_mana::colors(context, creature_rules)?,
                        creature_rules,
                    });
                }
                CardNote::RetainsArtifactType => {
                    check_retained_artifact_type(context)?;
                    if std::mem::replace(&mut retains_artifact_type, true) {
                        return Err("duplicate artifact type reminder".into());
                    }
                }
                CardNote::Prowl | CardNote::Amplify | CardNote::CopyTokenTypes => {
                    let slot = match note {
                        CardNote::Prowl => &mut prowl,
                        CardNote::Amplify => &mut amplify,
                        _ => &mut copy_token,
                    };
                    if slot
                        .replace(subtype_reminder_types(context, note)?)
                        .is_some()
                    {
                        return Err("duplicate subtype reminder".into());
                    }
                }
                CardNote::LayoutReminder(reminder) => {
                    reminder.check(context)?;
                    if layout_reminder.replace(*reminder).is_some() {
                        return Err("duplicate layout reminder".into());
                    }
                }
                CardNote::MeldWith(partner) => {
                    if partner.is_empty() || partner.len() > 4096 {
                        return Err("invalid meld partner name".into());
                    }
                    if meld_with.replace(partner).is_some() {
                        return Err("duplicate meld reminder".into());
                    }
                }
                CardNote::TransformsFrom => {
                    let front = transform_front
                        .filter(|name| !name.is_empty() && name.len() <= 4096)
                        .ok_or("transform reminder needs the original front face name")?;
                    if transforms_from.replace(front).is_some() {
                        return Err("duplicate transform reminder".into());
                    }
                }
            }
        }
        Ok(Self::native_surfaces(&reading.abilities, written)?
            .into_iter()
            .map(|abilities| json!({"notes":notes,"assist_limit":assist_limit,"no_mana_cost":no_mana_cost,"monocolored_hybrid":monocolored_hybrid,"transmute_mana_value":transmute_mana_value,"deck_identity_color":deck_identity_color,"counted_power_example":counted_power_example,"compleated_symbol":compleated,"prowl_types":prowl,"amplify_types":amplify,"copy_token_types":copy_token,"land_mana":land_mana,"retains_artifact_type":retains_artifact_type,"layout_reminder":layout_reminder,"transforms_from":transforms_from,"meld_with":meld_with,"station":false,"station_explicit":false,"station_declaration":null,"abilities":abilities}))
            .collect())
    }
    /// A card's surfaces, with the views only a `written` text asks for when it does.
    fn card_surfaces(
        &self,
        reading: &CardReading,
        context: &Characteristics,
        written: bool,
    ) -> Result<Vec<Value>, String> {
        let surfaces = self.card_surfaces_inner(
            reading,
            context,
            None,
            context.power_toughness.map(|(_, toughness)| toughness),
            written,
        )?;
        station::checked_surfaces(reading, None, surfaces)
    }
    fn card_surfaces_with_source(
        &self,
        reading: &CardReading,
        context: &CardInputContext,
        written: bool,
    ) -> Result<Vec<Value>, String> {
        station::checked_surfaces(
            reading,
            Some(context),
            self.card_surfaces_inner(
                reading,
                &context.characteristics,
                context.transform_front.as_deref(),
                context.printed_power_toughness.fixed_toughness(),
                written,
            )?,
        )
    }
}
