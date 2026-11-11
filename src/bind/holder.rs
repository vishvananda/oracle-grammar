//! A token or a granted ability has a new holder. Its printed card name still
//! refers to the creating/granting card, distinct from `this creature`.
use super::{Scope, bind_scope, json, scope_for, symbol_name};
use serde_json::Value;

/// The IR identifies changes of holder even when a mined declaration leaves an
/// ability literal or uses the older generic scope marker. Modes and delayed
/// triggers retain their source; token definitions and grants introduce a holder.
pub(super) fn mark(value: &mut Value, holders: u16) {
    mark_in(value, holders, false);
}

fn mark_in(value: &mut Value, holders: u16, own: bool) {
    if symbol_name(value) == Some("holder") {
        return;
    }
    let own = same_source(value, holders, own);
    if value["kind"] == "GrantAbility" {
        wrap(&mut value["args"], own);
    }
    if value["kind"] == "CreateEmblem"
        && let Some(abilities) = value["args"]["abilities"].as_array_mut()
    {
        for ability in abilities {
            wrap(ability, false);
        }
    }
    if let Some(abilities) = value
        .pointer_mut("/Defined/abilities")
        .and_then(Value::as_array_mut)
    {
        for ability in abilities {
            wrap(ability, false);
        }
    }
    match value {
        Value::Object(items) => items
            .values_mut()
            .for_each(|item| mark_in(item, holders, own)),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| mark_in(item, holders, own)),
        _ => {}
    }
}

pub(super) fn same_source(value: &Value, holders: u16, inherited: bool) -> bool {
    if let Some(affected) = value.pointer("/args/affected/Objects") {
        affected["kind"] == "One"
            && (affected["args"]["kind"] == "Source"
                || symbol_name(&affected["args"]) == Some("self")
                || holders == 0 && symbol_name(&affected["args"]) == Some("named_source"))
    } else {
        inherited || value["args"]["active"] == "SourceEntering"
    }
}

fn wrap(value: &mut Value, same_source: bool) {
    match symbol_name(value) {
        Some("holder") => {}
        Some("scope") => value["$sym"] = json!("holder"),
        _ => {
            let at = first_position(value).unwrap_or(0);
            *value = json!({"$sym":"holder", "at":at, "args":[value.take()]});
        }
    }
    if same_source {
        value["same_source"] = json!(true);
    }
}

fn first_position(value: &Value) -> Option<u64> {
    if symbol_name(value).is_some() {
        return value["at"].as_u64();
    }
    match value {
        Value::Object(items) => items.values().filter_map(first_position).min(),
        Value::Array(items) => items.iter().filter_map(first_position).min(),
        _ => None,
    }
}

pub(super) fn bind(node: &mut Value, outer: &Scope) -> Result<Value, String> {
    let mut inner = node["args"]
        .get_mut(0)
        .ok_or("an ability holder needs its contents")?
        .take();
    let holders = if node["same_source"] == true {
        outer.holders
    } else {
        outer.holders.checked_add(1).ok_or("ability holder depth")?
    };
    let departure_controller = super::departure_controller::prepare(&mut inner)?;
    event_sources(&mut inner, holders)?;
    let mut scope = scope_for(&inner, &outer.card_name, holders);
    super::departure_controller::seed(&mut scope, departure_controller);
    scope.binding_floor = outer.binding_floor;
    bind_scope(&mut inner, &mut scope)?;
    Ok(inner)
}

pub(super) fn named_source(scope: &mut Scope, acted: bool) -> Result<Value, String> {
    let reference = reference(scope.holders, scope.leaving)?;
    if acted {
        scope.objects.push(reference.clone());
    }
    Ok(reference)
}

fn reference(holders: u16, leaving: bool) -> Result<Value, String> {
    Ok(match holders {
        0 if leaving => json!({"kind":"EventSubject"}),
        0 => json!({"kind":"Source"}),
        1 => json!({"kind":"GrantingObject"}),
        // The IR has no reference for an arbitrary chain of granters. Refuse
        // rather than silently name the immediate granter instead of the card.
        _ => return Err("a card name crosses more than one ability holder".into()),
    })
}

/// A holder's own target/tap event still refers to that source. This
/// must not mistake a spell *targeting* the source for the source's event.
pub(super) fn self_event(event: &Value, holders: u16) -> bool {
    if let Some(inner) = event.get("FirstEachTurn") {
        return self_event(inner, holders);
    }
    fn filter(value: &Value, holders: u16) -> bool {
        match value["kind"].as_str() {
            Some("SameObject") => {
                value["args"]["kind"] == "Source"
                    || symbol_name(&value["args"]) == Some("self")
                    || holders == 0 && symbol_name(&value["args"]) == Some("named_source")
            }
            Some("AllOf") => value["args"]
                .as_array()
                .is_some_and(|parts| parts.iter().any(|v| filter(v, holders))),
            _ => false,
        }
    }
    for wrapper in ["Filtered", "NthEachTurn"] {
        if let Some(inner) = event.pointer(&format!("/{wrapper}/event")) {
            return self_event(inner, holders);
        }
    }
    if let Some(cases) = event.get("AnyOf").and_then(Value::as_array) {
        return !cases.is_empty() && cases.iter().all(|event| self_event(event, holders));
    }
    for kind in [
        "Object",
        "ZoneChange",
        "ZoneChangeFromElsewhere",
        "Pair",
        "BecomesTarget",
    ] {
        if filter(&event[kind]["objects"], holders) {
            return true;
        }
    }
    // Damage roles already have their own referents and canonical reduction.
    false
}

/// Resolve the event's source roles before it establishes pronoun and leaving
/// context. Those roles need no target/result binding or spelling metadata.
pub(super) fn event_sources(ability: &mut Value, holders: u16) -> Result<(), String> {
    let ability = super::presentation::core_mut(ability);
    fn source_filter(object: &mut Value, holders: u16) -> Result<(), String> {
        if object["kind"] == "SameObject" {
            match symbol_name(&object["args"]) {
                Some("self") => object["args"] = json!({"kind":"Source"}),
                Some("named_source") => object["args"] = reference(holders, false)?,
                _ => {}
            }
        } else if object["kind"] == "AllOf"
            && let Some(parts) = object["args"].as_array_mut()
        {
            for part in parts {
                source_filter(part, holders)?;
            }
        }
        Ok(())
    }
    fn walk(value: &mut Value, holders: u16) -> Result<(), String> {
        for kind in [
            "Object",
            "ZoneChange",
            "ZoneChangeFromElsewhere",
            "Pair",
            "Player",
            "Counters",
        ] {
            if let Some(object) = value.pointer_mut(&format!("/{kind}/objects")) {
                source_filter(object, holders)?;
            }
        }
        for path in [
            "/Damage/sources",
            "/Damage/recipients/Objects",
            "/Attack/attackers",
        ] {
            if let Some(object) = value.pointer_mut(path) {
                source_filter(object, holders)?;
            }
        }
        // References nested in spell filters must still introduce their normal
        // referents during binding (a spell targeting this creature is heroic).
        if let Some(cases) = value.get_mut("AnyOf").and_then(Value::as_array_mut) {
            for event in cases {
                walk(event, holders)?;
            }
        }
        for wrapper in ["FirstEachTurn", "FirstForObjectEachTurn", "FirstEachGame"] {
            if let Some(event) = value.get_mut(wrapper) {
                walk(event, holders)?;
            }
        }
        for wrapper in [
            "Filtered",
            "NthEachTurn",
            "Caused",
            "WhileActivating",
            "Batch",
        ] {
            if let Some(event) = value
                .get_mut(wrapper)
                .and_then(|value| value.get_mut("event"))
            {
                walk(event, holders)?;
            }
        }
        Ok(())
    }
    if let Some(event) = ability.pointer_mut("/args/event") {
        walk(event, holders)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(symbol: &str) -> Value {
        json!({"kind":"SameObject","args":{"$sym":symbol,"args":[]}})
    }

    #[test]
    fn direct_damage_roles_bind_before_holder_identity_is_proven() {
        for (symbol, expected) in [("self", "Source"), ("named_source", "GrantingObject")] {
            let mut ability = json!({"kind":"Triggered","args":{"event":{"Damage":{
                "subject":"Source","sources":same(symbol),
                "recipients":{"Objects":same(symbol)}
            }}}});
            event_sources(&mut ability, 1).unwrap();
            let event = &ability["args"]["event"];
            assert_eq!(event["Damage"]["sources"]["args"]["kind"], expected);
            assert_eq!(
                event["Damage"]["recipients"]["Objects"]["args"]["kind"],
                expected
            );
            assert_eq!(
                super::super::other::source_damage(event).is_some(),
                symbol == "self"
            );
        }
    }

    #[test]
    fn counter_batches_distinguish_granted_holder_and_named_creator() {
        for (symbol, expected) in [("self", "Source"), ("named_source", "GrantingObject")] {
            let mut ability = json!({"kind":"Triggered","args":{"event":{"Batch":{
                "per":"Subject","event":{"Counters":{"objects":same(symbol)}}
            }}}});
            event_sources(&mut ability, 1).unwrap();
            let event = &ability["args"]["event"];
            assert_eq!(
                event["Batch"]["event"]["Counters"]["objects"]["args"]["kind"],
                expected
            );
            assert_eq!(super::super::other::source_event(event), symbol == "self");
        }
    }

    #[test]
    fn attack_identity_uses_the_holder_without_capturing_a_named_creator() {
        for (symbol, expected) in [("self", "Source"), ("named_source", "GrantingObject")] {
            let mut ability = json!({"kind":"Triggered","args":{"event":{"Attack":{
                "attackers":same(symbol),"by":null,"defenders":{"Players":{"kind":"All"}}
            }}}});
            event_sources(&mut ability, 1).unwrap();
            let event = &ability["args"]["event"];
            assert_eq!(event["Attack"]["attackers"]["args"]["kind"], expected);
            assert_eq!(super::super::other::source_event(event), symbol == "self");
        }
    }

    #[test]
    fn contextual_pronoun_cannot_supply_a_fixed_holder_proof() {
        let mut ability = json!({"kind":"Triggered","args":{"event":{"Damage":{
            "subject":"Source","sources":same("it"),"recipients":{"Players":{"kind":"All"}}
        }}}});
        let original = ability.clone();
        event_sources(&mut ability, 1).unwrap();
        assert_eq!(ability, original);
        assert!(!super::super::other::source_event(
            &ability["args"]["event"]
        ));
    }
}
