//! A literal semantic template can skip the token-recipe rule on the way to a
//! child hole. Carry that recipe's spelling context to the hole as well.
use crate::pattern::{Bindings, Item, Pattern};
use serde_json::Value;

const HOLDER_HINT: &str = "GrantHolder:";

fn token(definition: &Value) -> Vec<String> {
    let mut context = crate::corpus::object_print_context(definition);
    context.insert(0, "Token".to_string());
    context
}

/// The context a value sets for the abilities inside it: a token's types, or an emblem.
pub(super) fn own(value: &Value) -> Option<Vec<String>> {
    if let Some(definition) = value.get("Defined")
        && definition["card_types"].is_array()
    {
        return Some(token(definition));
    }
    (value["kind"] == "CreateEmblem").then(|| vec!["Emblem".to_string()])
}

pub(super) fn holes<'p>(
    pattern: &'p Pattern,
    value: &Value,
    bindings: &Bindings<'_, '_>,
    current: &[String],
) -> Vec<(&'p str, Vec<String>)> {
    let mut found = Vec::new();
    let pending: Vec<_> = current
        .iter()
        .filter_map(|name| name.strip_prefix(HOLDER_HINT).map(str::to_string))
        .collect();
    let holder = (!pending.is_empty()).then_some(pending.as_slice());
    walk(pattern, value, bindings, Some(current), holder, &mut found);
    found
}

fn hole_context(context: &[String], holder: Option<&[String]>) -> Vec<String> {
    let mut local = context.to_vec();
    if let Some(holder) = holder {
        local.retain(|name| !name.starts_with(HOLDER_HINT));
        local.extend(holder.iter().map(|name| format!("{HOLDER_HINT}{name}")));
    }
    local
}

fn recipient(value: &Value, current: Option<&[String]>) -> Option<Vec<String>> {
    if !matches!(
        value["kind"].as_str(),
        Some("CreateContinuousEffect" | "Static")
    ) {
        return None;
    }
    let objects = value.pointer("/args/affected/Objects")?;
    if objects["kind"] == "One" && objects["args"]["kind"] == "Source" {
        // An animated card goes by the type it gains: `this creature`, not `this land`.
        let gained = value.pointer("/args/changes").and_then(Value::as_array);
        let gained = gained
            .into_iter()
            .flatten()
            .filter(|c| c["kind"] == "AddCardType");
        let mut context: Vec<String> = gained
            .filter_map(|change| change["args"].as_str().map(str::to_string))
            .collect();
        // Pending receiver hints are metadata, not fresh receiver types. Copying
        // their encoded names would grow the context on grammar cycles.
        return current.map(|current| {
            context.extend(
                current
                    .iter()
                    .filter(|name| name.as_str() != "Holder" && !name.starts_with(HOLDER_HINT))
                    .filter(|name| !context.contains(name))
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            context
        });
    }
    if objects["kind"] == "Created" {
        return Some(vec!["Token".into(), "Permanent".into()]);
    }
    let (filter, kind) = if objects["kind"] == "Zone"
        && objects["args"]["scope"]["Shared"] == "Battlefield"
        && objects["args"]["kind"] == "Permanent"
    {
        (&objects["args"]["filter"], "Permanent")
    } else {
        let spec = objects.pointer("/args/args/slot/args/0")?;
        (
            &spec["filter"],
            spec.pointer("/kind/Object/kind")?.as_str()?,
        )
    };
    fn names(filter: &Value, out: &mut Vec<String>) {
        match filter["kind"].as_str() {
            Some("CardType" | "Subtype") => {
                if let Some(name) = filter["args"].as_str() {
                    out.push(name.to_string());
                }
            }
            Some("AllOf") => {
                if let Some(filters) = filter["args"].as_array() {
                    for filter in filters {
                        names(filter, out);
                    }
                }
            }
            _ => {}
        }
    }
    let mut context = Vec::new();
    names(filter, &mut context);
    if kind == "Permanent" {
        context.push(kind.to_string());
    } else {
        // A creature spell is still named as a spell, and a card in another
        // zone as a card, ahead of its intrinsic characteristic nouns.
        context.insert(0, kind.to_string());
    }
    (!context.is_empty()).then_some(context)
}

fn walk<'p>(
    pattern: &'p Pattern,
    value: &Value,
    bindings: &Bindings<'_, '_>,
    inherited: Option<&[String]>,
    holder: Option<&[String]>,
    found: &mut Vec<(&'p str, Vec<String>)>,
) {
    let own = own(value);
    let context = own.as_deref().or(inherited);
    // At @holder the value is already the child ability, whose affected objects
    // are not the receiver that supplied this pending holder context.
    let recipient = if matches!(pattern, Pattern::Symbol(name, _) if name == "holder") {
        None
    } else {
        recipient(value, context)
    };
    let holder = recipient
        .as_deref()
        .or(if own.is_some() { None } else { holder });
    match pattern {
        Pattern::Hole(name, _) => {
            if let Some(context) = context {
                found.push((name, hole_context(context, holder)));
            }
        }
        Pattern::Object(fields) => {
            for (name, child) in fields {
                if let Some(item) = value.get(name) {
                    walk(child, item, bindings, context, holder, found);
                }
            }
        }
        Pattern::List(pieces) => {
            let Some(items) = value.as_array() else {
                return;
            };
            let mut at = 0;
            for piece in pieces {
                match piece {
                    Item::One(child) => {
                        if let Some(item) = items.get(at) {
                            walk(child, item, bindings, context, holder, found);
                        }
                        at += 1;
                    }
                    Item::Splice(name, _) => {
                        if let Some(context) = context {
                            found.push((name, hole_context(context, holder)));
                        }
                        at += bindings
                            .get(name)
                            .and_then(Value::as_array)
                            .map_or(0, Vec::len);
                    }
                }
            }
        }
        Pattern::Symbol(name, args) if name == "holder" => {
            if let Some(inner) = args.first() {
                let created = |c: &&[String]| c.iter().any(|n| n == "Token" || n == "Emblem");
                let known = holder.or(context.filter(created));
                // An unknown receiver still establishes a new holder. The
                // creator's types would otherwise favor its printed name,
                // which names a different object inside the granted ability.
                let mut local = known.unwrap_or_default().to_vec();
                local.retain(|name| name != "Holder" && !name.starts_with(HOLDER_HINT));
                local.insert(0, "Holder".to_string());
                walk(inner, value, bindings, Some(&local), None, found);
            }
        }
        Pattern::Symbol(name, args)
            if matches!(
                name.as_str(),
                "scope"
                    | "implicit_return_grant"
                    | "return_grant"
                    | "manifest_result"
                    | "exiled_card_result"
                    | "search_move_result"
                    | "seek_move_result"
                    | "created_result"
                    | "selected_result"
                    | "owner_return"
                    | "sacrificed_result"
                    | "measured_sacrifice"
                    | "resolution"
                    | "leading_if"
                    | "distributive_pump_subject"
                    | "pump_other_ref"
                    | "pump_name_ref"
            ) =>
        {
            if let Some(inner) = args.first() {
                walk(inner, value, bindings, context, holder, found);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ir_schema::Ty;
    use serde_json::json;

    fn child_context(current: &[&str], holder: bool) -> Vec<String> {
        let child = Pattern::Hole("ability".into(), Ty::Named("Ability".into()));
        let pattern = if holder {
            Pattern::Symbol("holder".into(), vec![child])
        } else {
            child
        };
        holes(
            &pattern,
            &json!({"kind":"Triggered","args":{}}),
            &Bindings::default(),
            &current.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
        )[0]
        .1
        .clone()
    }

    #[test]
    fn unknown_grant_holder_drops_the_creators_type_context() {
        assert_eq!(child_context(&["Instant"], true), vec!["Holder"]);
    }

    #[test]
    fn a_known_receiver_keeps_its_own_types() {
        assert_eq!(
            child_context(&["Instant", "GrantHolder:Creature"], true),
            vec!["Holder", "Creature"]
        );
        assert_eq!(
            child_context(&["Token", "Artifact"], true),
            vec!["Holder", "Token", "Artifact"]
        );
    }

    #[test]
    fn an_ordinary_hole_preserves_the_creator_context() {
        assert_eq!(child_context(&["Instant"], false), vec!["Instant"]);
    }
}
