//! Validate event independence on the guarded scalar parse before canonicalizing.
use crate::model::canonical::trigger_batches::{event_independent, limited_single_event};
use serde_json::{Value, json};

const MARKER: &str = "$event_free_once";

pub(crate) fn mark(value: &Value) -> Result<Value, String> {
    if !limited_single_event(value) {
        return Err(
            "event-free single matcher needs a once-per-turn death or entry trigger".into(),
        );
    }
    let mut marked = value.clone();
    marked["args"][MARKER] = json!(true);
    Ok(marked)
}

pub(crate) fn finish(value: &mut Value) -> Result<(), String> {
    let marker = value
        .get_mut("args")
        .and_then(Value::as_object_mut)
        .and_then(|args| args.remove(MARKER));
    if let Some(marker) = marker {
        if marker != true || !limited_single_event(value) {
            return Err("invalid event-free single matcher marker".into());
        }
        if !event_independent(value)? {
            return Err("once-per-turn single matcher observes event data".into());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "trigger_batches_entry_tests.rs"]
mod entry_tests;
