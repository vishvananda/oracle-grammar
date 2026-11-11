//! A typed grammar carrier: native rules and native symbols, before note binding.
use crate::model::ir::{Ability, ManaColor, ManaSymbol};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CardSurface {
    pub notes: Vec<ManaSymbol>,
    pub station: bool,
    pub station_explicit: bool,
    pub station_declaration: Option<StationSource>,
    pub land_mana: Option<LandMana>,
    pub abilities: Vec<Ability>,
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
