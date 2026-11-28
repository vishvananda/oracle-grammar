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
    let left_label = &left["args"]["label"];
    let right_label = &right["args"]["label"];
    if !left_label.is_null() && !right_label.is_null() && left_label != right_label {
        return None;
    }
    let tail = ordinary_instructions(right)?;
    if let Some(program) = kicker_program(left) {
        return (program["instructions"].as_array()?.len() < MAX_SPLIT_INSTRUCTIONS
            && independent_kicker_tail(tail))
        .then_some(tail);
    }
    ordinary_instructions(left).map(|_| tail)
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
            let body = if kicker_program(paragraph).is_some() {
                &mut paragraph["args"][1]["args"]["body"]
            } else {
                &mut paragraph["args"]["body"]
            };
            if let Some(list) = body["instructions"].as_array_mut() {
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
    for surface in surfaces {
        let Some(abilities) = surface.as_array() else {
            continue;
        };
        for (index, ability) in abilities.iter().enumerate() {
            let Some(parts) = written_kicker_suffix(ability) else {
                continue;
            };
            let mut split = abilities.clone();
            split.splice(index..=index, parts);
            let view = Value::Array(split);
            if !results.contains(&view) {
                results.push(view);
                if results.len() == MAX_SPLITS {
                    return results;
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

// A targetless, independent continuation resolves on both sides of the
// kicker decision. Check the exact raw operand forms before sharing the frame.
fn kicker_program(value: &Value) -> Option<&Value> {
    let branch = unpresented(value);
    (branch["$sym"] == "kicker_branch" && branch["args"][1]["kind"] == "Spell")
        .then_some(&branch["args"][1]["args"]["body"])
}

fn constant(value: &Value) -> bool {
    value["kind"] == "Constant" && value["args"].is_i64()
}

fn independent_kicker_tail(body: &[Value]) -> bool {
    let [item] = body else { return false };
    if item["kind"] != "Effect" {
        return false;
    }
    let operation = &item["args"]["operation"];
    let operation = if operation["$sym"] == "created_result" {
        &operation["args"][0]
    } else {
        operation
    };
    let args = &operation["args"];
    let you =
        |player: &Value| player["kind"] == "One" && player["args"]["kind"] == "AbilityController";
    match operation["kind"].as_str() {
        Some("Draw") => you(&args["players"]) && constant(&args["amount"]),
        Some("Move") => {
            let objects = &args["objects"];
            let actor = &objects["args"];
            objects["kind"] == "One"
                && (actor["kind"] == "Source"
                    || matches!(actor["$sym"].as_str(), Some("self" | "named_source")))
                && args["destination"]["Exile"]["visibility"] == "FaceUp"
                && args["qualifiers"].as_array().is_some_and(Vec::is_empty)
        }
        Some("CreateContinuousEffect") => {
            let subjects = &args["affected"]["Objects"];
            let subjects = if subjects["$sym"] == "distributive_pump_subject" {
                &subjects["args"][0]
            } else {
                subjects
            };
            subjects["kind"] == "Zone"
                && subjects["args"]["scope"]["Shared"] == "Battlefield"
                && subjects["args"]["kind"] == "Permanent"
                && subjects["args"]["filter"] == json!({"kind":"CardType","args":"Creature"})
                && args["evaluation"] == "Snapshot"
                && args["duration"] == "EndOfTurn"
                && args["changes"].as_array().is_some_and(|c| {
                    matches!(c.as_slice(), [pump] if pump["kind"] == "Pump"
                        && constant(&pump["args"]["power"]) && constant(&pump["args"]["toughness"]))
                })
        }
        Some("CreateTokens") => {
            you(&args["players"])
                && args["tokens"].as_array().is_some_and(|tokens| {
                    matches!(tokens.as_slice(), [token] if constant(&token["amount"])
                    && token["recipe"]["Defined"]["name"] == "FromSubtypes"
                    && token["recipe"]["Defined"]["abilities_of"].is_null())
                })
                && args["entry"]["controller"].is_null()
                && args["entry"]["counters"]
                    .as_array()
                    .is_some_and(Vec::is_empty)
        }
        _ => false,
    }
}

// A following "that creature enters with ..." describes the target of the
// preceding reanimation. A replacement on the source would lose that target.
pub(super) fn detached_entry(items: &[Value]) -> bool {
    items.windows(2).any(|pair| {
        let first = unpresented(&pair[0]);
        let next = unpresented(&pair[1]);
        let Some([instruction]) = ordinary_instructions(first).map(Vec::as_slice) else {
            return false;
        };
        let move_card = &instruction["args"]["operation"];
        let objects = &move_card["args"]["objects"];
        let objects = if objects["$sym"] == "zone_members" {
            &objects["args"][0]
        } else {
            objects
        };
        let target = &objects["args"];
        let domain = &target["args"]["slot"]["args"][0];
        let args = &next["args"];
        let qualifier = &args["event"]["Enter"]["objects"];
        let reference = &qualifier["args"];
        let reference = if reference["$sym"] == "remember_operand" {
            &reference["args"][0]
        } else {
            reference
        };
        let Some([branch]) = args["body"]["instructions"].as_array().map(Vec::as_slice) else {
            return false;
        };
        let follow = &branch["args"]["then"]["instructions"];
        instruction["kind"] == "Effect"
            && move_card["kind"] == "Move"
            && move_card["args"]["destination"]
                .get("Battlefield")
                .is_some()
            && objects["kind"] == "One"
            && target["kind"] == "Target"
            && domain["kind"]["Object"]["zones"] == json!(["Graveyard"])
            && domain["count"] == json!({"Exactly":{"kind":"Constant","args":1}})
            && next["kind"] == "Replacement"
            && args["active"] == "SourceEntering"
            && qualifier["kind"] == "SameObject"
            && reference["$sym"] == "qualified_object"
            && reference["args"][1] == json!({"kind":"CardType","args":"Creature"})
            && branch["kind"] == "If"
            && branch["args"]["otherwise"]["instructions"] == json!([])
            && follow.as_array().is_some_and(|actions| {
                matches!(actions.as_slice(), [effect] if effect["kind"] == "Effect"
                    && effect["args"]["operation"]["kind"] == "ModifyEvent"
                    && !effect["args"]["operation"]["args"]["change"]["AddCounters"].is_null())
            })
    })
}

// Exact replay may place a shared final instruction in its own paragraph.
// Parsing the split view binds the instruction back into both resolutions.
fn written_kicker_suffix(ability: &Value) -> Option<[Value; 2]> {
    let branch = unpresented(ability);
    if branch["kind"] != "CostBranch" || branch["args"]["keyword"] != "Kicker" {
        return None;
    }
    let paid = ordinary_instructions(&branch["args"]["paid"])?;
    let unpaid = ordinary_instructions(&branch["args"]["unpaid"])?;
    let last = paid.last()?;
    let other = unpaid.last()?;
    if paid.len() > MAX_SPLIT_INSTRUCTIONS
        || unpaid.len() > MAX_SPLIT_INSTRUCTIONS
        || !independent_kicker_tail(std::slice::from_ref(last))
        || other["kind"] != "Effect"
        || last["args"]["operation"] != other["args"]["operation"]
    {
        return None;
    }
    let mut first = ability.clone();
    let args = if first["kind"] == "WithPresentation" {
        &mut first["args"]["ability"]["args"]
    } else {
        &mut first["args"]
    };
    for side in ["paid", "unpaid"] {
        args[side]["args"]["body"]["instructions"]
            .as_array_mut()?
            .pop();
    }
    let suffix = json!({"kind":"Spell","args":{
        "announcement":{"targets":[],"modes":null},
        "body":{"instructions":[last]}
    }});
    Some([first, suffix])
}
