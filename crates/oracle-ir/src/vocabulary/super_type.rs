#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, bincode::Encode, bincode::Decode)]
pub enum SuperType {
    Basic,
    Legendary,
    Ongoing,
    Snow,
    World,
}
