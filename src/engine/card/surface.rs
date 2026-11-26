//! A typed grammar carrier: native rules and native symbols, before note binding.
use crate::model::ir::{Ability, ManaColor, ManaSymbol};
use crate::model::mtg_types::CardType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CardSurface {
    pub notes: Vec<ManaSymbol>,
    pub station: bool,
    pub station_explicit: bool,
    pub station_declaration: Option<StationSource>,
    pub land_mana: Option<LandMana>,
    pub layout_reminder: Option<LayoutReminder>,
    pub abilities: Vec<Ability>,
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
