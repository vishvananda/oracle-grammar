//! Join matching trigger branches in their shared event frame.
use super::{
    trace,
    walk::{Visit, Walker},
};
use crate::ir_schema::{Schema, Ty};
use serde_json::{Value, json};

// Context lifting is optional; exhaustion retains the explicit branches.
const MAX_LIFT_WORK: usize = 8192;

struct Lift<'a> {
    abilities: usize,
    events: usize,
    closures: usize,
    remaining: &'a mut usize,
}
fn closure(value: &Value, ty: &Ty) -> bool {
    matches!(ty, Ty::Named(name) if name == "Operation")
        && matches!(
            value["kind"].as_str(),
            Some(
                "CreateTrigger" | "CreateDelayedTrigger" | "CreateReplacement" | "ReflexiveTrigger"
            )
        )
}
impl Visit for Lift<'_> {
    fn enter(&mut self, value: &Value, ty: &Ty) {
        *self.remaining = self.remaining.saturating_sub(1);
        self.abilities += usize::from(matches!(ty, Ty::Named(name) if name == "Ability"));
        self.events += usize::from(
            matches!(ty, Ty::Named(name) if matches!(name.as_str(), "EventSpec" | "ReplacementEvent")),
        );
        self.closures += usize::from(closure(value, ty));
    }
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        _: bool,
        _: &str,
        _: &mut Walker<'_>,
    ) -> Result<(), String> {
        if *self.remaining == 0 {
            return Err("trigger branch comparison work limit".into());
        }
        if self.abilities == 1
            && self.events == 0
            && self.closures == 0
            && matches!(ty, Ty::Named(name) if name == "ObjectRef")
            && value.as_object().is_some_and(|fields| fields.len() == 1)
            && value["kind"] == "Source"
        {
            *value = json!({"kind":"EventSubject"});
        }
        self.closures -= usize::from(closure(value, ty));
        self.events -= usize::from(
            matches!(ty, Ty::Named(name) if matches!(name.as_str(), "EventSpec" | "ReplacementEvent")),
        );
        self.abilities -= usize::from(matches!(ty, Ty::Named(name) if name == "Ability"));
        Ok(())
    }
}

fn lift(value: &mut Value, schema: &Schema, remaining: &mut usize) -> bool {
    if !super::source_events::names_source(&value["args"]["event"]) {
        return true;
    }
    let mut visitor = Lift {
        abilities: 0,
        events: 0,
        closures: 0,
        remaining,
    };
    Walker::new(schema)
        .walk(
            value,
            &Ty::Named("Ability".into()),
            false,
            "",
            0,
            &mut visitor,
        )
        .is_ok()
}

fn compatible(a: &mut Value, b: &mut Value, schema: &Schema, tracing: bool) -> bool {
    if same_but_event(a, b, tracing) {
        return true;
    }
    if a["kind"] != "Triggered"
        || b["kind"] != "Triggered"
        || !(super::source_events::names_source(&a["args"]["event"])
            || super::source_events::names_source(&b["args"]["event"]))
    {
        return false;
    }
    // Only an independently proved source branch can express Source as its
    // EventSubject. Keep descendant abilities, future bodies and event headers.
    let (mut shared_a, mut shared_b) = (a.clone(), b.clone());
    let mut remaining = MAX_LIFT_WORK;
    if !lift(&mut shared_a, schema, &mut remaining)
        || !lift(&mut shared_b, schema, &mut remaining)
        || !same_but_event(&shared_a, &shared_b, tracing)
    {
        return false;
    }
    *a = shared_a;
    *b = shared_b;
    true
}

/// Whether two triggered abilities differ in nothing but what triggers them.
fn same_but_event(a: &Value, b: &Value, tracing: bool) -> bool {
    a["kind"] == "Triggered"
        && b["kind"] == "Triggered"
        && ["intervening_if", "limits", "announcement", "body"]
            .iter()
            .all(|field| trace::same(&a["args"][*field], &b["args"][*field], tracing))
}

/// The events of a trigger, whether it has one or lists several.
fn trigger_events(ability: &Value) -> Vec<Value> {
    match ability["args"]["event"].get("AnyOf") {
        Some(Value::Array(cases)) => cases.clone(),
        _ => vec![ability["args"]["event"].clone()],
    }
}

/// `Whenever a or b, do c`, held as one ability with `Either` or as two sharing a
/// body, means the same either way, so comparison sees one ability in event order.
pub(super) fn merge(items: &mut Vec<Value>, schema: &Schema, tracing: bool) {
    let mut merged: Vec<Value> = Vec::with_capacity(items.len());
    for mut item in items.drain(..) {
        if let Some(last) = merged.last_mut()
            && compatible(last, &mut item, schema, tracing)
        {
            if tracing {
                for field in ["intervening_if", "limits", "announcement", "body"] {
                    trace::merge(&mut last["args"][field], &item["args"][field]);
                }
            }
            let mut events = trigger_events(last);
            events.extend(trigger_events(&item));
            last["args"]["event"] = json!({ "AnyOf": events });
        } else {
            merged.push(item);
        }
    }
    for item in merged.iter_mut().filter(|item| item["kind"] == "Triggered") {
        if let Some(Value::Array(events)) = item["args"]["event"].get_mut("AnyOf") {
            events.sort_by_key(|event| trace::key(event, tracing));
        }
    }
    *items = merged;
}
