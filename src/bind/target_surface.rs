//! Repeated semantic operands can represent one lexical target occurrence.
use super::{Step, at_path, slot_uses};
use serde_json::{Value, json};

/// Add one scope-wide form after the complete old enumeration. Each declared
/// slot keeps its own exact specification; only its repeated references share
/// annotation. Existing nested announcement boundaries remain unchanged.
pub(super) fn correlated(root: &Value, specs: &[Value], body: &[Step]) -> Option<Value> {
    let mut copy = root.clone();
    let program = at_path(&mut copy, body)?;
    let mut addresses = Vec::new();
    let mut plans = Vec::new();
    let mut total = 0;
    let mut repeated = false;
    // Match the existing inverse specification work/depth bounds. Charge each
    // inserted specification, so repeated annotations cannot expand without bound.
    let mut work = 32_768;
    for spec in specs {
        let address = spec["slot"].as_u64()?;
        if addresses.contains(&address) {
            return None;
        }
        addresses.push(address);
        let mut uses = Vec::new();
        slot_uses(program, address, &mut Vec::new(), &mut uses);
        total += uses.len();
        if uses.is_empty() || total > 64 {
            return None;
        }
        repeated |= uses.len() > 1;
        for _ in &uses {
            charge(spec, 0, &mut work)?;
        }
        plans.push((spec, uses));
    }
    if !repeated {
        return None;
    }
    for (spec, uses) in plans {
        let marker = json!({"$sym":"target","args":[spec],"at":0});
        for path in uses {
            *at_path(program, &path)? = marker.clone();
        }
    }
    Some(copy)
}

fn charge(value: &Value, depth: usize, work: &mut usize) -> Option<()> {
    if depth > 128 {
        return None;
    }
    *work = work.checked_sub(1)?;
    match value {
        Value::Array(items) => items
            .iter()
            .try_for_each(|item| charge(item, depth + 1, work)),
        Value::Object(fields) => fields
            .values()
            .try_for_each(|item| charge(item, depth + 1, work)),
        _ => Some(()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{chosen_form, path_of, slot_forms};
    use super::*;

    fn target(slot: u64) -> Value {
        json!({"kind":"Target","args":{"slot":slot,"index":0}})
    }

    fn spec(slot: u64) -> Value {
        json!({"slot":slot,"kind":{"Object":{"kind":"Permanent","zones":["Battlefield"]}},
            "count":{"Exactly":{"kind":"Constant","args":1}},
            "filter":{"kind":"CardType","args":"Creature"}})
    }

    fn frame(actor: u64, object: u64) -> Value {
        json!({"args":{"body":{"instructions":[{"kind":"Effect","args":{"result":7,"operation":{"kind":"Keyword","args":{"actor":{"kind":"One","args":{"kind":"ControllerOf","args":target(actor)}},"action":{"Connive":{"object":target(object),"amount":null}}}}}}]}}})
    }

    fn operation(value: &Value) -> &Value {
        &value["args"]["body"]["instructions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|instruction| instruction["args"]["operation"]["kind"] == "Keyword")
            .unwrap()["args"]["operation"]["args"]
    }

    #[test]
    fn shared_shifted_slot_adds_one_form_after_every_original_form() {
        let root = frame(3, 3);
        let spec = spec(3);
        let body = path_of(&["args", "body"]);
        let mut uses = Vec::new();
        slot_uses(&root["args"]["body"], 3, &mut Vec::new(), &mut uses);
        assert_eq!(uses.len(), 2);
        let mut original = Vec::new();
        for path in uses {
            let mut copy = root.clone();
            *at_path(at_path(&mut copy, &body).unwrap(), &path).unwrap() =
                json!({"$sym":"target","args":[spec.clone()],"at":0});
            original.push(copy);
        }
        original.push(chosen_form(&root, 0, &spec, &body).unwrap());
        let actual = slot_forms(&root, &[spec], &body);
        assert_eq!(&actual[..original.len()], original.as_slice());
        assert_eq!(actual.len(), original.len() + 1);
        let paired = operation(actual.last().unwrap());
        assert_eq!(
            paired["actor"]["args"]["args"],
            paired["action"]["Connive"]["object"]
        );
        assert_eq!(
            paired["action"]["Connive"]["object"]["args"]["slot"]["args"][0]["slot"],
            3
        );
        assert_eq!(
            actual.last().unwrap()["args"]["body"]["instructions"][0]["args"]["result"],
            7
        );
        assert_eq!(root, frame(3, 3));
    }

    #[test]
    fn distinct_same_filter_targets_never_become_one_operand() {
        let root = frame(0, 1);
        let body = path_of(&["args", "body"]);
        assert!(correlated(&root, &[spec(0)], &body).is_none());
        assert!(correlated(&root, &[spec(1)], &body).is_none());
        assert!(correlated(&frame(0, 0), &[spec(1)], &body).is_none());
        for form in slot_forms(&root, &[spec(0), spec(1)], &body) {
            let operation = operation(&form);
            assert_ne!(
                operation["actor"]["args"]["args"],
                operation["action"]["Connive"]["object"]
            );
        }
    }

    #[test]
    fn nested_owner_with_same_local_slot_is_not_correlated() {
        let body = path_of(&["args", "body"]);
        let mut root = frame(0, 1);
        root["args"]["body"]["instructions"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "kind":"GrantAbility","args":{"kind":"Activated","args":{
                    "announcement":{"targets":[],"modes":null},
                    "body":{"instructions":[{"kind":"Effect","args":{"operation":{
                        "kind":"Keyword","args":{"object":target(0)}
                    }}}]}
                }}
            }));
        assert!(correlated(&root, &[spec(0)], &body).is_none());
    }

    #[test]
    fn full_old_capacity_does_not_admit_an_extra_candidate() {
        let body = path_of(&["args", "body"]);
        let mut root = frame(0, 0);
        let item = root["args"]["body"]["instructions"][0].clone();
        root["args"]["body"]["instructions"] = Value::Array(vec![item; 40]);
        assert!(correlated(&root, &[spec(0)], &body).is_none());
        let forms = slot_forms(&root, &[spec(0)], &body);
        // Preserve the old 64 single-use forms plus its trailing chosen form;
        // tightening that pre-existing overshoot is outside this change.
        assert_eq!(forms.len(), 65);
        assert_eq!(
            forms.last().unwrap()["args"]["body"]["instructions"][0]["$sym"],
            "choose"
        );
    }

    #[test]
    fn later_slot_capacity_is_not_amplified_by_correlated_parents() {
        let body = path_of(&["args", "body"]);
        let mut root = frame(0, 0);
        for index in 0..80 {
            root["args"]["body"]["instructions"].as_array_mut().unwrap().push(json!({
                "kind":"Effect","args":{"result":8+index,"operation":{
                    "kind":"Destroy","args":{"objects":{"kind":"One","args":target(1)},"regeneration":true}
                }}
            }));
        }
        // Old slot0 has three parent variants. At slot1 the first produces
        // 64+chosen, each remaining parent produces one+chosen: exactly69.
        assert_eq!(slot_forms(&root, &[spec(0), spec(1)], &body).len(), 69);
    }

    #[test]
    fn indices_specs_and_annotation_work_remain_distinct_and_bounded() {
        let body = path_of(&["args", "body"]);
        assert!(correlated(&frame(0, 0), &[spec(0), spec(0)], &body).is_none());
        let mut indexed = frame(0, 0);
        indexed["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["action"]["Connive"]
            ["object"]["args"]["index"] = 1.into();
        let mut grouped = spec(0);
        grouped["count"] = json!({"Exactly":{"kind":"Constant","args":2}});
        let paired = correlated(&indexed, &[grouped], &body).unwrap();
        let paired = operation(&paired);
        assert_eq!(paired["actor"]["args"]["args"]["args"]["index"], 0);
        assert_eq!(paired["action"]["Connive"]["object"]["args"]["index"], 1);
        assert_ne!(
            paired["actor"]["args"]["args"],
            paired["action"]["Connive"]["object"]
        );
        let mut large = spec(0);
        large["filter"] = json!({"kind":"Any","terms":vec![Value::Null;17_000]});
        assert!(correlated(&frame(0, 0), &[large], &body).is_none());
        let mut nested = Value::Null;
        for _ in 0..130 {
            nested = json!({"nested":nested});
        }
        let mut deep = spec(0);
        deep["filter"] = nested;
        assert!(correlated(&frame(0, 0), &[deep], &body).is_none());
    }
}
