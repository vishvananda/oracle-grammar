#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, bincode::Encode, bincode::Decode)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum LandType {
    Cave,
    Desert,
    Forest,
    Gate,
    Island,
    Lair,
    Locus,
    Mine,
    Mountain,
    Plains,
    Planet,
    PowerPlant,
    Sphere,
    Swamp,
    Tower,
    Town,
    Urzas,
}
