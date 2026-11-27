//! Saved split forms of an ordinary spell and its closed delayed-draw paragraph.
use super::{
    trace,
    walk::{Uses, Walker, lossless},
};
use crate::{
    ir::{
        Ability, DelayedStart, EventSpec, Expr, Instruction, Operation, PlayerRef, PlayerSet,
        TurnPoint,
    },
    ir_schema::{Schema, Ty},
};
use serde_json::Value;
use std::collections::HashSet;

fn ordinary_spell(value: &Value) -> bool {
    let spell = if value["kind"] == "WithPresentation" {
        &value["args"]["ability"]
    } else {
        value
    };
    spell["kind"] == "Spell"
        && spell["args"]["announcement"]
            .get("modes")
            .is_some_and(Value::is_null)
        && spell["args"]["body"]["instructions"].is_array()
}

fn closed_draw(value: &Value, tracing: bool) -> bool {
    if value["kind"] != "Spell"
        || value["args"]["body"]["instructions"][0]["args"]["operation"]["kind"]
            != "CreateDelayedTrigger"
    {
        return false;
    }
    let Some(Ability::Spell { announcement, body }) = lossless(value, tracing) else {
        return false;
    };
    let [
        Instruction::Effect {
            operation:
                Operation::CreateDelayedTrigger {
                    event,
                    start: DelayedStart::NextTurn,
                    expires: None,
                    intervening_if: None,
                    body,
                },
            ..
        },
    ] = body.instructions.as_slice()
    else {
        return false;
    };
    announcement.targets.is_empty()
        && announcement.modes.is_none()
        && matches!(
            **event,
            EventSpec::Turn {
                point: TurnPoint::UpkeepBegins,
                players: PlayerSet::All
            }
        )
        && matches!(body.instructions.as_slice(), [Instruction::Effect {
            operation: Operation::Draw { players: PlayerSet::One(PlayerRef::AbilityController), amount: Expr::Constant(count) }, ..
        }] if *count >= 0)
}

struct ResultIds {
    used: HashSet<u64>,
    candidates: std::ops::RangeInclusive<u64>,
}
impl ResultIds {
    fn of(ability: &mut Value, schema: &Schema) -> Option<Self> {
        let mut uses = Uses::default();
        Walker::new(schema)
            .walk(
                ability,
                &Ty::Named("Ability".into()),
                false,
                "",
                0,
                &mut uses,
            )
            .ok()?;
        Some(Self {
            used: uses.total.into_keys().collect(),
            candidates: 0..=u64::from(u32::MAX),
        })
    }

    fn reserve(&mut self) -> Option<u64> {
        self.candidates.find(|id| self.used.insert(*id))
    }
}

fn append_draw(
    prefix: &mut Value,
    tail: &mut Value,
    ids: &mut Option<ResultIds>,
    schema: &Schema,
) -> Option<()> {
    if ids.is_none() {
        *ids = ResultIds::of(prefix, schema);
    }
    let ids = ids.as_mut()?;
    let outer = ids.reserve()?;
    let inner = ids.reserve()?;
    let mut delay = tail["args"]["body"]["instructions"][0].take();
    trace::set_number(&mut delay["args"]["result"], outer);
    let draw = &mut delay["args"]["operation"]["args"]["body"]["instructions"][0];
    trace::set_number(&mut draw["args"]["result"], inner);
    let spell = if prefix["kind"] == "WithPresentation" {
        &mut prefix["args"]["ability"]
    } else {
        prefix
    };
    spell["args"]["body"]["instructions"]
        .as_array_mut()
        .expect("ordinary spell instructions")
        .push(delay);
    Some(())
}

/// This tail owns no targets or reads of earlier results. Reserve fresh
/// definitions before combining the frames, preserving their origin annotations.
/// A modal prefix retains its independent resolution program.
pub(super) fn join_closed_draws(value: &mut Value, schema: &Schema, tracing: bool) -> bool {
    let Some(items) = value.as_array_mut() else {
        return false;
    };
    let Some(first) = items
        .windows(2)
        .position(|pair| ordinary_spell(&pair[0]) && closed_draw(&pair[1], tracing))
    else {
        return false;
    };
    let mut ids = None;
    let mut joined = false;
    for mut item in items.split_off(first + 1) {
        let previous = items.last_mut().expect("split retains the preceding spell");
        if ordinary_spell(previous)
            && closed_draw(&item, tracing)
            && append_draw(previous, &mut item, &mut ids, schema).is_some()
        {
            joined = true;
        } else {
            items.push(item);
            ids = None;
        }
    }
    joined
}
