//! Read-only test fixtures; production loading remains uncached.
use crate::engine::Engine;
use std::sync::OnceLock;

pub(crate) fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| Engine::load().expect("load test grammar"))
}
