//! One plain exchange chooses the same distinct pair in either target spelling.
use super::trace;
use super::walk::{Visit, Walker, lossless};
use crate::ir::{
    Ability, Count, Expr, Instruction, KeywordAction, ObjectDomain, ObjectRef, Operation,
    PlayerRef, PlayerSet, TargetKind,
};
use crate::ir_schema::Ty;
use serde_json::{Value, json};

#[derive(Default)]
struct TargetUse(bool);
impl Visit for TargetUse {
    fn leave(
        &mut self,
        _: &mut Value,
        ty: &Ty,
        _: bool,
        _: &str,
        _: &mut Walker<'_>,
    ) -> Result<(), String> {
        self.0 |= matches!(ty, Ty::Named(name) if name == "TargetSlot");
        Ok(())
    }
}

fn remainder(filter: &Value, slot: u64, tracing: bool) -> Option<Value> {
    let distinct = json!({"kind":"Not","args":{"kind":"SameObject","args":{
        "kind":"Target","args":{"slot":slot,"index":0}
    }}});
    let mut parts = if filter["kind"] == "AllOf" {
        filter["args"].as_array()?.clone()
    } else {
        vec![filter.clone()]
    };
    let index = parts
        .iter()
        .position(|part| trace::same(part, &distinct, tracing))?;
    parts.remove(index);
    Some(match parts.len() {
        0 => json!({"kind":"Any"}),
        1 => parts.remove(0),
        _ => json!({"kind":"AllOf","args":parts}),
    })
}

fn candidate(value: &Value, tracing: bool) -> Option<(u64, Value)> {
    if value["kind"] != "Spell"
        || value["args"]["announcement"]["targets"].as_array()?.len() != 2
        || value["args"]["body"]["instructions"].as_array()?.len() != 1
        || value
            .pointer("/args/body/instructions/0/args/operation/args/action/ExchangeControl")
            .is_none()
    {
        return None;
    }
    let typed: Ability = lossless(value, tracing)?;
    crate::validate::validate_ability(&typed).ok()?;
    let Ability::Spell { announcement, body } = typed else {
        return None;
    };
    if announcement.modes.is_some() {
        return None;
    }
    let [first, second] = announcement.targets.as_slice() else {
        return None;
    };
    for spec in [first, second] {
        if spec.kind != TargetKind::Object(ObjectDomain::permanent())
            || !matches!(spec.count, Count::Exactly(Expr::Constant(1)))
            || spec.players.is_some()
            || spec.distribute.is_some()
            || !spec.constraints.is_empty()
            || spec.chooser.is_some()
        {
            return None;
        }
    }
    if first.slot.0 == second.slot.0 {
        return None;
    }
    let [
        Instruction::Effect {
            operation:
                Operation::Keyword {
                    actor: PlayerSet::One(PlayerRef::AbilityController),
                    action:
                        KeywordAction::ExchangeControl {
                            first: ObjectRef::Target { slot: a, index: 0 },
                            second: ObjectRef::Target { slot: b, index: 0 },
                        },
                },
            ..
        },
    ] = body.instructions.as_slice()
    else {
        return None;
    };
    if a.0 != first.slot.0 || b.0 != second.slot.0 {
        return None;
    }
    let specs = &value["args"]["announcement"]["targets"];
    let rest = remainder(&specs[1]["filter"], u64::from(first.slot.0), tracing)?;
    trace::same(&specs[0]["filter"], &rest, tracing).then_some((u64::from(first.slot.0), rest))
}

pub(super) fn normalize(
    value: &mut Value,
    tracing: bool,
    walker: &mut Walker<'_>,
) -> Result<bool, String> {
    let Some((slot, rest)) = candidate(value, tracing) else {
        return Ok(false);
    };
    // No shared filter may inspect either slot, its target population or a
    // distributed share. The schema sees all TargetSlot uses, not just nouns.
    let mut used = TargetUse::default();
    let mut filter = value["args"]["announcement"]["targets"][0]["filter"].clone();
    walker.walk(
        &mut filter,
        &Ty::Named("ObjectFilter".into()),
        false,
        "",
        0,
        &mut used,
    )?;
    if used.0 {
        return Ok(false);
    }
    let specs = value["args"]["announcement"]["targets"]
        .as_array_mut()
        .unwrap();
    let second = specs.remove(1);
    let first = &mut specs[0];
    let merged = trace::with_number(&second["slot"], slot);
    trace::merge(&mut first["slot"], &merged);
    trace::merge(&mut first["filter"], &rest);
    first["count"] = json!({"Exactly":{"kind":"Constant","args":2}});
    let reference = &mut value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["action"]
        ["ExchangeControl"]["second"]["args"];
    trace::set_number(&mut reference["slot"], slot);
    reference["index"] = json!(1);
    Ok(true)
}

#[cfg(test)]
mod tests;
