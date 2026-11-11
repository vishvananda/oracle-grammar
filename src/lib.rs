//! Phrase-level Oracle grammar over MTGish's normalized IR. See SPEC.md.
pub mod bind;
pub mod corpus;
pub mod derivation;
pub mod engine;
pub mod functions;
pub mod grammar;
pub mod heads;
pub mod mine;
pub mod model;
pub mod parse;
pub mod pattern;
pub mod reader;
pub mod render;
pub mod replay;
pub mod starts;
pub mod style;
pub mod tokens;
mod traverse;

#[cfg(test)]
mod saved_meaning_preflight;

#[cfg(test)]
mod harnessed_turn_heading_tests;

#[cfg(test)]
mod resolution_sacrifice_actor_tests;

#[cfg(test)]
mod remembered_life_player_tests;

#[cfg(test)]
mod linked_card_action_tests;

#[cfg(test)]
mod target_contrast_tests;

#[cfg(test)]
mod revealed_card_antecedent_tests;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests;
