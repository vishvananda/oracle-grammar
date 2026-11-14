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
use crate::model::canonical;
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

/// Adjacent same-but-event triggers joined into one, in their order, or `None`
/// when no two adjacent abilities join.
pub(super) fn joined(abilities: &Value) -> Option<Value> {
    let items = abilities.as_array()?;
    let mut merged: Vec<Value> = Vec::with_capacity(items.len());
    let mut any = false;
    for item in items {
        let view = branch(item);
        match merged.last_mut() {
            Some(last) if same_but_event(&branch(last), &view) => {
                *last = branch(last);
                let mut cases = events(last);
                cases.extend(events(&view));
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
/// point, in order.
pub(super) fn split_statics(abilities: &Value) -> Vec<Value> {
    let Some(items) = abilities.as_array() else {
        return Vec::new();
    };
    let mut views = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let count = changes(item);
        if item["kind"] != "Static" || !(2..=CHANGES).contains(&count) {
            continue;
        }
        let every: Vec<usize> = (1..count).collect();
        views.push(split_at(items, index, &every));
        if count > 2 {
            views.extend(every.iter().map(|cut| split_at(items, index, &[*cut])));
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
        let views = split_statics(&json!([fixed, "after"]));
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
        assert!(split_statics(&json!([{"kind":"Static","args":{"changes":["a"]}}])).is_empty());
    }
}
