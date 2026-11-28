//! Abilities that canonical comparison joins, offered to printing both ways.
//! Triggers that share everything but their event mean one trigger on either
//! event, so printing also offers that joined trigger: "enters or leaves the
//! battlefield" can print as written. A joined block pair names the other
//! creature `Other`, which under "becomes blocked by" alone is its `Blocker`;
//! the joined view reads it that way. Statics in a row for the same objects are
//! held as one static with all their changes, so printing also offers them
//! split again: where the lowered abilities held them apart, and at each cut,
//! since one sentence can lower to several statics. Reparse still decides
//! whether a text means the same abilities.
use crate::{bind::presentation, model::canonical};
use serde_json::{Value, json};

fn blocker_to_other(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if object.get("kind").and_then(Value::as_str) == Some("BlockPair")
                && object.get("args").and_then(Value::as_str) == Some("Blocker")
            {
                object.insert("args".into(), json!("Other"));
                return;
            }
            object.values_mut().for_each(blocker_to_other);
        }
        Value::Array(items) => items.iter_mut().for_each(blocker_to_other),
        _ => {}
    }
}

/// The ability as a branch of a joined trigger.
fn branch(ability: &Value) -> Value {
    let mut view = ability.clone();
    if view["args"]["event"]["Pair"]["event"] == "BecomesBlockedBy" {
        blocker_to_other(&mut view["args"]["body"]);
    }
    view
}

/// Whether two triggered abilities differ in nothing but what triggers them.
fn same_but_event(a: &Value, b: &Value) -> bool {
    a["kind"] == "Triggered"
        && b["kind"] == "Triggered"
        && ["intervening_if", "limits", "announcement", "body"]
            .iter()
            .all(|field| a["args"][*field] == b["args"][*field])
}

fn events(ability: &Value) -> Vec<Value> {
    match ability["args"]["event"].get("AnyOf") {
        Some(Value::Array(cases)) => cases.clone(),
        _ => vec![ability["args"]["event"].clone()],
    }
}

/// One ability in canonical form.
fn canonical_one(ability: &Value) -> Option<Value> {
    let mut one = Value::Array(vec![ability.clone()]);
    canonical::abilities(&mut one).ok()?;
    match one {
        Value::Array(mut items) if items.len() == 1 => items.pop(),
        _ => None,
    }
}

/// Two adjacent triggers as branches of one, when they differ only in their
/// events: as lowered, or else once canonical form has equated their bodies
/// (a body that names the dying source as the event's subject, say).
fn join_pair(last: &Value, item: &Value) -> Option<(Value, Value)> {
    let (a, b) = (branch(last), branch(item));
    if same_but_event(&a, &b) {
        return Some((a, b));
    }
    if a["kind"] != "Triggered" || b["kind"] != "Triggered" {
        return None;
    }
    let (a, b) = (canonical_one(&a)?, canonical_one(&b)?);
    same_but_event(&a, &b).then_some((a, b))
}

/// Adjacent same-but-event triggers joined into one, in their order, or `None`
/// when no two adjacent abilities join.
pub(super) fn joined(abilities: &Value) -> Option<Value> {
    let items = abilities.as_array()?;
    let mut merged: Vec<Value> = Vec::with_capacity(items.len());
    let mut any = false;
    for item in items {
        let pair = merged.last().and_then(|last| join_pair(last, item));
        match (pair, merged.last_mut()) {
            (Some((first, then)), Some(last)) => {
                let mut cases = events(&first);
                cases.extend(events(&then));
                *last = first;
                last["args"]["event"] = json!({ "AnyOf": cases });
                any = true;
            }
            _ => merged.push(item.clone()),
        }
    }
    any.then_some(Value::Array(merged))
}

/// Statics with this many changes or fewer are offered split.
const CHANGES: usize = 4;
/// At most this many split views.
const SPLITS: usize = 8;

/// The static at `index` split before each change in `cuts`.
fn split_at(items: &[Value], index: usize, cuts: &[usize]) -> Value {
    let item = &items[index];
    let changes = item["args"]["changes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut out = items[..index].to_vec();
    let mut start = 0;
    for &end in cuts.iter().chain(std::iter::once(&changes.len())) {
        let mut part = item.clone();
        part["args"]["changes"] = Value::Array(changes[start..end].to_vec());
        out.push(part);
        start = end;
    }
    out.extend_from_slice(&items[index + 1..]);
    Value::Array(out)
}

fn changes(item: &Value) -> usize {
    item["args"]["changes"].as_array().map_or(0, Vec::len)
}

/// The reduced abilities with each static split where the lowered abilities
/// held separate statics, or `None` when reduction joined none or the two do
/// not line up. Each lowered ability is reduced alone to count its changes, but
/// everything else is the reduced form's own.
pub(super) fn lowered_apart(lowered: &Value, reduced: &Value) -> Option<Value> {
    let mut alone = Vec::new();
    for item in lowered.as_array()? {
        let mut one = Value::Array(vec![item.clone()]);
        canonical::reduced(&mut one).ok()?;
        alone.extend(one.as_array()?.iter().cloned());
    }
    split_like(&alone, reduced)
}

/// `reduced` with its statics cut where `alone` holds them apart.
fn split_like(alone: &[Value], reduced: &Value) -> Option<Value> {
    let mut items = reduced.as_array()?.clone();
    let mut next = 0;
    let mut index = 0;
    let mut split = false;
    while index < items.len() {
        let total = changes(&items[index]);
        let mut cuts = Vec::new();
        let mut seen = changes(alone.get(next)?);
        next += 1;
        if items[index]["kind"] == "Static" {
            while seen < total {
                let part = alone.get(next).filter(|part| part["kind"] == "Static")?;
                cuts.push(seen);
                seen += changes(part);
                next += 1;
            }
            if seen != total {
                return None;
            }
        }
        if cuts.is_empty() {
            index += 1;
            continue;
        }
        split = true;
        let parts = cuts.len() + 1;
        items = split_at(&items, index, &cuts).as_array()?.clone();
        index += parts;
    }
    (split && next == alone.len()).then_some(Value::Array(items))
}

/// Each joined static split into one static per change, and at each single
/// point, in order. A written form can also group four changes across two
/// boundaries, such as a single change, a pair, and a single change.
pub(super) fn split_statics(abilities: &Value, written: bool) -> Vec<Value> {
    let Some(items) = abilities.as_array() else {
        return Vec::new();
    };
    let mut views = Vec::new();
    let mut paired = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let core = presentation::core(item);
        if core["kind"] == "Class" {
            views.extend(split_levels(items, index, core, written));
            continue;
        }
        let count = changes(item);
        if item["kind"] != "Static" || !(2..=CHANGES).contains(&count) {
            continue;
        }
        let every: Vec<usize> = (1..count).collect();
        views.push(split_at(items, index, &every));
        if count > 2 {
            views.extend(every.iter().map(|cut| split_at(items, index, &[*cut])));
        }
        if written && count == 4 {
            for cuts in [[1, 2], [1, 3], [2, 3]] {
                paired.push(split_at(items, index, &cuts));
            }
        }
    }
    views.extend(paired);
    views.truncate(SPLITS);
    views
}

/// A Class level holds its own abilities, and a static joined within one level
/// splits the same way. Levels hold no Class, so this recursion ends.
fn split_levels(items: &[Value], index: usize, class: &Value, written: bool) -> Vec<Value> {
    let Some(levels) = class["args"]["levels"].as_array() else {
        return Vec::new();
    };
    let mut views = Vec::new();
    for (at, level) in levels.iter().enumerate() {
        for split in split_statics(&level["abilities"], written) {
            let mut view = items.to_vec();
            presentation::core_mut(&mut view[index])["args"]["levels"][at]["abilities"] = split;
            views.push(Value::Array(view));
        }
    }
    views
}

/// Triggers on at most this many events are offered split.
const EVENTS: usize = 3;

/// The groups a trigger's events split into: each event alone, in the order
/// held and, for two, the other way round; and for three, the first two
/// together or the last two.
fn event_groups(cases: &[Value]) -> Vec<Vec<Vec<Value>>> {
    let single = |order: &[Value]| order.iter().map(|case| vec![case.clone()]).collect();
    let mut groups = vec![single(cases)];
    match cases {
        [first, second] => groups.push(single(&[second.clone(), first.clone()])),
        [first, second, third] => {
            groups.push(vec![
                vec![first.clone(), second.clone()],
                vec![third.clone()],
            ]);
            groups.push(vec![
                vec![first.clone()],
                vec![second.clone(), third.clone()],
            ]);
        }
        _ => {}
    }
    groups
}

fn joined_event_cases(ability: &Value) -> Option<&[Value]> {
    if ability["kind"] != "Triggered" {
        return None;
    }
    ability["args"]["event"]["AnyOf"]
        .as_array()
        .map(Vec::as_slice)
        .filter(|cases| (2..=EVENTS).contains(&cases.len()))
}

pub(super) fn has_joined_events(abilities: &Value) -> bool {
    abilities
        .as_array()
        .is_some_and(|items| items.iter().any(|item| joined_event_cases(item).is_some()))
}

/// Each trigger on any of a few events split into a trigger per group of its
/// events. "Whenever you cast a green spell and whenever a Forest you control
/// enters" writes two triggers that canonical form holds as one on either event.
pub(super) fn split_events(abilities: &Value) -> Vec<Value> {
    let Some(items) = abilities.as_array() else {
        return Vec::new();
    };
    let mut views = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(cases) = joined_event_cases(item) else {
            continue;
        };
        for groups in event_groups(cases) {
            let mut out = items[..index].to_vec();
            for mut group in groups {
                let mut part = item.clone();
                part["args"]["event"] = match group.len() {
                    1 => group.remove(0),
                    _ => json!({ "AnyOf": group }),
                };
                out.push(part);
            }
            out.extend_from_slice(&items[index + 1..]);
            views.push(Value::Array(out));
        }
    }
    views.truncate(SPLITS);
    views
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trigger(event: &str, body: i64) -> Value {
        json!({"kind":"Triggered","args":{"event":{"Object":{"event":event}},
            "intervening_if":null,"limits":{},"announcement":{"targets":[]},"body":body}})
    }

    #[test]
    fn adjacent_triggers_with_one_body_join_in_order() {
        let abilities = json!([
            trigger("Blocks", 1),
            trigger("BecomesBlocked", 1),
            trigger("Dies", 2)
        ]);
        let one = joined(&abilities).unwrap();
        assert_eq!(one.as_array().unwrap().len(), 2);
        assert_eq!(
            one[0]["args"]["event"],
            json!({"AnyOf":[{"Object":{"event":"Blocks"}},{"Object":{"event":"BecomesBlocked"}}]})
        );
        assert_eq!(one[1], trigger("Dies", 2));
        assert!(joined(&json!([trigger("Blocks", 1), trigger("Dies", 2)])).is_none());
    }

    #[test]
    fn a_block_pair_joins_with_the_blocker_read_as_the_other_creature() {
        let pair = |event: &str, role: &str| {
            json!({"kind":"Triggered","args":{"event":{"Pair":{"event":event}},
                "intervening_if":null,"limits":{},"announcement":{"targets":[]},
                "body":{"objects":{"kind":"BlockPair","args":role}}}})
        };
        let one = joined(&json!([
            pair("Blocks", "Other"),
            pair("BecomesBlockedBy", "Blocker")
        ]))
        .unwrap();
        assert_eq!(one.as_array().unwrap().len(), 1);
        assert_eq!(one[0]["args"]["body"]["objects"]["args"], "Other");
        assert!(
            joined(&json!([
                pair("Blocks", "Blocker"),
                pair("BecomesBlockedBy", "Other")
            ]))
            .is_none()
        );
    }

    #[test]
    fn a_joined_static_splits_where_it_was_lowered_apart() {
        let fixed =
            |changes: Value| json!({"kind":"Static","args":{"affected":1,"changes":changes}});
        let alone = [
            fixed(json!(["a"])),
            fixed(json!(["b", "c"])),
            fixed(json!(["d"])),
            json!("after"),
        ];
        let reduced = json!([fixed(json!(["a", "b", "c", "d"])), "after"]);
        assert_eq!(
            split_like(&alone, &reduced),
            Some(Value::Array(alone.to_vec()))
        );
        assert_eq!(split_like(&alone[..1], &json!([fixed(json!(["a"]))])), None);
        assert_eq!(split_like(&alone[..2], &reduced), None);
    }

    #[test]
    fn a_joined_static_splits_in_order() {
        let fixed = json!({"kind":"Static","args":{"affected":1,"changes":["a","b","c"]}});
        let views = split_statics(&json!([fixed, "after"]), false);
        let changes = |view: &Value| -> Vec<Value> {
            view.as_array()
                .unwrap()
                .iter()
                .map(|item| item["args"]["changes"].clone())
                .collect()
        };
        assert_eq!(views.len(), 3);
        assert_eq!(
            changes(&views[0]),
            vec![json!(["a"]), json!(["b"]), json!(["c"]), Value::Null]
        );
        assert_eq!(
            changes(&views[1]),
            vec![json!(["a"]), json!(["b", "c"]), Value::Null]
        );
        assert_eq!(
            changes(&views[2]),
            vec![json!(["a", "b"]), json!(["c"]), Value::Null]
        );
        assert!(
            split_statics(&json!([{"kind":"Static","args":{"changes":["a"]}}]), false).is_empty()
        );
    }

    #[test]
    fn a_trigger_on_either_event_splits_per_event_in_both_orders() {
        let event = |name: &str| json!({"Object":{"event":name}});
        let mut joined = trigger("Blocks", 1);
        joined["args"]["event"] = json!({"AnyOf":[event("Blocks"), event("Dies")]});
        let events = |view: &Value| -> Vec<Value> {
            view.as_array()
                .unwrap()
                .iter()
                .map(|item| item["args"]["event"].clone())
                .collect()
        };
        let views = split_events(&json!([joined.clone(), "after"]));
        assert_eq!(views.len(), 2);
        assert_eq!(
            events(&views[0]),
            vec![event("Blocks"), event("Dies"), Value::Null]
        );
        assert_eq!(
            events(&views[1]),
            vec![event("Dies"), event("Blocks"), Value::Null]
        );
        assert_eq!(views[0][0]["args"]["body"], 1);
        joined["args"]["event"] = json!({"AnyOf":[event("a"), event("b"), event("c")]});
        let views = split_events(&json!([joined]));
        assert_eq!(views.len(), 3);
        assert_eq!(
            events(&views[1]),
            vec![json!({"AnyOf":[event("a"), event("b")]}), event("c")]
        );
        assert!(split_events(&json!([trigger("Blocks", 1)])).is_empty());
    }
}
