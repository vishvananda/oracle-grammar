#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, bincode::Encode, bincode::Decode)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum ArtifactType {
    Attraction,
    Blood,
    Bobblehead,
    Book,
    Clue,
    Equipment,
    Food,
    Fortification,
    Gold,
    Heartwood,
    Incubator,
    Infinity,
    Junk,
    Lander,
    Map,
    Mutagen,
    Powerstone,
    Spacecraft,
    Stone,
    Treasure,
    Vehicle,
    Vibranium,
}
