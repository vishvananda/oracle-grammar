//! Lexically marked current roles following one actual scalar graveyard return.
use super::{Scope, Step, at_path, references::TargetIntroduction, symbol_name};
use crate::model::ir::{
    Count, EventZone, Expr, MovementDestination, ObjectKind, ObjectRef, ObjectSet, Operation,
    ResultId, TargetKind,
};
use serde_json::{Value, json};

const TYPE: &str = "move_after_current_type";
const ACTOR: &str = "move_after_actor";
const PROPERTY: &str = "move_after_property";
const OTHER: &str = "move_after_other";

fn at<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get(key)?,
            Step::Index(i) => value.get(*i)?,
        };
    }
    Some(value)
}
fn marker(name: &str) -> bool {
    matches!(name, TYPE | ACTOR | PROPERTY | OTHER)
}
fn unwrap(mut value: &Value) -> &Value {
    for _ in 0..16 {
        if matches!(
            symbol_name(value),
            Some("remember_operand" | "member_count_it" | "in_filter")
        ) {
            value = &value["args"][0];
        } else {
            break;
        }
    }
    value
}
fn pronoun(value: &Value) -> bool {
    let value = unwrap(value);
    symbol_name(value) == Some("it")
        || (symbol_name(value) == Some("qualified_object")
            && value["args"][0] == "Permanent"
            && value["args"][1] == json!({"kind":"CardType","args":"Creature"}))
}
fn previous<'a>(root: &'a Value, path: &[Step]) -> Option<(&'a [Value], usize)> {
    let (depth, index) = path.iter().enumerate().rev().find_map(|(depth, step)| {
        if let Step::Index(index) = step
            && matches!(depth.checked_sub(1).and_then(|n| path.get(n)), Some(Step::Key(k)) if k == "instructions")
        { Some((depth, *index)) } else { None }
    })?;
    let values = at(root, &path[..depth])?.as_array()?;
    (index > 0 && index < values.len()).then_some((values.as_slice(), index))
}
pub(super) fn input_spec(operation: &Value) -> Option<&Value> {
    if operation["kind"] != "Move" {
        return None;
    }
    let mut objects = &operation["args"]["objects"];
    while matches!(
        symbol_name(objects),
        Some("zone_members" | "exile_followup")
    ) {
        objects = &objects["args"][0];
    }
    if objects["kind"] != "One" {
        return None;
    }
    let object = unwrap(&objects["args"]);
    if object["kind"] != "Target" || object["args"]["index"] != 0 {
        return None;
    }
    let slot = &object["args"]["slot"];
    (symbol_name(slot) == Some("target")).then_some(&slot["args"][0])
}
fn creature(filter: &Value) -> bool {
    filter == &json!({"kind":"CardType","args":"Creature"})
        || (filter["kind"] == "AllOf"
            && filter["args"].as_array().is_some_and(|parts| {
                parts
                    .iter()
                    .any(|part| part == &json!({"kind":"CardType","args":"Creature"}))
            }))
}
pub(super) fn eligible(operation: &Value) -> Result<bool, String> {
    let Some(spec) = input_spec(operation) else {
        return Ok(false);
    };
    if !bounded(operation, 0, &mut 131_072, &mut 1_048_576) {
        return Err("move After operation work/byte/depth bound".into());
    }
    let Ok(TargetKind::Object(domain)) = serde_json::from_value(spec["kind"].clone()) else {
        return Ok(false);
    };
    let Ok(Count::Exactly(Expr::Constant(1))) = serde_json::from_value(spec["count"].clone())
    else {
        return Ok(false);
    };
    Ok(domain.kind == ObjectKind::Card
        && domain.zones == [EventZone::Graveyard]
        && creature(&spec["filter"])
        && (operation["args"]["destination"]
            .get("Battlefield")
            .is_some()))
}
fn owner<'a>(root: &'a Value, path: &[Step]) -> Option<(&'a Value, usize)> {
    for depth in (0..path.len()).rev() {
        let operation = at(root, &path[..depth])?;
        if operation["kind"] == "DealDamage" {
            return Some((operation, depth));
        }
    }
    None
}
fn role(root: &Value, path: &[Step], name: &str) -> bool {
    if path.len() < 2 {
        return false;
    }
    let parent = at(root, &path[..path.len() - 2]).unwrap_or(&Value::Null);
    let field = path.last();
    match name {
        TYPE => {
            parent["kind"] == "ObjectMatches"
                && matches!(field, Some(Step::Key(k)) if k == "object")
        }
        ACTOR => {
            parent["kind"] == "DealDamage" && matches!(field, Some(Step::Key(k)) if k == "source")
        }
        PROPERTY => {
            parent["kind"] == "ObjectProperty"
                && parent["args"]["property"] == "ManaValue"
                && parent["args"]["information"] == "CurrentOrLastKnown"
                && path.len() >= 4
                && matches!(&path[path.len()-3], Step::Key(k) if k == "amount")
                && at(root, &path[..path.len() - 4]).is_some_and(|v| v["kind"] == "LoseLife")
        }
        OTHER => {
            let mut object_path = path.to_vec();
            if path.len() >= 2
                && at(root, &path[..path.len() - 2])
                    .is_some_and(|v| symbol_name(v) == Some("pump_other_ref"))
            {
                object_path.truncate(path.len() - 2);
            }
            let same = object_path
                .last()
                .is_some_and(|s| matches!(s,Step::Key(k) if k == "args"))
                && at(root, &object_path[..object_path.len() - 1])
                    .is_some_and(|v| v["kind"] == "SameObject")
                && object_path.len() >= 2
                && at(root, &object_path[..object_path.len() - 2])
                    .is_some_and(|v| v["kind"] == "Not");
            same && owner(root, path).is_some_and(|(op, depth)| {
                let recipients = &op["args"]["recipients"];
                let mut objects = &recipients["args"];
                if symbol_name(objects) == Some("distributive_pump_subject") {
                    objects = &objects["args"][0];
                }
                matches!(path.get(depth+1),Some(Step::Key(k)) if k == "recipients")
                    && recipients["kind"] == "Objects"
                    && objects["kind"] == "Zone"
                    && objects["args"]["scope"]["Shared"] == "Battlefield"
                    && objects["args"]["kind"] == "Permanent"
                    && symbol_name(&op["args"]["source"]) == Some(ACTOR)
                    && pronoun(&op["args"]["source"]["args"][0])
            })
        }
        _ => false,
    }
}

/// Remove unrelated annotations before holder guards and target identity capture.
/// Eligible annotations retain every original subtree until finish_scope.
pub(super) fn prepare(root: &mut Value) -> Result<(), String> {
    fn collect(
        value: &Value,
        path: &mut Vec<Step>,
        found: &mut Vec<Vec<Step>>,
        work: &mut usize,
    ) -> Result<(), String> {
        if path.len() > 128 || *work == 0 {
            return Err("move After preparation bound".into());
        }
        *work -= 1;
        if symbol_name(value).is_some_and(marker) {
            if found.len() >= 4096 {
                return Err("move After annotation bound".into());
            }
            found.push(path.clone());
        }
        match value {
            Value::Object(fields) => {
                for (key, child) in fields {
                    path.push(Step::Key(key.clone()));
                    collect(child, path, found, work)?;
                    path.pop();
                }
            }
            Value::Array(items) => {
                for (i, child) in items.iter().enumerate() {
                    path.push(Step::Index(i));
                    collect(child, path, found, work)?;
                    path.pop();
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut found = Vec::new();
    collect(root, &mut Vec::new(), &mut found, &mut 131_072)?;
    for path in found.into_iter().rev() {
        let node = at(root, &path).ok_or("move After preparation path")?;
        let name = symbol_name(node).ok_or("move After annotation")?;
        let candidate = role(root, &path, name) && (name == OTHER || pronoun(&node["args"][0]));
        let keep = if candidate {
            match previous(root, &path) {
                Some((values, index)) => eligible(&values[index - 1]["args"]["operation"])?,
                None => false,
            }
        } else {
            false
        };
        if !keep {
            let node = at_path(root, &path).ok_or("move After preparation path")?;
            *node = node["args"]
                .get_mut(0)
                .ok_or("move After needs one object")?
                .take();
        }
    }
    Ok(())
}

fn bounded(value: &Value, depth: usize, work: &mut usize, bytes: &mut usize) -> bool {
    if depth > 128 || *work == 0 {
        return false;
    }
    *work -= 1;
    let charge = |remaining: &mut usize, size: usize| match remaining.checked_sub(size) {
        Some(left) => {
            *remaining = left;
            true
        }
        None => false,
    };
    match value {
        Value::String(text) => charge(bytes, text.len()),
        Value::Object(fields) => fields
            .iter()
            .all(|(key, child)| charge(bytes, key.len()) && bounded(child, depth + 1, work, bytes)),
        Value::Array(values) => values.iter().all(|v| bounded(v, depth + 1, work, bytes)),
        _ => true,
    }
}

struct Receipt {
    input: ObjectRef,
    result: ResultId,
}
fn receipt(
    root: &Value,
    path: &[Step],
    keys: &[TargetIntroduction],
    base: usize,
) -> Result<Option<Receipt>, String> {
    let Some((values, index)) = previous(root, path) else {
        return Ok(None);
    };
    if values.len() > 256 {
        return Err("move After instruction bound".into());
    }
    let instruction = &values[index - 1];
    if instruction["kind"] != "Effect" || !eligible(&instruction["args"]["operation"])? {
        return Ok(None);
    }
    let result: ResultId = serde_json::from_value(instruction["args"]["result"].clone())
        .map_err(|_| "move After needs actual result address")?;
    if values[..index]
        .iter()
        .filter(|v| v["kind"] == "Effect" && v["args"]["result"] == json!(result))
        .count()
        != 1
    {
        return Err("ambiguous move After result address".into());
    }
    if keys.len() > 256
        || keys
            .iter()
            .try_fold(0_usize, |sum, (key, _)| sum.checked_add(key.1.len()))
            .is_none_or(|bytes| bytes > 1_048_576)
    {
        return Err("move After target-key work/byte bound".into());
    }
    let mut operation =
        super::references::bound_reference(&instruction["args"]["operation"], keys, base)?;
    super::filters::strip(&mut operation);
    super::objects::resolve_shapes(&mut operation)?;
    let typed: Operation = serde_json::from_value(operation)
        .map_err(|why| format!("move After typed operation: {why}"))?;
    let Operation::Move {
        objects: ObjectSet::One(input),
        destination: MovementDestination::Battlefield { .. },
        ..
    } = typed
    else {
        return Ok(None);
    };
    Ok(Some(Receipt { input, result }))
}
fn after(receipt: &Receipt) -> Value {
    json!({"kind":"Result","args":{"result":receipt.result,"phase":"After","index":0}})
}

pub(super) fn reference(
    root: &Value,
    path: &[Step],
    keys: &[TargetIntroduction],
    base: usize,
    reference: &Value,
) -> Result<Option<Value>, String> {
    for depth in (0..path.len()).rev() {
        let Some(node) = at(root, &path[..depth]) else {
            continue;
        };
        let Some(name) = symbol_name(node).filter(|name| marker(name)) else {
            continue;
        };
        if name == OTHER || !role(root, &path[..depth], name) {
            return Ok(None);
        }
        let mut direct = path[..depth].to_vec();
        direct.extend([Step::Key("args".into()), Step::Index(0)]);
        while at(root, &direct)
            .is_some_and(|v| matches!(symbol_name(v), Some("remember_operand" | "member_count_it")))
        {
            direct.extend([Step::Key("args".into()), Step::Index(0)]);
        }
        if direct != path {
            return Ok(None);
        }
        let Some(receipt) = receipt(root, path, keys, base)? else {
            return Ok(None);
        };
        return Ok((json!(receipt.input) == *reference).then(|| after(&receipt)));
    }
    Ok(None)
}

/// Result actors are supported only through this exact validated lexical role.
pub(super) fn actor(
    root: &Value,
    path: &[Step],
    keys: &[TargetIntroduction],
    base: usize,
    value: &Value,
) -> Result<bool, String> {
    let Some(operation) = at(root, path) else {
        return Ok(false);
    };
    if symbol_name(&operation["args"]["source"]) != Some(ACTOR) {
        return Ok(false);
    }
    let mut instruction = path.to_vec();
    instruction.extend([Step::Key("args".into()), Step::Key("source".into())]);
    Ok(receipt(root, &instruction, keys, base)?.is_some_and(|r| after(&r) == *value))
}

pub(super) fn bind_marker(
    root: &mut Value,
    scope: &Scope,
    path: &[Step],
    name: &str,
    keys: &[TargetIntroduction],
) -> Result<bool, String> {
    if name != OTHER {
        return Ok(false);
    }
    let (operation, depth) = owner(root, path).ok_or("move After other needs damage actor")?;
    let mut actor_value = operation["args"]["source"].clone();
    super::filters::strip(&mut actor_value);
    let operation_path = path[..depth].to_vec();
    if !actor(root, &operation_path, keys, scope.target_base, &actor_value)? {
        return Err("move After other needs proved actor".into());
    }
    let node = at_path(root, path).ok_or("move After other path")?;
    node["args"][0] = actor_value;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn library_preflight_actual_full_native_frames_and_prior_controls() {
        use crate::engine::{Engine, canonical_abilities};
        let engine = Engine::load().unwrap();
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/move-after-information.json"
        ))
        .unwrap();
        for case in cases {
            let mut expected: Vec<_> = case["expected_readings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|reading| canonical_abilities(reading).unwrap())
                .collect();
            expected.sort_by_key(Value::to_string);
            expected.dedup();
            let mut actual = engine
                .parse(
                    case["name"].as_str().unwrap(),
                    case["text"].as_str().unwrap(),
                )
                .unwrap()
                .readings;
            actual.sort_by_key(Value::to_string);
            assert_eq!(actual, expected, "{}", case["name"]);
            assert!(actual.contains(&canonical_abilities(&case["native_reference"]).unwrap()));
        }
    }

    fn target(slot: usize) -> Value {
        json!({"kind":"Target","args":{"slot":slot,"index":0}})
    }
    fn spec() -> Value {
        json!({"slot":{"$sym":"slot","at":1},"kind":{"Object":{"kind":"Card","zones":["Graveyard"]}},"count":{"Exactly":{"kind":"Constant","args":1}},"filter":{"kind":"CardType","args":"Creature"},"players":null,"distribute":null,"constraints":[],"chooser":null})
    }
    fn frame() -> (Value, Vec<Step>, Vec<TargetIntroduction>) {
        let declaration = spec();
        let input = json!({"kind":"Target","args":{"slot":{"$sym":"target","at":1,"args":[declaration.clone()]},"index":0}});
        let root = json!({"instructions":[{"kind":"Effect","args":{"result":11,"operation":{"kind":"Move","args":{"objects":{"$sym":"zone_members","args":[{"kind":"One","args":input}]},"destination":{"Battlefield":{"controller":"Owner","arrival":{"controller":null,"tapped":false,"counters":[],"states":[]}}},"qualifiers":[]}}}},{"kind":"If","args":{"condition":{"kind":"ObjectMatches","args":{"object":{"$sym":TYPE,"args":[{"$sym":"it","args":[],"at":20}]},"filter":{"kind":"Subtype","args":"Elf"}}},"then":{"instructions":[]},"otherwise":{"instructions":[]}}}]});
        (
            root,
            vec![
                Step::Key("instructions".into()),
                Step::Index(1),
                Step::Key("args".into()),
                Step::Key("condition".into()),
                Step::Key("args".into()),
                Step::Key("object".into()),
                Step::Key("args".into()),
                Step::Index(0),
            ],
            vec![((1, declaration.to_string()), 2)],
        )
    }
    #[test]
    fn actual_return_result_with_assigned_target_offset_and_deferred_shape() {
        let (mut root, path, keys) = frame();
        let original = root.clone();
        prepare(&mut root).unwrap();
        assert_eq!(root, original);
        assert_eq!(
            reference(&root, &path, &keys, 5, &target(7)).unwrap(),
            Some(json!({"kind":"Result","args":{"result":11,"phase":"After","index":0}}))
        );
        assert_eq!(reference(&root, &path, &keys, 5, &target(6)).unwrap(), None);
        assert_eq!(root, original);
    }
    #[test]
    fn literal_other_survives_the_objects_each_distributive_annotation() {
        let (mut root, _, _) = frame();
        root["instructions"][1] = json!({"kind":"Effect","args":{"result":12,"operation":{"kind":"DealDamage","args":{"source":{"$sym":ACTOR,"args":[{"$sym":"remember_operand","args":[{"$sym":"qualified_object","args":["Permanent",{"kind":"CardType","args":"Creature"}]}]}]},"recipients":{"kind":"Objects","args":{"$sym":"distributive_pump_subject","args":[{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"AllOf","args":[{"kind":"CardType","args":"Creature"},{"kind":"Not","args":{"kind":"SameObject","args":{"$sym":"pump_other_ref","args":[{"$sym":OTHER,"args":[{"kind":"Source"}]}]}}}]}}}]}},"amount":{"kind":"Constant","args":2},"preventable":true}}}});
        let original = root.clone();
        prepare(&mut root).unwrap();
        assert_eq!(root, original);
    }

    #[test]
    fn unrelated_property_source_and_cost_annotations_are_removed_early() {
        for name in [TYPE, ACTOR, PROPERTY, OTHER] {
            let original = json!({"kind":"Source"});
            let mut root = json!({"kind":"ObjectProperty","args":{"object":{"$sym":name,"args":[original.clone()]},"property":"Power","information":"CurrentOrLastKnown"}});
            prepare(&mut root).unwrap();
            assert_eq!(root["args"]["object"], original);
        }
    }
    #[test]
    fn no_branch_escape_intervening_action_or_nonbattlefield_plural_move() {
        let (root, path, keys) = frame();
        let mut nonbattlefield = root.clone();
        nonbattlefield["instructions"][0]["args"]["operation"]["args"]["destination"] =
            super::super::movement::test_destination(json!("Exile"));
        prepare(&mut nonbattlefield).unwrap();
        assert_eq!(
            nonbattlefield["instructions"][1]["args"]["condition"]["args"]["object"]["$sym"],
            "it"
        );
        let mut plural = root.clone();
        plural["instructions"][0]["args"]["operation"]["args"]["objects"] =
            json!({"kind":"Selected","args":9});
        assert!(receipt(&plural, &path, &keys, 5).unwrap().is_none());
        let mut intervening = root.clone();
        intervening["instructions"].as_array_mut().unwrap().insert(
            1,
            json!({"kind":"Effect","args":{"result":12,"operation":{"kind":"Draw"}}}),
        );
        let mut later = path.clone();
        later[1] = Step::Index(2);
        assert!(receipt(&intervening, &later, &keys, 5).unwrap().is_none());
        let nested = json!({"instructions":[root["instructions"][0].clone(),{"kind":"If","args":{"then":{"instructions":[root["instructions"][1].clone()]}}}]});
        let nested_path = vec![
            Step::Key("instructions".into()),
            Step::Index(1),
            Step::Key("args".into()),
            Step::Key("then".into()),
            Step::Key("instructions".into()),
            Step::Index(0),
        ];
        assert!(receipt(&nested, &nested_path, &keys, 5).unwrap().is_none());
    }
    #[test]
    fn target_domain_and_count_are_proved_not_assumed_from_one_index() {
        let (mut root, path, keys) = frame();
        root["instructions"][0]["args"]["operation"]["args"]["objects"]["args"][0]["args"]["args"]
            ["slot"]["args"][0]["count"] = json!("AnyNumber");
        assert!(receipt(&root, &path, &keys, 5).unwrap().is_none());
    }
}
