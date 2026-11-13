//! Portable IR contracts: validation, binding, canonicalization and authoring.
//! Legacy Source compilation and engine backends are separate consumers.
pub use oracle_ir::{
    battlefield_query, counter, declarations, ir, ir_schema, mtg_types, targets, visit,
};
pub mod binding_map;
mod binding_roles;
pub mod canonical;
pub mod change_lists;
pub mod coin_policies;
pub mod complete;
pub mod copying;
pub mod cost_history;
pub mod human;
pub mod json_input;
pub mod keyword_actions;
pub mod migrate;
pub mod mode_policies;
pub mod movement;
pub mod payments;
pub mod presentation;
pub mod primitive_spec;
pub mod result_reads;
pub mod semantics;
pub mod source_values;
pub mod subtype;
pub mod validate;

#[cfg(test)]
extern crate self as mtgish_ast;
#[cfg(test)]
mod corpus {
    pub fn on_large_stack<T: Send>(f: impl FnOnce() -> T + Send) -> T {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(32 * 1024 * 1024)
                .spawn_scoped(scope, f)
                .unwrap()
                .join()
                .unwrap()
        })
    }
}

#[cfg(test)]
mod extraction_tests;
