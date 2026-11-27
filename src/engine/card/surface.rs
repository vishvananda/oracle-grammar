//! A typed grammar carrier: native rules and native symbols, before note binding.
use crate::model::ir::{Ability, Characteristics, ManaColor, ManaSymbol};
use crate::model::mtg_types::{CardType, CreatureType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CardSurface {
    pub notes: Vec<ManaSymbol>,
    pub assist_limit: Option<ManaSymbol>,
    pub monocolored_hybrid: Option<MonocoloredHybridNote>,
    pub prowl_types: Option<Vec<CreatureType>>,
    pub station: bool,
    pub station_explicit: bool,
    pub station_declaration: Option<StationSource>,
    pub land_mana: Option<LandMana>,
    pub layout_reminder: Option<LayoutReminder>,
    pub transforms_from: Option<String>,
    pub abilities: Vec<Ability>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MonocoloredHybridNote {
    pub symbol: ManaSymbol,
    pub mana_value: u32,
}

pub(super) fn printed_mana_value(cost: &[ManaSymbol]) -> Result<u32, String> {
    cost.iter().try_fold(0u32, |sum, symbol| {
        let value = match symbol {
            ManaSymbol::Generic(value) => *value,
            ManaSymbol::MonocoloredHybrid(_) => 2,
            ManaSymbol::X => 0,
            _ => 1,
        };
        sum.checked_add(value)
            .ok_or_else(|| "printed mana value overflow".into())
    })
}

/// Explanations of rules intrinsic to the source face's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum LayoutReminder {
    Adventure(AdventureReturnType),
    Omen,
    Room,
    Siege,
    OngoingScheme,
    FaceUpConspiracy,
    LegendarySorcery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum AdventureReturnType {
    Creature,
    Artifact,
    Enchantment,
}

impl AdventureReturnType {
    pub(crate) fn matches(self, card_type: &CardType) -> bool {
        matches!(
            (self, card_type),
            (Self::Creature, CardType::Creature)
                | (Self::Artifact, CardType::Artifact)
                | (Self::Enchantment, CardType::Enchantment)
        )
    }
}

/// The typed options described by an inherent-mana header before context binding.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LandMana {
    pub colors: Vec<ManaColor>,
}

/// A source reminder declares the striation that contains the printed box.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StationSource {
    pub minimum: Option<u32>,
}

/// Prowl keys on the source's creature subtypes, including on a kindred spell.
/// A printed reminder enumerates that entire set once.
pub(super) fn prowl_types(context: &Characteristics) -> Result<Vec<serde_json::Value>, String> {
    if !context
        .card_types
        .iter()
        .any(|kind| matches!(kind, CardType::Creature | CardType::Kindred))
    {
        return Err("prowl reminder needs a creature or kindred face".into());
    }
    let mut kinds = Vec::new();
    for subtype in &context.subtypes {
        let value = serde_json::to_value(subtype).map_err(|error| error.to_string())?;
        if CreatureType::deserialize(&value).is_ok() && !kinds.contains(&value) {
            kinds.push(value);
        }
    }
    if kinds.is_empty() || kinds.len() > 16 {
        return Err("prowl reminder needs a bounded nonempty creature type list".into());
    }
    Ok(kinds)
}
