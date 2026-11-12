//! "Up to one target creature" chooses a population of zero or one objects. Wherever
//! that population is used as a set of objects or players, or as damage recipients,
//! its normal form is `Targets(slot)`, independent of the consuming operation. Scalar
//! references keep their identity: attachment hosts, property reads, conditions and
//! members of larger groups. Mandatory single targets stay scalar.
//!
//! A required payment must not become payable because the population is empty: a cost
//! selection names the population and an explicit count of exactly one, and nothing
//! else inside a cost changes. Created triggers, modes and reflexive closures see the
//! slots their creator announced; an independent ability sees only its own.
//! Binding lowers the scalar spelling; the surface restores it for the printer.
use crate::traverse::{Boundary, boundary};
use serde_json::{Map, Value, json};

/// Slot counts in scope.
type Slots = Map<String, Value>;

fn up_to_one() -> Value {
    json!({"UpTo": {"kind": "Constant", "args": 1}})
}

fn exactly_one() -> Value {
    json!({"Exactly": {"kind": "Constant", "args": 1}})
}

/// The slots in scope at this node: an independent ability's own announcement, or the
/// creator's slots extended by a nested announcement.
fn scope(value: &Value, outer: &Slots, root: bool) -> Option<Slots> {
    let independent = !root && boundary(value) == Boundary::Ability;
    let announcement = value
        .pointer("/args/announcement/targets")
        .or_else(|| value.pointer("/announcement/targets"))
        .and_then(Value::as_array);
    if announcement.is_none() && !independent {
        return None;
    }
    let mut slots = if independent {
        Slots::new()
    } else {
        outer.clone()
    };
    for target in announcement.into_iter().flatten() {
        if let Some(slot) = target["slot"].as_u64() {
            slots.insert(slot.to_string(), target["count"].clone());
        }
    }
    Some(slots)
}

fn optional(slots: &Slots, slot: u64) -> bool {
    slots.get(&slot.to_string()) == Some(&up_to_one())
}

fn first_target(target: &Value) -> Option<u64> {
    (target["kind"] == "Target" && target["args"]["index"] == 0)
        .then(|| target["args"]["slot"].as_u64())
        .flatten()
}

fn set(slot: u64) -> Value {
    json!({"kind": "One", "args": {"kind": "Target", "args": {"slot": slot, "index": 0}}})
}

fn recipient(slot: u64) -> Value {
    json!({"kind": "Target", "args": {"slot": slot, "index": 0}})
}

/// One population node in the given direction; `scalar` spells its single member.
fn population(value: &mut Value, slots: &Slots, lower: bool, scalar: fn(u64) -> Value) -> bool {
    let slot = if !lower {
        (value["kind"] == "Targets")
            .then(|| value["args"].as_u64())
            .flatten()
    } else if value["kind"] == "One" {
        first_target(&value["args"])
    } else if scalar(0)["kind"] == "Target" {
        first_target(value)
    } else {
        None
    };
    let Some(slot) = slot.filter(|slot| optional(slots, *slot)) else {
        return false;
    };
    *value = if lower {
        json!({"kind": "Targets", "args": slot})
    } else {
        scalar(slot)
    };
    true
}

/// Damage recipients, including each member of a union.
fn recipients(value: &mut Value, slots: &Slots, lower: bool) -> bool {
    if value["kind"] == "Union"
        && let Some(members) = value["args"].as_array_mut()
    {
        let mut changed = false;
        for member in members {
            changed |= recipients(member, slots, lower);
        }
        return changed;
    }
    population(value, slots, lower, recipient)
}

/// A cost's selections of an optional target: the population, chosen exactly once.
fn cost(value: &mut Value, slots: &Slots, lower: bool) -> bool {
    let mut changed = false;
    for part in value["parts"].as_array_mut().into_iter().flatten() {
        let Some((kind, part)) = part.as_object_mut().and_then(|part| part.iter_mut().next())
        else {
            continue;
        };
        let selections: Vec<&mut Value> = match kind.as_str() {
            "Sacrifice" => vec![part],
            "Move" | "ChangeTapState" => part["selections"]
                .as_array_mut()
                .map(|selections| selections.iter_mut().collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        for selection in selections {
            changed |= cost_selection(selection, slots, lower);
        }
    }
    changed
}

fn cost_selection(selection: &mut Value, slots: &Slots, lower: bool) -> bool {
    if !lower && selection["choose"] != exactly_one() {
        return false;
    }
    if !population(&mut selection["objects"], slots, lower, set) {
        return false;
    }
    if !lower {
        selection["choose"] = Value::Null;
    } else if selection["choose"].is_null() {
        selection["choose"] = exactly_one();
    }
    true
}

/// Rewrite every population, from the scalar spelling to the whole population (`lower`)
/// or back (`!lower`). Whether anything changed.
fn rewrite(value: &mut Value, slots: &Slots, lower: bool, root: bool) -> bool {
    let own = scope(value, slots, root);
    let slots = own.as_ref().unwrap_or(slots);
    if population(value, slots, lower, set) {
        return true;
    }
    let mut changed = false;
    match value {
        Value::Object(fields) => {
            for (key, child) in fields.iter_mut() {
                changed |= match key.as_str() {
                    "recipients" if child.get("kind").is_some() => {
                        recipients(child, slots, lower) || rewrite(child, slots, lower, false)
                    }
                    "cost" if child.get("parts").is_some() => cost(child, slots, lower),
                    _ => rewrite(child, slots, lower, false),
                };
            }
        }
        Value::Array(items) => {
            for child in items {
                changed |= rewrite(child, slots, lower, false);
            }
        }
        _ => {}
    }
    changed
}

pub(super) fn lower(ability: &mut Value) {
    rewrite(ability, &Slots::new(), true, true);
}

/// The scalar spelling of each optional population. Whether any changed.
pub(super) fn surface(ability: &mut Value) -> bool {
    rewrite(ability, &Slots::new(), false, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ability(count: Value, instructions: Value) -> Value {
        json!({"kind":"Spell","args":{"announcement":{"targets":[{"slot":0,"count":count}]},
            "body":{"instructions":instructions}}})
    }

    fn effect(operation: Value) -> Value {
        json!([{"kind":"Effect","args":{"operation":operation}}])
    }

    fn whole() -> Value {
        json!({"kind": "Targets", "args": 0})
    }

    #[test]
    fn an_optional_population_is_its_whole_slot_in_every_operation_and_back() {
        let exile = |objects: Value| {
            effect(json!({"kind":"Move","args":{"objects":objects,"destination":{"Exile":{}}}}))
        };
        let mut spell = ability(up_to_one(), exile(set(0)));
        lower(&mut spell);
        assert_eq!(spell, ability(up_to_one(), exile(whole())));
        assert!(surface(&mut spell));
        assert_eq!(spell, ability(up_to_one(), exile(set(0))));
        let damage = |to: Value| effect(json!({"kind":"DealDamage","args":{"recipients":to}}));
        let mut dealt = ability(up_to_one(), damage(recipient(0)));
        lower(&mut dealt);
        assert_eq!(dealt, ability(up_to_one(), damage(whole())));
    }

    #[test]
    fn mandatory_targets_and_scalar_references_keep_their_identity() {
        let destroy = effect(json!({"kind":"Destroy","args":{"objects":set(0)}}));
        let mut mandatory = ability(exactly_one(), destroy.clone());
        lower(&mut mandatory);
        assert_eq!(mandatory, ability(exactly_one(), destroy));
        let attach = effect(json!({"kind":"Attach","args":{"host":recipient(0)}}));
        let mut host = ability(up_to_one(), attach.clone());
        lower(&mut host);
        assert_eq!(host, ability(up_to_one(), attach));
    }

    #[test]
    fn a_cost_selects_the_population_exactly_once() {
        let pay = |objects: Value, choose: Value| {
            effect(json!({"kind":"PayCost","args":{"cost":{"parts":[
                {"Sacrifice":{"objects":objects,"choose":choose}}]}}}))
        };
        let mut paid = ability(up_to_one(), pay(set(0), Value::Null));
        lower(&mut paid);
        assert_eq!(paid, ability(up_to_one(), pay(whole(), exactly_one())));
        assert!(surface(&mut paid));
        assert_eq!(paid, ability(up_to_one(), pay(set(0), Value::Null)));
    }

    #[test]
    fn an_independent_ability_does_not_borrow_its_creators_slots() {
        let destroy = effect(json!({"kind":"Destroy","args":{"objects":set(0)}}));
        let granted = json!({"kind":"Triggered","args":{"announcement":{"targets":[]},
            "body":{"instructions":destroy}}});
        let grant = effect(json!({"kind":"CreateContinuousEffect","args":{"changes":[granted]}}));
        let mut spell = ability(up_to_one(), grant.clone());
        lower(&mut spell);
        assert_eq!(spell, ability(up_to_one(), grant));
    }
}
