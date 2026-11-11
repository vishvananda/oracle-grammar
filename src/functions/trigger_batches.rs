//! A once-per-turn single matcher is an alternative only without event reads.
//! Keep both IR forms; this guard never changes a supplied event or aliases IR.
use serde_json::{Value, json};

const MARKER: &str = "$event_free_once";

fn limited_single_event(value: &Value) -> bool {
    let event = &value["args"]["event"];
    value["kind"] == "Triggered"
        && value["args"]["limits"]["per_turn"] == 1
        && ((event["ZoneChange"]["from"] == "Battlefield"
            && event["ZoneChange"]["to"] == "Graveyard")
            || event["ZoneChange"]["to"] == "Battlefield"
            || event["Player"]["event"] == "EntersUnderControl")
}

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

fn enter(work: &mut usize, depth: usize) -> Result<(), String> {
    if *work == 0 || depth > 128 {
        return Err("event-free trigger work/depth limit".into());
    }
    *work -= 1;
    Ok(())
}

// Conservative across nested declarations too: a correct Batch remains
// available when this redundant representation cannot prove independence.
fn event_read(value: &Value, work: &mut usize, depth: usize) -> Result<bool, String> {
    enter(work, depth)?;
    if matches!(
        value["kind"].as_str(),
        Some(
            "EventSubject"
                | "EventDestination"
                | "EventStackObject"
                | "EventOther"
                | "EventOtherPlayer"
                | "EventGroup"
                | "EventAmount"
                | "EventActivationX"
                | "Damage"
                | "DamageRecipients"
                | "DamagedByEvent"
                | "CountersTheEventPut"
                | "Discarded"
                | "BlockPair"
                | "Scoped"
                | "Prior"
                | "Primitive"
                | "Term"
                | "Form"
        )
    ) || matches!(value.as_str(), Some("SpentOnEventCost" | "ProducedByEvent"))
    {
        return Ok(true);
    }
    match value {
        Value::Object(fields) => {
            for child in fields.values() {
                if event_read(child, work, depth + 1)? {
                    return Ok(true);
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                if event_read(child, work, depth + 1)? {
                    return Ok(true);
                }
            }
        }
        _ => {}
    }
    Ok(false)
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
        let mut work = 131_072;
        for field in ["announcement", "intervening_if", "body"] {
            if event_read(&value["args"][field], &mut work, 0)? {
                return Err("once-per-turn single matcher observes event data".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "trigger_batches_entry_tests.rs"]
mod entry_tests;
