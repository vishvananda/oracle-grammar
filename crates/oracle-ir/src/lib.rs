//! Frozen normalized IR types and their wire/schema contracts.
//! Consumers pin this package independently of changing canonicalization,
//! validation and source-lowering code, so its serialization derives stay cached.
// Preserve the extraction baseline's enum layout; boxing is a separate model change.
#![allow(clippy::large_enum_variant)]
pub mod vocabulary;
/// Compatibility name for existing frontend imports.
pub use vocabulary as mtg_types;
pub mod counter;
pub mod declarations;
pub mod ir;
pub mod ir_schema;
pub mod visit;

pub mod targets;
