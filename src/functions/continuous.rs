//! Split a continuous clause into maximal runs with independent evaluation.
//! Keeping run order makes the inverse linear and preserves every interleaving.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

fn creature_animation(change: &Value) -> bool {
    (change["kind"] == "AddCardType" && change["args"] == "Creature")
        || (change["kind"] == "SetCardTypes" && change["args"] == json!(["Creature"]))
}

/// CR 205.1b: an explicit retention phrase adds the animation's creature types.
/// Only the immediate animation changes, never a granted ability, are visited.
pub(crate) fn retain_animation_types(value: &Value) -> Result<Value, String> {
    let changes = value.as_array().ok_or("retained animation needs changes")?;
    if changes.len() > 64
        || changes
            .iter()
            .filter(|c| c["kind"] == "SetBasePowerToughness")
            .count()
            != 1
        || changes.iter().filter(|c| creature_animation(c)).count() != 1
        || changes
            .iter()
            .filter(|c| c["kind"] == "CreatureTypes")
            .count()
            > 8
    {
        return Err("retained animation needs one creature animation".into());
    }
    let mut output = changes.clone();
    for change in &mut output {
        if change["kind"] == "SetCardTypes" && creature_animation(change) {
            *change = json!({"kind":"AddCardType","args":"Creature"});
        }
        if change["kind"] == "CreatureTypes" {
            if !matches!(change["args"]["mode"].as_str(), Some("Set" | "Add")) {
                return Err("retained animation needs set or additive creature types".into());
            }
            change["args"]["mode"] = json!("Add");
        }
    }
    Ok(Value::Array(output))
}

pub(crate) fn unify_retained_animation_types<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else {
        return Vec::new();
    };
    if !retain_animation_types(value).is_ok_and(|retained| retained == *value) {
        return Vec::new();
    }
    retained_preimages(value)
        .into_iter()
        .flat_map(|changes| inner.unify_made(&changes, &bindings))
        .collect()
}

fn retained_preimages(value: &Value) -> Vec<Value> {
    let indices: Vec<usize> = value
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter_map(|(i, c)| (c["kind"] == "CreatureTypes").then_some(i))
        .collect();
    let creature = value
        .as_array()
        .unwrap()
        .iter()
        .position(creature_animation)
        .expect("validated retained creature animation");
    // At most eight subtype changes and one card-type choice: 512 preimages.
    (0..1usize << (indices.len() + 1))
        .map(|mask| {
            let mut candidate = value.clone();
            if mask & (1 << indices.len()) != 0 {
                candidate[creature] = json!({"kind":"SetCardTypes","args":["Creature"]});
            }
            for (bit, index) in indices.iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    candidate[*index]["args"]["mode"] = json!("Set");
                }
            }
            candidate
        })
        .collect()
}

pub(crate) fn retain_animation(value: &Value) -> Result<Value, String> {
    if value["kind"] != "CreateContinuousEffect" {
        return Err("retention needs a continuous animation".into());
    }
    let changes = retain_animation_types(&value["args"]["changes"])?;
    let mut output = value.clone();
    output["args"]["changes"] = changes;
    Ok(output)
}

pub(crate) fn unify_retained_animation<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else {
        return Vec::new();
    };
    if !retain_animation(value).is_ok_and(|retained| retained == *value) {
        return Vec::new();
    }
    retained_preimages(&value["args"]["changes"])
        .into_iter()
        .flat_map(|changes| {
            let mut candidate = value.clone();
            candidate["args"]["changes"] = changes;
            inner.unify_made(&candidate, &bindings)
        })
        .collect()
}

fn rules(change: &Value) -> bool {
    matches!(change["kind"].as_str(), Some("Rule" | "PlayerRule"))
}

fn evaluation(rule: bool) -> Value {
    json!(if rule { "LiveObjects" } else { "Snapshot" })
}

fn changes(value: &Value) -> Result<&[Value], String> {
    if value["kind"] != "Effect"
        || value
            .pointer("/args/operation/kind")
            .and_then(Value::as_str)
            != Some("CreateContinuousEffect")
    {
        return Err("@continuous_parts needs a continuous-effect instruction".into());
    }
    value
        .pointer("/args/operation/args/changes")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .map(Vec::as_slice)
        .ok_or_else(|| "@continuous_parts needs nonempty changes".into())
}

pub(crate) fn parts(template: &Value) -> Result<Value, String> {
    let changes = changes(template)?;
    let mut base = template.clone();
    base["args"]["operation"]["args"]["changes"] = json!([]);
    let mut output = Vec::new();
    let mut start = 0;
    while start < changes.len() {
        let rule = rules(&changes[start]);
        let mut end = start + 1;
        while end < changes.len() && rules(&changes[end]) == rule {
            end += 1;
        }
        let mut part = base.clone();
        let fields = &mut part["args"]["operation"]["args"];
        fields["changes"] = json!(changes[start..end]);
        fields["evaluation"] = evaluation(rule);
        output.push(part);
        start = end;
    }
    Ok(Value::Array(output))
}

fn common(value: &Value) -> Value {
    let mut value = value.clone();
    value["args"].as_object_mut().unwrap().remove("result");
    let fields = value["args"]["operation"]["args"].as_object_mut().unwrap();
    fields.remove("changes");
    fields.remove("evaluation");
    super::plain(&value)
}

fn combined(value: &Value) -> Option<Value> {
    let items = value.as_array()?;
    let first = items.first()?;
    changes(first).ok()?;
    let wanted = common(first);
    let mut all = Vec::new();
    let mut previous = None;
    for item in items {
        let changes = changes(item).ok()?;
        let rule = rules(&changes[0]);
        if previous == Some(rule)
            || changes.iter().any(|change| rules(change) != rule)
            || item.pointer("/args/operation/args/evaluation") != Some(&evaluation(rule))
            || common(item) != wanted
        {
            return None;
        }
        previous = Some(rule);
        all.extend_from_slice(changes);
    }
    let mut template = first.clone();
    // Unbinding introduces a target at one use only. Keep that introduction as
    // the shared subject while comparing the remaining uses by their address.
    if let Some(affected) = items.iter().find_map(|item| {
        let affected = item.pointer("/args/operation/args/affected")?;
        (super::plain(affected) != *affected).then_some(affected)
    }) {
        template["args"]["operation"]["args"]["affected"] = affected.clone();
    }
    template["args"]["operation"]["args"]["changes"] = json!(all);
    template["args"]["operation"]["args"]["evaluation"] = json!("Snapshot");
    Some(template)
}

pub(crate) fn unify_parts<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let ([template], Some(combined)) = (args, combined(value)) else {
        return Vec::new();
    };
    template.unify_made(&combined, &bindings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_inverse_keeps_mixed_modes_and_has_a_finite_bound() {
        let mut changes = json!([
            {"kind":"SetBasePowerToughness","args":{"power":{"kind":"Constant","args":2},"toughness":{"kind":"Constant","args":3}}},
            {"kind":"AddCardType","args":"Creature"},
            {"kind":"CreatureTypes","args":{"mode":"Add","types":{"Listed":["Bird"]}}},
            {"kind":"CreatureTypes","args":{"mode":"Add","types":{"Listed":["Angel"]}}}
        ]);
        let actual: std::collections::BTreeSet<_> = retained_preimages(&changes)
            .iter()
            .map(|v| {
                assert_eq!(retain_animation_types(v).unwrap(), changes);
                (
                    v[2]["args"]["mode"].as_str().unwrap().to_owned(),
                    v[3]["args"]["mode"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(
            actual,
            [
                ("Add", "Add"),
                ("Add", "Set"),
                ("Set", "Add"),
                ("Set", "Set")
            ]
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .into()
        );
        let extra = changes[2].clone();
        for _ in 0..6 {
            changes.as_array_mut().unwrap().push(extra.clone());
        }
        assert!(retain_animation_types(&changes).is_ok());
        assert_eq!(retained_preimages(&changes).len(), 512);
        changes.as_array_mut().unwrap().push(extra);
        assert!(retain_animation_types(&changes).is_err());
    }

    #[test]
    fn explicit_retention_preserves_prior_card_types_after_a_creature_setter() {
        let original = json!([
            {"kind":"SetBasePowerToughness","args":{"power":{"kind":"Constant","args":4},"toughness":{"kind":"Constant","args":4}}},
            {"kind":"CreatureTypes","args":{"mode":"Set","types":{"Listed":["Dragon"]}}},
            {"kind":"SetCardTypes","args":["Creature"]}
        ]);
        let kept = retain_animation_types(&original).unwrap();
        assert_eq!(kept[1]["args"]["mode"], "Add");
        assert_eq!(kept[2], json!({"kind":"AddCardType","args":"Creature"}));
        assert!(retained_preimages(&kept).contains(&original));
        for candidate in retained_preimages(&kept) {
            assert_eq!(retain_animation_types(&candidate).unwrap(), kept);
        }
        let mut unsupported = original;
        unsupported[2]["args"] = json!(["Creature", "Planeswalker"]);
        assert!(retain_animation_types(&unsupported).is_err());
    }

    fn template(changes: Value) -> Value {
        json!({"kind":"Effect","args":{"result":0,"operation":{
            "kind":"CreateContinuousEffect","args":{"source":{"kind":"Source"},
            "affected":{"Objects":{"kind":"One","args":{"kind":"Source"}}},
            "changes":changes,"duration":"EndOfTurn","evaluation":"Snapshot"}}}})
    }

    #[test]
    fn every_small_interleaving_splits_and_combines_without_losing_order() {
        for count in 1..=6 {
            for mask in 0..1 << count {
                let changes: Vec<Value> = (0..count).map(|i| {
                    if mask >> i & 1 == 1 {
                        json!({"kind":"Rule","args":{"AttackException":{"ignore":"Defender","defenders":"Any"}}})
                    } else {
                        json!({"kind":"Pump","args":{"power":{"kind":"Constant","args":i},"toughness":{"kind":"Constant","args":1}}})
                    }
                }).collect();
                let template = template(json!(changes));
                let split = parts(&template).unwrap();
                assert_eq!(combined(&split), Some(template));
            }
        }
    }

    #[test]
    fn inverse_refuses_mismatched_subjects_durations_and_evaluation() {
        let input = template(json!([
            {"kind":"Pump","args":{"power":{"kind":"Constant","args":1},"toughness":{"kind":"Constant","args":1}}},
            {"kind":"Rule","args":{"AttackException":{"ignore":"Defender","defenders":"Any"}}}
        ]));
        for field in ["source", "affected", "duration", "evaluation"] {
            let mut split = parts(&input).unwrap();
            split[1]["args"]["operation"]["args"][field] = Value::Null;
            assert!(combined(&split).is_none(), "{field}");
        }
        assert!(parts(&template(json!([]))).is_err());
        assert!(combined(&json!([])).is_none());
    }

    #[test]
    fn inverse_preserves_one_target_introduction_in_the_shared_subject() {
        let mut input = template(json!([
            {"kind":"Pump","args":{"power":{"kind":"Constant","args":1},"toughness":{"kind":"Constant","args":1}}},
            {"kind":"Rule","args":{"AttackException":{"ignore":"Defender","defenders":"Any"}}}
        ]));
        input["args"]["operation"]["args"]["affected"] = json!({"Objects":{"kind":"One","args":{
            "kind":"Target","args":{"slot":0,"index":0}}}});
        let mut split = parts(&input).unwrap();
        let mention = json!({"$sym":"target","at":0,"args":[{"slot":0}]});
        split[1]["args"]["operation"]["args"]["affected"]["Objects"]["args"]["args"]["slot"] =
            mention.clone();
        let restored = combined(&split).unwrap();
        assert_eq!(super::super::plain(&restored), input);
        assert_eq!(
            restored["args"]["operation"]["args"]["affected"]["Objects"]["args"]["args"]["slot"],
            mention
        );
    }
}
