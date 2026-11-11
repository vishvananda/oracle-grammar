//! Views of the native destination, independent of its arrival modifiers.
use serde_json::Value;

pub(super) fn zone(destination: &Value) -> Option<&'static str> {
    [
        ("Battlefield", "Battlefield"),
        ("Exile", "Exile"),
        ("Library", "Library"),
        ("OwnersHand", "Hand"),
        ("Hand", "Hand"),
        ("OwnersGraveyard", "Graveyard"),
        ("CommandZone", "Command"),
        ("Ante", "Ante"),
        ("OwnersPlanarDeck", "PlanarDeck"),
    ]
    .into_iter()
    .find_map(|(key, zone)| destination.get(key).map(|_| zone))
}

#[cfg(test)]
pub(super) fn test_destination(value: Value) -> Value {
    use crate::model::ir::{EntryController, ExileVisibility, MovementDestination};
    let destination = match value.as_str() {
        Some("Exile") => MovementDestination::Exile {
            visibility: ExileVisibility::FaceUp,
            arrival: Default::default(),
        },
        Some("OwnersBattlefield") => MovementDestination::Battlefield {
            controller: EntryController::Owner,
            arrival: Default::default(),
        },
        Some("OwnersHand") => MovementDestination::OwnersHand(Default::default()),
        Some("OwnersGraveyard") => MovementDestination::OwnersGraveyard(Default::default()),
        _ => return value,
    };
    serde_json::to_value(destination).expect("native destination serializes")
}
