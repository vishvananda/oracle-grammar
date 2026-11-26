//! Bounded identity proofs for an event's already-defined distinguished object.
use serde_json::Value;

const MAX_DEPTH: usize = 32;
const MAX_WORK: usize = 128;
const MAX_TERMS: usize = 64;

pub(super) fn names_source(event: &Value) -> bool {
    let mut left = MAX_WORK;
    event_source(event, 0, &mut left)
}

fn visit(depth: usize, left: &mut usize) -> bool {
    if depth > MAX_DEPTH || *left == 0 {
        return false;
    }
    *left -= 1;
    true
}

fn fields(value: &Value, keys: &[&str]) -> bool {
    value.as_object().is_some_and(|fields| {
        fields.len() == keys.len() && keys.iter().all(|key| fields.contains_key(*key))
    })
}

fn unit_source(value: &Value) -> bool {
    value.as_object().is_some_and(|fields| fields.len() == 1) && value["kind"] == "Source"
}

fn filter_source(value: &Value, depth: usize, left: &mut usize) -> bool {
    if !visit(depth, left) || !value.as_object().is_some_and(|fields| fields.len() == 2) {
        return false;
    }
    match value["kind"].as_str() {
        Some("SameObject") => unit_source(&value["args"]),
        Some(kind @ ("AllOf" | "AnyOf")) => {
            let Some(items) = value["args"]
                .as_array()
                .filter(|items| !items.is_empty() && items.len() <= MAX_TERMS)
            else {
                return false;
            };
            if kind == "AllOf" {
                items
                    .iter()
                    .any(|item| filter_source(item, depth + 1, left))
            } else {
                items
                    .iter()
                    .all(|item| filter_source(item, depth + 1, left))
            }
        }
        _ => false,
    }
}

fn damage_filter(damage: &Value) -> Option<&Value> {
    if !fields(
        damage,
        &[
            "source_kind",
            "sources",
            "recipients",
            "kind",
            "grouping",
            "subject",
        ],
    ) {
        return None;
    }
    let grouping = damage["grouping"].as_str()?;
    match damage["subject"].as_str()? {
        "Source" if matches!(grouping, "Source" | "SourceRecipient") => Some(&damage["sources"]),
        "Recipient" if matches!(grouping, "Recipient" | "SourceRecipient") => {
            let recipients = &damage["recipients"];
            recipients
                .as_object()
                .filter(|fields| fields.len() == 1)?
                .get("Objects")
        }
        _ => None,
    }
}

fn event_source(event: &Value, depth: usize, left: &mut usize) -> bool {
    if !visit(depth, left) {
        return false;
    }
    let Some(event_fields) = event.as_object().filter(|fields| fields.len() == 1) else {
        return false;
    };
    let (kind, args) = event_fields.iter().next().expect("one event field");
    let filter = match kind.as_str() {
        "AnyOf" => {
            return args.as_array().is_some_and(|events| {
                !events.is_empty()
                    && events.len() <= MAX_TERMS
                    && events
                        .iter()
                        .all(|event| event_source(event, depth + 1, left))
            });
        }
        "Filtered" => {
            return fields(args, &["event", "condition"])
                && event_source(&args["event"], depth + 1, left);
        }
        "FirstEachTurn" => return event_source(args, depth + 1, left),
        "Batch" => {
            return fields(args, &["event", "per", "count", "including"])
                && args["per"] == "Subject"
                && event_source(&args["event"], depth + 1, left);
        }
        "Object" if fields(args, &["event", "objects"]) => &args["objects"],
        "ZoneChange" if fields(args, &["objects", "from", "to"]) => &args["objects"],
        "Pair" if fields(args, &["event", "objects", "others"]) => &args["objects"],
        "Counters" if fields(args, &["change", "kind", "objects", "by"]) => &args["objects"],
        "Attack" if fields(args, &["attackers", "by", "defenders"]) => &args["attackers"],
        "Spell" if fields(args, &["event", "players", "spells"]) => &args["spells"],
        "Damage" => {
            let Some(filter) = damage_filter(args) else {
                return false;
            };
            filter
        }
        _ => return false,
    };
    filter_source(filter, depth + 1, left)
}
