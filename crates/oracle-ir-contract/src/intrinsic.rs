//! Basic-land mana is derived from current types (CR 305.6), including when its
//! familiar text is printed. A printed plain manifestation is not an additional
//! ability. This classification applies to a face's own printed definitions,
//! never to an independently granted ability or acquisition.
use crate::ir::*;
use crate::mtg_types::{CardType, SubType};

/// The intrinsic outputs of a land's current basic land types. Nonbasic lands
/// qualify; having a basic supertype without a basic land type does not.
pub fn basic_land_mana(characteristics: &Characteristics) -> Vec<ManaColor> {
    if !characteristics
        .card_types
        .iter()
        .any(|kind| matches!(kind, CardType::Land))
    {
        return Vec::new();
    }
    let mut present = [false; 5];
    for subtype in &characteristics.subtypes {
        let index = match subtype {
            SubType::Plains => 0,
            SubType::Island => 1,
            SubType::Swamp => 2,
            SubType::Mountain => 3,
            SubType::Forest => 4,
            _ => continue,
        };
        present[index] = true;
    }
    [
        ManaColor::White,
        ManaColor::Blue,
        ManaColor::Black,
        ManaColor::Red,
        ManaColor::Green,
    ]
    .into_iter()
    .enumerate()
    .filter_map(|(index, color)| present[index].then_some(color))
    .collect()
}

/// Whether a direct printed activation states an intrinsic mana ability already
/// supplied by this face's printed/base types. Classify before applying copy
/// exceptions or type-changing effects; deriving outputs from the resulting
/// current types is a separate step. Costs, restrictions, limits, targets and other
/// actions must match the exact plain definition. No text or card name is used.
pub fn printed_basic_land_mana(
    characteristics: &Characteristics,
    ability: &Ability,
) -> Option<ManaColor> {
    // Presentation retains this same printed definition and acquisition. It
    // cannot turn the manifestation into another activation. Other wrappers,
    // including scope restrictions and grants, still fail the exact proof.
    let mut ability = ability;
    let mut remaining = 128;
    while let Ability::WithPresentation { ability: inner, .. } = ability {
        if remaining == 0 {
            return None;
        }
        remaining -= 1;
        ability = inner;
    }
    let Ability::Activated {
        origin,
        cost,
        limits,
        announcement,
        body,
    } = ability
    else {
        return None;
    };
    if !matches!(origin, ActivationOrigin::Ordinary)
        || !matches!(cost.parts.as_slice(), [CostPart::TapSymbol])
        || limits.once
        || limits.once_each_turn
        || limits.sorcery
        || limits.condition.is_some()
        || limits.activators.is_some()
        || limits.keyword.is_some()
        || !limits.modifiers.is_empty()
        || !announcement.targets.is_empty()
        || announcement.modes.is_some()
    {
        return None;
    }
    let [
        Instruction::Effect {
            operation:
                Operation::AddMana {
                    players: PlayerSet::One(PlayerRef::AbilityController),
                    mana:
                        ManaProduction::Amount {
                            color,
                            amount: Expr::Constant(1),
                        },
                    restriction: None,
                },
            ..
        },
    ] = body.instructions.as_slice()
    else {
        return None;
    };
    basic_land_mana(characteristics)
        .contains(color)
        .then(|| color.clone())
}

/// The face's nonintrinsic printed abilities. Consumers combine these with the
/// intrinsic abilities derived from current characteristics, rather than adding
/// a second activation for a printed intrinsic mana line. The same projection
/// gives the card-context comparison normal form for optional intrinsic lines.
/// Keep the serialized written form when preserving exact Oracle wording.
/// Derive intrinsic mana before applying the ability layer, so an effect removing
/// abilities can still remove these abilities. Validate the face before comparing.
pub fn nonintrinsic_printed_abilities(card: &Card) -> impl Iterator<Item = &Ability> {
    card.abilities
        .iter()
        .filter(|ability| printed_basic_land_mana(&card.characteristics, ability).is_none())
}
