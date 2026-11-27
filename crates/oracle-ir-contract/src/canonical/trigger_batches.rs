//! Normalize event grouping when only one event-independent trigger can resolve.
use serde_json::{Value, json};

fn entry_or_death(event: &Value) -> bool {
    (event["ZoneChange"]["from"] == "Battlefield" && event["ZoneChange"]["to"] == "Graveyard")
        || event["ZoneChange"]["to"] == "Battlefield"
        || event["Player"]["event"] == "EntersUnderControl"
}

fn once_per_turn(value: &Value) -> bool {
    value["kind"] == "Triggered" && value["args"]["limits"]["per_turn"] == 1
}

pub fn limited_single_event(value: &Value) -> bool {
    once_per_turn(value) && entry_or_death(&value["args"]["event"])
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

/// Reject collapsed groups if resolution, announcement, or an intervening
/// condition observes the triggering participant or group. This proof is shared
/// by pre-canonical grammar binding and canonical comparison of saved IR.
pub fn event_independent(value: &Value) -> Result<bool, String> {
    let mut work = 131_072;
    for field in ["announcement", "intervening_if", "body"] {
        if event_read(&value["args"][field], &mut work, 0)? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn capped_occurrence(value: &mut Value) {
    let event = &value["args"]["event"];
    let batch = &event["Batch"];
    if once_per_turn(value)
        && event.as_object().is_some_and(|fields| fields.len() == 1)
        && batch.as_object().is_some_and(|fields| fields.len() == 4)
        && batch["per"] == "Whole"
        && batch.get("count").is_some_and(Value::is_null)
        && batch.get("including").is_some_and(Value::is_null)
        && entry_or_death(&batch["event"])
        && matches!(event_independent(value), Ok(true))
    {
        let event = value["args"]["event"]["Batch"]["event"].take();
        value["args"]["event"] = event;
    }
}

/// Each member of an attack batch is a declared, matching attacker. A batch
/// exists when at least one attacker matches (CR 508.3a, 508.3c and 508.3d).
pub(super) fn attack_minimum(value: &mut Value) {
    let batch = &mut value["Batch"];
    if batch["per"] == "Whole"
        && batch["event"].get("Attack").is_some()
        && batch["count"] == json!(["GreaterOrEqual", {"kind":"Constant","args":1}])
    {
        batch["count"] = Value::Null;
    }
}

// Matching rules may spell a redundant cardinality or explicit capped group.
// Build these inverse forms alongside the canonical one, then verify the text.
#[derive(Clone, Copy)]
enum WrittenEvent {
    AttackMinimum,
    CappedGroup,
}

fn implicit_attack(value: &Value) -> bool {
    let batch = &value["Batch"];
    batch["per"] == "Whole"
        && batch["event"].get("Attack").is_some()
        && batch.get("count").is_some_and(Value::is_null)
}

fn has_written_form(value: &Value, limit: &mut usize, depth: usize) -> bool {
    if *limit == 0 || depth > 128 {
        return false;
    }
    *limit -= 1;
    match value {
        Value::Object(fields) => {
            implicit_attack(value)
                || limited_single_event(value)
                || fields
                    .values()
                    .any(|child| has_written_form(child, limit, depth + 1))
        }
        Value::Array(items) => items
            .iter()
            .any(|child| has_written_form(child, limit, depth + 1)),
        _ => false,
    }
}

#[derive(Default)]
struct WrittenEvents(Vec<(String, WrittenEvent)>);
impl super::walk::Visit for WrittenEvents {
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &crate::ir_schema::Ty,
        _: bool,
        path: &str,
        _: &mut super::walk::Walker<'_>,
    ) -> Result<(), String> {
        if self.0.len() == 5 {
            return Ok(());
        }
        if matches!(ty, crate::ir_schema::Ty::Named(name) if name == "EventSpec")
            && implicit_attack(value)
        {
            self.0.push((path.into(), WrittenEvent::AttackMinimum));
        }
        if matches!(ty, crate::ir_schema::Ty::Named(name) if name == "Ability")
            && limited_single_event(value)
            && matches!(event_independent(value), Ok(true))
        {
            self.0
                .push((format!("{path}/args/event"), WrittenEvent::CappedGroup));
        }
        Ok(())
    }
}

pub fn print_views(ability: &Value) -> Result<Vec<Value>, String> {
    if !has_written_form(ability, &mut 131_072, 0) {
        return Ok(Vec::new());
    }
    let mut view = ability.clone();
    let mut sites = WrittenEvents::default();
    super::walk::Walker::new(super::schema()?).walk(
        &mut view,
        &crate::ir_schema::Ty::Named("Ability".into()),
        false,
        "",
        0,
        &mut sites,
    )?;
    if sites.0.is_empty() {
        return Ok(Vec::new());
    }
    let all = (1 << sites.0.len()) - 1;
    std::iter::once(all).chain(1..all).map(|mask| {
        let mut variant = view.clone();
        // The typed walker visits children first, so wrapping a parent keeps
        // every previously restored child attached to the same event.
        for (at, (path, form)) in sites.0.iter().enumerate() {
            if mask & (1 << at) == 0 {
                continue;
            }
            let event = variant.pointer_mut(path).ok_or("event print view lost its path")?;
            match form {
                WrittenEvent::AttackMinimum => {
                    event["Batch"]["count"] = json!(["GreaterOrEqual", {"kind":"Constant","args":1}]);
                }
                WrittenEvent::CappedGroup => {
                    *event = json!({"Batch":{"event":event.take(),"per":"Whole","count":null,"including":null}});
                }
            }
        }
        Ok(variant)
    }).collect()
}
