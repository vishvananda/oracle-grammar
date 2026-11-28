//! A typed grammar carrier: native rules and native symbols, before note binding.
use super::{CardInputContext, CardNote};
use crate::model::ir::{Ability, Characteristics, Expr, ManaColor, ManaSymbol};
use crate::model::mtg_types::{CardType, CreatureType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CardSurface {
    pub notes: Vec<ManaSymbol>,
    pub assist_limit: Option<ManaSymbol>,
    #[serde(default)]
    pub no_mana_cost: bool,
    pub monocolored_hybrid: Option<MonocoloredHybridNote>,
    pub transmute_mana_value: Option<u32>,
    pub deck_identity_color: Option<ManaColor>,
    pub counted_power_example: Option<CountedPowerExample>,
    pub compleated_symbol: Option<ManaSymbol>,
    pub prowl_types: Option<Vec<CreatureType>>,
    pub amplify_types: Option<Vec<CreatureType>>,
    pub copy_token_types: Option<Vec<CreatureType>>,
    pub station: bool,
    pub station_explicit: bool,
    pub station_declaration: Option<StationSource>,
    pub land_mana: Option<LandMana>,
    #[serde(default)]
    pub retains_artifact_type: bool,
    pub layout_reminder: Option<LayoutReminder>,
    pub transforms_from: Option<String>,
    pub meld_with: Option<String>,
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

/// A printed land-play cost describes this source as a land, not a spell or
/// a generic payment instruction. Check the actual face in both directions.
pub(super) fn check_land_play_cost(context: &Characteristics, present: bool) -> Result<(), String> {
    if present
        && !context
            .card_types
            .iter()
            .any(|kind| matches!(kind, CardType::Land))
    {
        return Err("land-play cost requires a printed land face".into());
    }
    Ok(())
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
    LegendaryInstant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum AdventureReturnType {
    Creature,
    Artifact,
    Enchantment,
    Land,
}

impl AdventureReturnType {
    pub(crate) fn matches(self, card_type: &CardType) -> bool {
        matches!(
            (self, card_type),
            (Self::Creature, CardType::Creature)
                | (Self::Artifact, CardType::Artifact)
                | (Self::Enchantment, CardType::Enchantment)
                | (Self::Land, CardType::Land)
        )
    }
}

/// The typed options described by an inherent-mana header before context binding.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LandMana {
    pub colors: Vec<ManaColor>,
    #[serde(default)]
    pub creature_rules: bool,
}

/// A source reminder declares the striation that contains the printed box.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StationSource {
    pub minimum: Option<u32>,
}

/// An intrinsic reminder enumerates the complete set once. Prowl also
/// applies on kindred spells, while Amplify describes a creature entering.
pub(super) fn creature_note_types(
    context: &Characteristics,
    kindred_allowed: bool,
) -> Result<Vec<serde_json::Value>, String> {
    if !context.card_types.iter().any(|kind| {
        matches!(kind, CardType::Creature) || kindred_allowed && matches!(kind, CardType::Kindred)
    }) {
        return Err("subtype reminder needs a creature or applicable kindred face".into());
    }
    let mut kinds = Vec::new();
    for subtype in &context.subtypes {
        let value = serde_json::to_value(subtype).map_err(|error| error.to_string())?;
        if CreatureType::deserialize(&value).is_ok() && !kinds.contains(&value) {
            kinds.push(value);
        }
    }
    if kinds.is_empty() || kinds.len() > 16 {
        return Err("subtype reminder needs a bounded nonempty creature type list".into());
    }
    Ok(kinds)
}

/// Embalm and Eternalize add Zombie to the copy's printed creature types.
pub(super) fn copy_token_note_types(
    context: &Characteristics,
) -> Result<Vec<serde_json::Value>, String> {
    let mut kinds = creature_note_types(context, false)?;
    let zombie = serde_json::to_value(CreatureType::Zombie).map_err(|error| error.to_string())?;
    if !kinds.contains(&zombie) {
        if kinds.len() == 16 {
            return Err("copy reminder exceeds the creature type list limit".into());
        }
        kinds.insert(0, zombie);
    }
    Ok(kinds)
}

pub(super) fn check_compleated(
    context: &Characteristics,
    symbol: &ManaSymbol,
) -> Result<(), String> {
    if !context
        .card_types
        .iter()
        .any(|kind| matches!(kind, CardType::Planeswalker))
        || !matches!(symbol, ManaSymbol::Phyrexian(_))
    {
        return Err("Compleated reminder needs a planeswalker and a Phyrexian mana symbol".into());
    }
    Ok(())
}

pub(super) fn check_no_mana_cost(context: &Characteristics) -> Result<(), String> {
    if context
        .mana_cost
        .as_ref()
        .is_some_and(|cost| !cost.is_empty())
    {
        return Err("a no-mana-cost reminder requires an absent mana cost".into());
    }
    Ok(())
}

pub(super) fn check_retained_artifact_type(context: &Characteristics) -> Result<(), String> {
    if !context
        .card_types
        .iter()
        .any(|kind| matches!(kind, CardType::Artifact))
    {
        return Err("retained artifact reminder needs a printed Artifact type".into());
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CountedPowerExample {
    pub count: Expr,
    pub toughness: u32,
}

impl CardSurface {
    pub(super) fn decode(
        value: serde_json::Value,
        context: &Characteristics,
    ) -> Result<Self, String> {
        let surface: Self =
            serde_json::from_value(value).map_err(|why| format!("typed card surface: {why}"))?;
        check_land_play_cost(
            context,
            surface
                .abilities
                .iter()
                .any(|ability| matches!(ability, Ability::LandPlayCost { .. })),
        )?;
        Ok(surface)
    }

    pub(super) fn note_count(&self) -> usize {
        self.notes.len()
            + usize::from(self.land_mana.is_some())
            + usize::from(self.retains_artifact_type)
            + usize::from(self.layout_reminder.is_some())
            + usize::from(self.transforms_from.is_some())
            + usize::from(self.meld_with.is_some())
            + usize::from(self.assist_limit.is_some())
            + usize::from(self.no_mana_cost)
            + usize::from(self.monocolored_hybrid.is_some())
            + usize::from(self.transmute_mana_value.is_some())
            + usize::from(self.deck_identity_color.is_some())
            + usize::from(self.counted_power_example.is_some())
            + usize::from(self.compleated_symbol.is_some())
            + usize::from(self.prowl_types.is_some())
            + usize::from(self.amplify_types.is_some())
            + usize::from(self.copy_token_types.is_some())
    }

    pub(super) fn characteristic_notes(
        &self,
        context: &Characteristics,
        source: Option<&CardInputContext>,
    ) -> Result<Vec<CardNote>, String> {
        let mut notes = Vec::new();
        if let Some(written) = &self.deck_identity_color {
            if fixed_identity_color(context)? != *written {
                return Err("deck identity reminder disagrees with the printed color".into());
            }
            notes.push(CardNote::DeckIdentityColor);
        }
        if let Some(example) = &self.counted_power_example {
            let toughness = match source {
                Some(source) => source.printed_power_toughness.fixed_toughness(),
                None => context.power_toughness.map(|(_, toughness)| toughness),
            };
            if example_toughness(toughness)? != example.toughness {
                return Err("power example disagrees with the printed toughness".into());
            }
            let Expr::Constant(count) = example.count else {
                return Err("power example requires a stated count".into());
            };
            let count = u32::try_from(count).map_err(|_| "power example count is outside u32")?;
            notes.push(CardNote::CountedPowerExample { count });
        }
        if let Some(written) = self.transmute_mana_value {
            let cost = context.mana_cost.as_deref().unwrap_or_default();
            if printed_mana_value(cost)? != written {
                return Err("Transmute reminder disagrees with the current mana value".into());
            }
            notes.push(CardNote::TransmuteManaValue);
        }
        Ok(notes)
    }
}

pub(super) fn fixed_identity_color(context: &Characteristics) -> Result<ManaColor, String> {
    let mana_colors = context.mana_cost.iter().flatten().flat_map(|symbol| {
        let (first, second): (&[_], &[_]) = match symbol {
            ManaSymbol::Color(color) | ManaSymbol::MonocoloredHybrid(color) => {
                (std::slice::from_ref(color), &[])
            }
            ManaSymbol::Hybrid(first, second) => {
                (std::slice::from_ref(first), std::slice::from_ref(second))
            }
            ManaSymbol::Phyrexian(colors) => (colors, &[]),
            ManaSymbol::Rainbow => (
                &[
                    ManaColor::White,
                    ManaColor::Blue,
                    ManaColor::Black,
                    ManaColor::Red,
                    ManaColor::Green,
                ],
                &[],
            ),
            ManaSymbol::Generic(_) | ManaSymbol::Snow | ManaSymbol::X => (&[], &[]),
        };
        first.iter().chain(second)
    });
    let mut wanted = None;
    for color in context.color_indicator.iter().chain(mana_colors) {
        match color {
            ManaColor::Colorless => continue,
            ManaColor::Chosen => return Err("deck identity reminder needs a fixed color".into()),
            _ => {
                if wanted.is_some_and(|existing| existing != color) {
                    return Err("deck identity reminder requires one printed color".into());
                }
                wanted = Some(color);
            }
        }
    }
    wanted
        .cloned()
        .ok_or_else(|| "deck identity reminder needs a printed color".into())
}

pub(super) fn example_toughness(value: Option<i32>) -> Result<u32, String> {
    value
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| "power example needs a fixed printed toughness".into())
}

#[cfg(test)]
mod land_play_cost_tests {
    use super::check_land_play_cost;
    use crate::model::ir::Characteristics;
    use crate::model::mtg_types::CardType;
    use serde_json::json;

    #[test]
    fn land_play_cost_requires_a_land_but_does_not_restrict_other_abilities() {
        let mut context: Characteristics = serde_json::from_value(json!({
            "supertypes":[], "card_types":["Land", "Creature"], "subtypes":[],
            "mana_cost":null, "color_indicator":[], "power_toughness":null,
            "loyalty":null, "defense":null
        }))
        .unwrap();
        assert!(check_land_play_cost(&context, true).is_ok());
        context.card_types = vec![CardType::Creature];
        assert!(check_land_play_cost(&context, true).is_err());
        assert!(check_land_play_cost(&context, false).is_ok());
    }
}
