//! Where an event's subject is proven to be one object, a body that follows the event
//! names that object rather than the event's subject: a "When you do" closure under
//! the source's own trigger names the source, and a trigger created for exactly one
//! captured target names that target. Independent abilities and the triggers a body
//! creates have events of their own, so the naming stops at them. A card put into a
//! hand is hidden there: a duplicate of it copies the card as it last existed. Connive
//! names its own object, so the source connives under its own event.
use super::other::{source_damage, source_event};
use crate::traverse::{Boundary, Visit, boundary, rewrite};
use serde_json::{Value, json};

pub(super) fn name(body: &mut Value, object: &Value) {
    name_role(body, &json!({"kind": "EventSubject"}), object);
}

/// Replace each reference to `subject` with `object` up to the next independent event.
/// Whether any changed.
fn name_role(body: &mut Value, subject: &Value, object: &Value) -> bool {
    let mut changed = false;
    rewrite(body, &mut |node| {
        if node == subject {
            *node = object.clone();
            changed = true;
            return Visit::Skip;
        }
        let creates = matches!(
            node["kind"].as_str(),
            Some("CreateTrigger" | "CreateDelayedTrigger")
        );
        if creates || boundary(node) == Boundary::Ability {
            Visit::Skip
        } else {
            Visit::Children
        }
    });
    changed
}

fn duplicate() -> Value {
    json!({"Duplicate": {"kind": "One", "args": {"kind": "EventSubject"}}})
}

fn last_known_duplicate() -> Value {
    json!({"DuplicateLastKnown": {"kind": "EventSubject"}})
}

/// Under a trigger for a card put into a hand, a duplicate of that card copies its last
/// known definition (`lower`), or back to the printed spelling. Whether any changed.
pub(super) fn duplicate_hidden_arrivals(ability: &mut Value, lower: bool) -> bool {
    let (from, to) = if lower {
        (duplicate(), last_known_duplicate())
    } else {
        (last_known_duplicate(), duplicate())
    };
    let mut changed = false;
    rewrite(ability, &mut |node| {
        if matches!(node["kind"].as_str(), Some("Triggered" | "CreateTrigger"))
            && node["args"]["event"]["ZoneChange"]["to"] == "Hand"
        {
            changed |= name_role(&mut node["args"]["body"], &from, &to);
        }
        Visit::Children
    });
    changed
}

/// An ability on its own object's event names that object the source in its intervening
/// condition. The model makes this substitution except inside a closure (a reflexive or
/// delayed trigger or a replacement); an independent ability there, such as a token's
/// or a grant's, still has its own event, so its own condition is named here.
pub(super) fn name_enclosed_sources(ability: &mut Value) {
    enclosed_sources(ability, false);
}

fn enclosed_sources(value: &mut Value, closed: bool) {
    let closed = closed
        || matches!(
            value["kind"].as_str(),
            Some("ReflexiveTrigger" | "CreateDelayedTrigger" | "CreateReplacement")
        );
    if closed && value["kind"] == "Triggered" && own_subject(&value["args"]["event"]) {
        name(
            &mut value["args"]["intervening_if"],
            &json!({"kind": "Source"}),
        );
    }
    match value {
        Value::Object(fields) => fields
            .values_mut()
            .for_each(|child| enclosed_sources(child, closed)),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|child| enclosed_sources(child, closed)),
        _ => {}
    }
}

/// The model's proof that an event is about the source: its objects are the source.
fn own_subject(event: &Value) -> bool {
    if let Some(filtered) = event.get("Filtered") {
        return own_subject(&filtered["event"]);
    }
    let source = json!({"kind": "SameObject", "args": {"kind": "Source"}});
    ["Object", "ZoneChange", "Pair"]
        .iter()
        .find_map(|kind| event.get(*kind))
        .is_some_and(|event| event["objects"] == source)
}

/// Connive names the conniving object itself (CR 701.50a): under the source's own
/// event that is the source, and its controller connives.
pub(super) fn name_conniving_sources(ability: &mut Value) {
    rewrite(ability, &mut |node| {
        if matches!(node["kind"].as_str(), Some("Triggered" | "CreateTrigger"))
            && super::holder::self_event(&node["args"]["event"], 0)
        {
            name_connivers(&mut node["args"]["body"]);
        }
        Visit::Children
    });
}

fn name_connivers(body: &mut Value) {
    let subject = json!({"kind": "EventSubject"});
    let source = json!({"kind": "Source"});
    rewrite(body, &mut |node| {
        if node["kind"] == "Keyword" && node["args"]["action"]["Connive"]["object"] == subject {
            let args = &mut node["args"];
            args["action"]["Connive"]["object"] = source.clone();
            name(&mut args["actor"], &source);
            return Visit::Skip;
        }
        if boundary(node) == Boundary::Ability || node["kind"] == "CreateTrigger" {
            Visit::Skip
        } else {
            Visit::Children
        }
    });
}

/// A payment is not an object event: under the source's own trigger, each "When you do"
/// closure still names the source. A damage trigger's subject is a damage role instead.
pub(super) fn name_reflexive_sources(ability: &mut Value) {
    rewrite(ability, &mut |node| {
        let event = &node["args"]["event"];
        if matches!(node["kind"].as_str(), Some("Triggered" | "CreateTrigger"))
            && source_event(event)
            && source_damage(event).is_none()
        {
            name_closures(&mut node["args"]["body"]);
        }
        Visit::Children
    });
}

fn name_closures(body: &mut Value) {
    rewrite(body, &mut |node| {
        if node["kind"] == "ReflexiveTrigger" {
            // Its announcement too: "target creature defending player controls".
            name(&mut node["args"], &json!({"kind": "Source"}));
            return Visit::Skip;
        }
        if boundary(node) == Boundary::Ability || node["kind"] == "CreateTrigger" {
            Visit::Skip
        } else {
            Visit::Children
        }
    });
}

/// A trigger created for exactly one captured target names that target in its body.
pub(super) fn name_captured_targets(ability: &mut Value) {
    rewrite(ability, &mut |node| {
        if node["kind"] == "CreateTrigger"
            && let Some((subject, target)) = captured_target(&node["args"]["event"])
        {
            name_role(&mut node["args"]["body"], &subject, &target);
        }
        Visit::Children
    });
}

/// The role a created trigger's object or damage-source event captures, and the one
/// target it is. A zone change keeps its subject: the matched object and its new arrival
/// differ. The damage recipient is a different object from the captured source.
fn captured_target(event: &Value) -> Option<(Value, Value)> {
    let (objects, subject) = if let Some(object) = event.get("Object") {
        (&object["objects"], json!({"kind": "EventSubject"}))
    } else if let Some(damage) = event.get("Damage")
        && damage["subject"] == "Source"
    {
        (
            &damage["sources"],
            json!({"kind": "Damage", "args": "Source"}),
        )
    } else {
        return None;
    };
    let target = &objects["args"];
    (objects["kind"] == "SameObject" && target["kind"] == "Target" && target["args"]["index"] == 0)
        .then(|| (subject, target.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subject_is_named_up_to_the_next_independent_event() {
        let subject = json!({"kind": "EventSubject"});
        let mut body = json!([subject, {"kind":"CreateTrigger","args":{"body":[subject]}}]);
        name(&mut body, &json!({"kind": "Source"}));
        assert_eq!(body[0], json!({"kind": "Source"}));
        assert_eq!(body[1]["args"]["body"][0], subject);
    }

    #[test]
    fn a_closure_under_the_sources_own_trigger_names_the_source() {
        let subject = json!({"kind": "EventSubject"});
        let closure = json!({"kind":"ReflexiveTrigger","args":{"body":[subject]}});
        let source = json!({"kind":"SameObject","args":{"kind":"Source"}});
        let mut dies = json!({"kind":"Triggered","args":{
            "event":{"ZoneChange":{"objects":source,"from":"Battlefield","to":"Graveyard"}},
            "body":[subject, closure]}});
        name_reflexive_sources(&mut dies);
        assert_eq!(dies["args"]["body"][0], subject);
        assert_eq!(dies["args"]["body"][1]["args"]["body"][0]["kind"], "Source");
    }

    #[test]
    fn an_enclosed_independent_ability_names_its_own_source() {
        let subject = json!({"kind": "EventSubject"});
        let source = json!({"kind":"SameObject","args":{"kind":"Source"}});
        let held = |object: &Value| {
            json!({"kind":"Triggered","args":{"event":{"Object":{"objects":source}},
                "intervening_if":{"kind":"ObjectMatches","args":{"object":object}},
                "body":{"instructions":[{"kind":"One","args":subject}]}}})
        };
        let closure = |object: &Value| json!({"kind":"ReflexiveTrigger","args":{"body":{"instructions":[held(object)]}}});
        let mut created = closure(&subject);
        name_enclosed_sources(&mut created);
        assert_eq!(created, closure(&json!({"kind": "Source"})));
        let mut open = held(&subject);
        name_enclosed_sources(&mut open);
        assert_eq!(open, held(&subject));
    }

    #[test]
    fn the_source_connives_under_its_own_event() {
        let subject = json!({"kind": "EventSubject"});
        let connive = |object: &Value| {
            json!({"kind":"Keyword","args":{"actor":{"kind":"One","args":{"kind":"ControllerOf",
                "args":object}},"action":{"Connive":{"object":object}}}})
        };
        let source = json!({"kind":"SameObject","args":{"kind":"Source"}});
        let targeted = |object: &Value| {
            json!({"kind":"Triggered","args":{"event":{"BecomesTarget":{"objects":source}},
                "body":{"instructions":[connive(object)]}}})
        };
        let mut own = targeted(&subject);
        name_conniving_sources(&mut own);
        assert_eq!(own, targeted(&json!({"kind": "Source"})));
        let mut other = targeted(&subject);
        other["args"]["event"]["BecomesTarget"]["objects"] = json!({"kind": "Any"});
        let unchanged = other.clone();
        name_conniving_sources(&mut other);
        assert_eq!(other, unchanged);
    }

    #[test]
    fn a_card_put_into_a_hand_is_duplicated_as_it_last_existed() {
        let conjure = |card: Value| json!({"kind":"Conjure","args":{"card":card}});
        let trigger = |to: &str, card: Value| {
            json!({"kind":"Triggered","args":{"event":{"ZoneChange":{"to":to}},
                "body":{"instructions":[conjure(card)]}}})
        };
        let mut drawn = trigger("Hand", duplicate());
        assert!(duplicate_hidden_arrivals(&mut drawn, true));
        assert_eq!(drawn, trigger("Hand", last_known_duplicate()));
        assert!(duplicate_hidden_arrivals(&mut drawn, false));
        assert_eq!(drawn, trigger("Hand", duplicate()));
        let mut entered = trigger("Battlefield", duplicate());
        assert!(!duplicate_hidden_arrivals(&mut entered, true));
    }

    #[test]
    fn only_a_captured_object_or_damage_source_is_named() {
        let target = json!({"kind":"Target","args":{"slot":0,"index":0}});
        let same = json!({"kind":"SameObject","args":target});
        let blocked = json!({"Object":{"event":"BecomesBlocked","objects":same}});
        let subject = json!({"kind": "EventSubject"});
        assert_eq!(captured_target(&blocked), Some((subject, target.clone())));
        let dealt = json!({"Damage":{"sources":same,"subject":"Source"}});
        let source = json!({"kind": "Damage", "args": "Source"});
        assert_eq!(captured_target(&dealt), Some((source, target)));
        let received = json!({"Damage":{"sources":same,"subject":"Recipient"}});
        assert_eq!(captured_target(&received), None);
        let entered = json!({"ZoneChange":{"objects":same,"to":"Battlefield"}});
        assert_eq!(captured_target(&entered), None);
    }
}
