//! Adjacent ordinary spell paragraphs share a binding frame.
use serde_json::{Value, json};

const MAX_SPLIT_INSTRUCTIONS: usize = 16;

/// Ordinary spell paragraphs share a resolution. A modal announcement owns
/// its selected programs; neighboring unconditional instructions remain separate.
fn ordinary_instructions(value: &Value) -> Option<&Vec<Value>> {
    let ability = if value["kind"] == "WithPresentation" {
        &value["args"]["ability"]
    } else {
        value
    };
    if ability["kind"] == "Spell" && ability["args"]["announcement"]["modes"].is_null() {
        ability["args"]["body"]["instructions"].as_array()
    } else {
        None
    }
}

fn continuation<'a>(left: &Value, right: &'a Value) -> Option<&'a Vec<Value>> {
    ordinary_instructions(left)?;
    let left_label = &left["args"]["label"];
    let right_label = &right["args"]["label"];
    if left_label.is_null() || right_label.is_null() || left_label == right_label {
        ordinary_instructions(right)
    } else {
        None
    }
}

pub(super) fn merge(items: Vec<Value>) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::with_capacity(items.len());
    for item in items {
        if let Some(previous) = merged.last_mut()
            && let Some(more) = continuation(previous, &item)
        {
            let paragraph = if previous["kind"] == "WithPresentation" {
                if previous["args"]["label"].is_null() {
                    previous["args"]["label"] = item["args"]["label"].clone();
                }
                if item["args"]["reminder"] == true {
                    previous["args"]["reminder"] = json!(true);
                }
                &mut previous["args"]["ability"]
            } else if item["kind"] == "WithPresentation" {
                let ability = std::mem::take(previous);
                *previous = json!({
                    "kind": "WithPresentation",
                    "args": {"ability": ability, "label": item["args"]["label"],
                             "reminder": item["args"]["reminder"]},
                });
                &mut previous["args"]["ability"]
            } else {
                previous
            };
            if let Some(list) = paragraph["args"]["body"]["instructions"].as_array_mut() {
                list.extend(more.iter().cloned());
                continue;
            }
        }
        merged.push(item);
    }
    merged
}

pub(super) fn can_merge(items: &[Value]) -> bool {
    items
        .windows(2)
        .any(|pair| continuation(&pair[0], &pair[1]).is_some())
}

pub(super) fn can_split_reminded_spell(abilities: &Value) -> bool {
    abilities.as_array().is_some_and(|items| {
        items.iter().any(|ability| {
            ability["kind"] == "WithPresentation"
                && ability["args"]["reminder"] == true
                && ordinary_instructions(ability)
                    .is_some_and(|body| (2..=MAX_SPLIT_INSTRUCTIONS).contains(&body.len()))
        })
    })
}

/// Spell one bound resolution as two paragraphs for printing and replay.
/// Split unbound views so all references still belong to the merged frame.
pub(super) fn written_splits(surfaces: &[Value]) -> Vec<Value> {
    const MAX_SPLITS: usize = 64;
    let mut results = Vec::new();
    // Keep the ordinary single-wrapper spellings first under the shared cap.
    for placement in [
        ReminderPlacement::WithLabel,
        ReminderPlacement::OtherParagraph,
        ReminderPlacement::Both,
    ] {
        for surface in surfaces {
            let Some(abilities) = surface.as_array() else {
                continue;
            };
            for (index, ability) in abilities.iter().enumerate() {
                if ability["kind"] != "WithPresentation" || !placement.applies(ability) {
                    continue;
                }
                let Some(instructions) = ordinary_instructions(ability) else {
                    continue;
                };
                if instructions.len() > MAX_SPLIT_INSTRUCTIONS {
                    continue;
                }
                for split in 1..instructions.len() {
                    for label_first in [true, false] {
                        let parts =
                            split_paragraph(ability, instructions, split, label_first, placement);
                        let mut split_view = abilities.clone();
                        split_view.splice(index..=index, parts);
                        let view = Value::Array(split_view);
                        if !results.contains(&view) {
                            results.push(view);
                            if results.len() == MAX_SPLITS {
                                return results;
                            }
                        }
                    }
                }
            }
        }
    }
    results
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReminderPlacement {
    WithLabel,
    OtherParagraph,
    Both,
}

impl ReminderPlacement {
    fn applies(self, ability: &Value) -> bool {
        let args = &ability["args"];
        match self {
            Self::WithLabel => true,
            Self::OtherParagraph => args["reminder"] == true && args["label"].is_string(),
            Self::Both => args["reminder"] == true,
        }
    }
}

fn split_paragraph(
    ability: &Value,
    instructions: &[Value],
    split: usize,
    label_first: bool,
    placement: ReminderPlacement,
) -> [Value; 2] {
    let (prefix, suffix) = instructions.split_at(split);
    let spell = &ability["args"]["ability"];
    let mut first = spell.clone();
    let mut second = spell.clone();
    first["args"]["body"]["instructions"] = json!(prefix);
    second["args"]["body"]["instructions"] = json!(suffix);
    let (labelled, other) = if label_first {
        (&mut first, &mut second)
    } else {
        (&mut second, &mut first)
    };
    let mut wrapper = ability.clone();
    wrapper["args"]["ability"] = std::mem::take(labelled);
    if placement == ReminderPlacement::OtherParagraph {
        wrapper["args"]["reminder"] = json!(false);
    }
    *labelled = wrapper;
    if placement != ReminderPlacement::WithLabel {
        *other = json!({"kind":"WithPresentation","args":{"ability":std::mem::take(other),"label":null,"reminder":true}});
    }
    [first, second]
}

fn unpresented(value: &Value) -> &Value {
    if value["kind"] == "WithPresentation" {
        &value["args"]["ability"]
    } else {
        value
    }
}

/// Ascend on a resolving spell belongs to that spell's instruction sequence.
pub(super) fn split_ascend(items: &[Value]) -> bool {
    items.windows(2).any(|pair| {
        let left = unpresented(&pair[0]);
        left["kind"] == "Mechanic"
            && left["args"]["mechanic"] == "Ascend"
            && ordinary_instructions(&pair[1]).is_some()
    })
}
