//! A value written after a plural player recipient can name that current player.
use super::{Step, symbol_name};
use crate::model::ir::PlayerRef;
use serde_json::{Value, json};

pub(super) fn reference(
    root: &Value,
    path: &[Step],
    introduced: &[super::references::TargetIntroduction],
    scope: &super::Scope,
    prior_targets: &super::references::Contexts,
) -> Result<Value, String> {
    player(root, path, introduced, scope.target_base)?
        .map(|player| json!(player))
        .or_else(|| prior_targets.before(path, scope).player())
        .ok_or_else(|| "no player for this reference".into())
}

pub(super) fn player(
    mut value: &Value,
    path: &[Step],
    introduced: &[super::references::TargetIntroduction],
    target_base: usize,
) -> Result<Option<PlayerRef>, String> {
    if path.len() > 128 {
        return Err("recipient scalar depth exceeds128".into());
    }
    let mut actual = None;
    let mut action_player = None;
    for step in path {
        if value["kind"] == "DealDamage" {
            actual = Some(value["args"]["recipients"].clone());
            action_player = None;
        }
        if let Some(players) = value.get("args").and_then(|args| args.get("players")) {
            actual = None;
            action_player = (players["kind"] == "One").then_some(&players["args"]);
        }
        if symbol_name(value) == Some("recipient_value") {
            // An explicitly annotated single-player amount owns its `their`.
            // A target introduction can still contain a pending slot marker.
            // Read its assigned address, including inside a controller phrase,
            // rather than falling back to an unrelated earlier player.
            if value["args"][0]["kind"] == "Players"
                && let Some(player) = action_player
            {
                let plain = super::references::bound_reference(player, introduced, target_base)?;
                return serde_json::from_value(plain)
                    .map(Some)
                    .map_err(|why| format!("annotated player amount: {why}"));
            }
            if let Some(recipients) = &actual
                && recipients["kind"] == "Players"
                && recipients["args"]["kind"] == "One"
                && recipients["args"]["args"]["kind"] == "Bound"
            {
                return serde_json::from_value(recipients["args"]["args"].clone())
                    .map(Some)
                    .map_err(|why| format!("bound damage recipient: {why}"));
            }
            let recipients = &value["args"][0];
            if recipients["kind"] == "Players"
                && matches!(
                    recipients["args"]["kind"].as_str(),
                    Some("All" | "OpponentsOf")
                )
            {
                return Ok(Some(PlayerRef::Acting));
            }
        }
        value = match step {
            Step::Key(key) => value.get(key),
            Step::Index(index) => value.get(*index),
        }
        .ok_or("recipient scalar path")?;
    }
    Ok(None)
}

struct Work(usize);

impl Work {
    fn new() -> Self {
        Self(131_072)
    }

    fn enter(&mut self, depth: usize) -> Result<(), String> {
        if depth > 128 || self.0 == 0 {
            return Err("recipient binding work/depth limit".into());
        }
        self.0 -= 1;
        Ok(())
    }
}

pub(super) fn binding_floor(value: &Value) -> Result<u64, String> {
    fn scan(
        value: &Value,
        maximum: &mut Option<u64>,
        work: &mut Work,
        depth: usize,
    ) -> Result<(), String> {
        work.enter(depth)?;
        let bound = (value["kind"] == "Bound")
            .then(|| value["args"].as_u64())
            .flatten();
        if let Some(id) = value.get("binding").and_then(Value::as_u64).or(bound) {
            *maximum = Some(maximum.unwrap_or(0).max(id));
        }
        match value {
            Value::Object(fields) => {
                for child in fields.values() {
                    scan(child, maximum, work, depth + 1)?;
                }
            }
            Value::Array(items) => {
                for child in items {
                    scan(child, maximum, work, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut maximum = None;
    scan(value, &mut maximum, &mut Work::new(), 0)?;
    maximum.map_or(Ok(0), |id| {
        id.checked_add(1).ok_or("player binding overflow".into())
    })
}

fn lexical(value: &Value) -> bool {
    matches!(
        value["kind"].as_str(),
        Some("Spell" | "Activated" | "Triggered" | "Static" | "ReflexiveTrigger" | "ForEachPlayer")
    )
}

fn replace(
    value: &mut Value,
    from: &Value,
    to: &Value,
    work: &mut Work,
    depth: usize,
) -> Result<bool, String> {
    work.enter(depth)?;
    if value == from {
        *value = to.clone();
        return Ok(true);
    }
    if lexical(value) || (from["kind"] == "Bound" && value.get("binding") == Some(&from["args"])) {
        return Ok(false);
    }
    let mut changed = false;
    match value {
        Value::Object(fields) => {
            for child in fields.values_mut() {
                changed |= replace(child, from, to, work, depth + 1)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                changed |= replace(child, from, to, work, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(changed)
}

/// Only an annotated amount can introduce a loop. Other native instructions
/// and lexical actors keep their existing owners and result receipts.
pub(super) fn lower(root: &mut Value, floor: u64) -> Result<(), String> {
    let mut next = floor.max(binding_floor(root)?);
    visit(root, &mut next, false, &mut Work::new(), 0)
}

pub(super) fn surface(root: &mut Value) -> Result<(), String> {
    visit(root, &mut 0, true, &mut Work::new(), 0)
}

fn visit(
    value: &mut Value,
    next: &mut u64,
    inverse: bool,
    work: &mut Work,
    depth: usize,
) -> Result<(), String> {
    work.enter(depth)?;
    match value {
        Value::Object(fields) => {
            for child in fields.values_mut() {
                visit(child, next, inverse, work, depth + 1)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                visit(child, next, inverse, work, depth + 1)?;
            }
        }
        _ => {}
    }
    let replacement = if inverse {
        unloop(value, work)?
    } else {
        loop_value(value, next, work)?
    };
    if let Some(replacement) = replacement {
        *value = replacement;
    }
    Ok(())
}

fn loop_value(value: &Value, next: &mut u64, work: &mut Work) -> Result<Option<Value>, String> {
    if value["kind"] != "Effect" || value["args"]["operation"]["kind"] != "DealDamage" {
        return Ok(None);
    }
    let original = &value["args"]["operation"];
    let recipients = &original["args"]["recipients"];
    let amount = &original["args"]["amount"];
    if symbol_name(amount) != Some("recipient_value")
        || recipients["kind"] != "Players"
        || !matches!(
            recipients["args"]["kind"].as_str(),
            Some("All" | "OpponentsOf")
        )
    {
        return Ok(None);
    }
    let binding: crate::model::ir::LoopBinding = serde_json::from_value(json!(*next))
        .map_err(|why| format!("generated recipient binding: {why}"))?;
    let reference = json!({"kind":"Bound", "args":binding});
    let mut expression = amount["args"][1].clone();
    if !replace(
        &mut expression,
        &json!({"kind":"Acting"}),
        &reference,
        work,
        0,
    )? {
        return Ok(None);
    }
    *next = next.checked_add(1).ok_or("recipient binding overflow")?;
    let mut operation = original.clone();
    operation["args"]["amount"] = expression;
    operation["args"]["recipients"] =
        json!({"kind":"Players", "args":{"kind":"One", "args":reference}});
    Ok(Some(json!({"kind":"ForEachPlayer", "args":{
        "players":recipients["args"], "binding":binding, "starting_with":null,
        "in_chosen_direction":false, "body":{"instructions":[{
            "kind":"Effect", "args":{"result":value["args"]["result"], "operation":operation}
        }]}
    }})))
}

/// The inverse recognizes the same single-action loop and leaves all other
/// loops intact. It never drops a choice, ordering constraint or captured source.
fn unloop(value: &Value, work: &mut Work) -> Result<Option<Value>, String> {
    if value["kind"] != "ForEachPlayer"
        || !value["args"]["starting_with"].is_null()
        || value["args"]["in_chosen_direction"] == true
    {
        return Ok(None);
    }
    let args = &value["args"];
    let Some(instructions) = args["body"]["instructions"].as_array() else {
        return Ok(None);
    };
    let [effect] = instructions.as_slice() else {
        return Ok(None);
    };
    let operation = &effect["args"]["operation"];
    let reference = json!({"kind":"Bound", "args":args["binding"]});
    if effect["kind"] != "Effect"
        || operation["kind"] != "DealDamage"
        || operation["args"]["recipients"]
            != json!({"kind":"Players", "args":{"kind":"One", "args":reference}})
    {
        return Ok(None);
    }
    let acting = json!({"kind":"Acting"});
    let mut source = operation["args"]["source"].clone();
    if replace(&mut source, &reference, &acting, work, 0)? {
        return Ok(None);
    }
    let mut amount = operation["args"]["amount"].clone();
    if !replace(&mut amount, &reference, &acting, work, 0)? {
        return Ok(None);
    }
    let mut copy = effect.clone();
    copy["args"]["operation"]["args"]["recipients"] =
        json!({"kind":"Players", "args":args["players"]});
    copy["args"]["operation"]["args"]["amount"] = amount;
    Ok(Some(copy))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn explicit_single_player_amount_uses_the_written_controller() {
        let controller = json!({"kind":"ControllerOf","args":{
            "kind":"Target","args":{"slot":3,"index":0}
        }});
        let recipients = json!({"kind":"Players","args":{
            "kind":"One","args":controller
        }});
        let root = json!({"kind":"Mill","args":{
            "players":{"kind":"One","args":controller},
            "amount":{"$sym":"recipient_value","args":[recipients,
                {"$sym":"that_player","args":[],"at":9}],"at":2}
        }});
        let path = super::super::path_of(&["args", "amount"]);
        let mut reference_path = path.clone();
        reference_path.extend([
            super::super::Step::Key("args".into()),
            super::super::Step::Index(1),
        ]);
        assert_eq!(
            serde_json::to_value(
                super::player(&root, &reference_path, &[], 0)
                    .unwrap()
                    .unwrap()
            )
            .unwrap(),
            controller
        );
        let mut ordinary = root;
        ordinary["args"]["amount"] = json!({"$sym":"that_player","args":[],"at":9});
        assert!(super::player(&ordinary, &path, &[], 0).unwrap().is_none());
    }

    #[test]
    fn pending_targets_use_their_assigned_slot_and_unknown_introductions_refuse() {
        let player = json!({"kind":"Target","args":{
            "slot":{"$sym":"target","args":[{}],"at":2},"index":0
        }});
        let root = json!({"kind":"Discard","args":{
            "players":{"kind":"One","args":player},
            "amount":{"$sym":"recipient_value","args":[
                {"kind":"Players","args":{"kind":"One","args":player}},
                {"$sym":"that_player","args":[],"at":9}],"at":3}
        }});
        let mut path = super::super::path_of(&["args", "amount", "args"]);
        path.push(super::super::Step::Index(1));
        assert!(super::player(&root, &path, &[], 0).is_err());
        let introduced = vec![((2, json!({}).to_string()), 1)];
        let actual = super::player(&root, &path, &introduced, 3)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            json!({"kind":"Target","args":{"slot":4,"index":0}})
        );
        let mut controller = root.clone();
        let target = controller["args"]["players"]["args"].take();
        controller["args"]["players"]["args"] = json!({"kind":"ControllerOf","args":target});
        let actual = super::player(&controller, &path, &introduced, 3)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            json!({"kind":"ControllerOf","args":{
                "kind":"Target","args":{"slot":4,"index":0}
            }})
        );
        assert_eq!(
            controller["args"]["players"]["args"]["args"]["args"]["slot"]["$sym"],
            "target"
        );
    }

    #[test]
    fn recipient_annotations_do_not_duplicate_real_target_introductions() {
        let target = json!({"$sym":"target", "args":[{}], "at":7});
        let recipient = json!({"kind":"Players", "args":{"kind":"One", "args":{
            "kind":"Target", "args":{"slot":target, "index":0}
        }}});
        let root = json!({"recipients":recipient,"amount":{
            "$sym":"recipient_value", "args":[recipient,
                {"$sym":"that_player", "args":[], "at":9}], "at":2
        }});
        let mut found = Vec::new();
        super::super::references::collect(
            &root,
            &mut Vec::new(),
            &mut found,
            super::super::filters::Context::Outside,
            &mut false,
        );
        assert_eq!(found.iter().filter(|item| item.at == 7).count(), 1);
        assert_eq!(found.iter().filter(|item| item.at == 9).count(), 1);
    }
}
