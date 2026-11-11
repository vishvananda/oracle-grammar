//! Model boundary used by the grammar, binder, renderer and diagnostics.
//!
//! The current schema implementation is pinned in `pins/model.json`. This
//! compatibility bridge keeps its provenance explicit while local IR ownership
//! and the optional MTGish validation adapter are developed independently.
pub use mtgish_ast::{
    canonical, cost_history, human, ir, ir_schema, mtg_types, semantics, validate,
};

#[cfg(test)]
mod tests {
    #[test]
    fn compiled_schema_matches_model_pin() {
        let pin: serde_json::Value =
            serde_json::from_str(include_str!("../pins/model.json")).unwrap();
        assert_eq!(pin["schema_version"], super::ir::SCHEMA_VERSION);
    }
}
