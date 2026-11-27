//! Read-only test fixtures; production loading remains uncached.
use crate::{engine::Engine, model::migrate};
use serde_json::Value;
use std::sync::OnceLock;

pub(crate) fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| Engine::load().expect("load test grammar"))
}

/// Load a complete historical fixture reading at its frozen schema boundary.
pub(crate) fn fixture_abilities(source: &Value) -> Value {
    migrate::abilities_from_194(source).unwrap_or_else(|_| source.clone())
}
