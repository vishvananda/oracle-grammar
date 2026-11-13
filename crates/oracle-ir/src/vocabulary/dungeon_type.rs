#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, bincode::Encode, bincode::Decode)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum DungeonType {
    Undercity,
}
