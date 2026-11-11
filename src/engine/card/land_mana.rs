//! A reminder references the mana granted by the current basic land types.
use crate::model::ir::{Characteristics, ManaColor};
use crate::model::mtg_types::{CardType, SubType};

pub(super) fn colors(context: &Characteristics) -> Result<Vec<ManaColor>, String> {
    if !context
        .card_types
        .iter()
        .any(|ty| matches!(ty, CardType::Land))
    {
        return Err("inherent mana requires a land".into());
    }
    let mut present = [false; 5];
    for subtype in &context.subtypes {
        let at = match subtype {
            SubType::Plains => 0,
            SubType::Island => 1,
            SubType::Swamp => 2,
            SubType::Mountain => 3,
            SubType::Forest => 4,
            _ => continue,
        };
        present[at] = true;
    }
    let colors: Vec<_> = [
        ManaColor::White,
        ManaColor::Blue,
        ManaColor::Black,
        ManaColor::Red,
        ManaColor::Green,
    ]
    .into_iter()
    .zip(present)
    .filter_map(|(color, found)| found.then_some(color))
    .collect();
    if colors.is_empty() {
        return Err("inherent mana requires a basic land type".into());
    }
    Ok(colors)
}
