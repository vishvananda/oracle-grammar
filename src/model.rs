//! Model boundary used by the grammar, binder, renderer and diagnostics.
//!
//! Oracle Grammar owns these workspace crates. MTGish reference snapshots are
//! independently pinned data; they are not a compiled model dependency.
pub use oracle_ir::{ir, ir_schema, mtg_types};
pub use oracle_ir_contract::{canonical, cost_history, human, semantics, validate};

#[cfg(test)]
mod tests {
    #[test]
    fn compiled_schema_matches_model_pin() {
        let pin: serde_json::Value =
            serde_json::from_str(include_str!("../pins/model.json")).unwrap();
        assert_eq!(pin["schema_version"], super::ir::SCHEMA_VERSION);
    }
}
