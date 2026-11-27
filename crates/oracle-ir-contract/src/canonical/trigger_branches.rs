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

fn departure_zones(left: &Value, right: &Value) -> Option<(&'static str, &'static str)> {
    let zone = |ability: &Value| {
        let event = &ability["args"]["event"]["ZoneChange"];
        if ability["kind"] != "Triggered" || event["from"] != "Battlefield" {
            return None;
        }
        match event["to"].as_str()? {
            "Exile" => Some("Exile"),
            "Graveyard" => Some("Graveyard"),
            _ => None,
        }
    };
    let a = zone(left)?;
    let b = zone(right)?;
    (a != b
        && left["args"]["event"]["ZoneChange"]["objects"]
            == right["args"]["event"]["ZoneChange"]["objects"])
        .then_some((a, b))
}

/// EventDestination already selects only the first public incarnation and
/// becomes empty if it leaves that zone. A return of that object has the same
/// source, recipient, and order when its own arrival zone is implicit.
fn departure_return(ability: &mut Value, zone: &str) -> bool {
    let Some([instruction]) = ability["args"]["body"]["instructions"]
        .as_array_mut()
        .map(Vec::as_mut_slice)
    else {
        return false;
    };
    let optional = instruction["kind"] == "Optional";
    let effect = if optional {
        let Some([effect]) = instruction["args"]["body"]["instructions"]
            .as_array_mut()
            .map(Vec::as_mut_slice)
        else {
            return false;
        };
        effect
    } else {
        instruction
    };
    if effect["kind"] != "Effect" {
        return false;
    }
    let operation = &mut effect["args"]["operation"];
    let destination = if optional { "Library" } else { "OwnersHand" };
    if operation["kind"] != "Move"
        || operation["args"]["destination"].get(destination).is_none()
        || operation["args"]["qualifiers"] != json!([])
    {
        return false;
    }
    let objects = &mut operation["args"]["objects"];
    let direct = json!({"kind":"One","args":{"kind":"EventDestination"}});
    if *objects == direct {
        return true;
    }
    if *objects
        != json!({"kind":"One","args":{"kind":"Query","args":{
            "objects":{"kind":"InZone","args":{"objects":direct,"zone":zone}},"index":0
        }}})
    {
        return false;
    }
    *objects = direct;
    true
}

fn merge_departure_returns(a: &mut Value, b: &mut Value, tracing: bool) -> bool {
    let Some((first, second)) = departure_zones(a, b) else {
        return false;
    };
    let mut left = a.clone();
    let mut right = b.clone();
    if !departure_return(&mut left, first)
        || !departure_return(&mut right, second)
        || !same_but_event(&left, &right, tracing)
    {
        return false;
    }
    *a = left;
    *b = right;
    true
}

fn compatible(a: &mut Value, b: &mut Value, schema: &Schema, tracing: bool) -> bool {
    if same_but_event(a, b, tracing) || merge_departure_returns(a, b, tracing) {
        return true;
    }
    if let Some((left, right)) = super::combat_pairs::comparable(a, b, schema, tracing)
        && same_but_event(&left, &right, tracing)
    {
        *a = left;
        *b = right;
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

/// Two disjoint departure branches with a shared return body joined above.
pub(super) fn single(value: &mut Value, tracing: bool) {
    let Some([only]) = value["args"].as_array().map(Vec::as_slice) else {
        return;
    };
    let Some([first, second]) = only["args"]["event"]["AnyOf"].as_array().map(Vec::as_slice) else {
        return;
    };
    let events = [first, second].map(|event| {
        json!({
            "kind":"Triggered", "args":{"event":event}
        })
    });
    if departure_zones(&events[0], &events[1]).is_some()
        && only["kind"] == "Triggered"
        && super::walk::lossless::<crate::ir::Ability>(value, tracing).is_some()
    {
        *value = value["args"][0].take();
    }
}
