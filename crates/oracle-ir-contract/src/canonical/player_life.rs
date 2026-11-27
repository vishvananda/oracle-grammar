//! The damage recipient or one announced player is the entire counted loop.
use super::walk::{Uses, lossless};
use crate::ir::{
    EventZone, Expr, Instruction, ObjectFilter, ObjectKind, ObjectSet, Operation, PlayerRef,
    PlayerSet, ZoneScope,
};
use serde_json::Value;

fn one_player(value: &Value, path: &str, uses: &Uses, tracing: bool) -> Option<(Value, usize)> {
    let Instruction::ForEachPlayer {
        players: PlayerSet::One(PlayerRef::Target { .. } | PlayerRef::EventSubject),
        binding,
        starting_with: None,
        in_chosen_direction: false,
        body,
    } = lossless(value, tracing)?
    else {
        return None;
    };
    let [
        Instruction::Effect {
            result,
            operation:
                Operation::LoseLife {
                    players: PlayerSet::One(PlayerRef::Bound(subject)),
                    amount: Expr::CountObjects(population),
                },
        },
    ] = body.instructions.as_slice()
    else {
        return None;
    };
    let ObjectSet::Zone {
        scope: ZoneScope::Shared(EventZone::Battlefield),
        kind: ObjectKind::Permanent,
        filter: ObjectFilter::AllOf(parts),
    } = population.as_ref()
    else {
        return None;
    };
    let result = u64::from(result.0);
    if subject.0 != binding.0
        || !(2..=16).contains(&parts.len())
        || uses.definitions.get(&result) != Some(&1)
        || uses.total.get(&result) != Some(&1)
        || parts.iter().any(|part| {
            !matches!(
                part,
                ObjectFilter::CardType(_)
                    | ObjectFilter::Subtype(_)
                    | ObjectFilter::Tapped
                    | ObjectFilter::ControlledBy(PlayerSet::One(PlayerRef::Bound(_)))
            )
        })
        || uses.bindings.iter().any(|(at, label)| {
            *label == u64::from(binding.0)
                && at.strip_suffix("/binding").is_some_and(|owner| {
                    path.strip_prefix(owner)
                        .is_some_and(|tail| tail.starts_with('/'))
                })
        })
    {
        return None;
    }
    let mut controllers = parts.iter().enumerate().filter_map(|(at, part)| {
        if let ObjectFilter::ControlledBy(PlayerSet::One(PlayerRef::Bound(other))) = part {
            Some((at, other.0))
        } else {
            None
        }
    });
    let (index, controller) = controllers.next()?;
    if controller != binding.0 || controllers.next().is_some() {
        return None;
    }
    Some((value["args"]["players"].clone(), index))
}

pub(super) fn flatten(items: &mut [Value], path: &str, uses: &Uses, tracing: bool) -> bool {
    let mut changed = false;
    for (index, item) in items.iter_mut().enumerate() {
        if item["kind"] != "ForEachPlayer" {
            continue;
        }
        if let Some((player, at)) = one_player(item, &format!("{path}/{index}"), uses, tracing) {
            let mut effect = item["args"]["body"]["instructions"][0].take();
            let op = &mut effect["args"]["operation"]["args"];
            op["amount"]["args"]["args"]["filter"]["args"][at]["args"] = player.clone();
            op["players"] = player;
            *item = effect;
            changed = true;
        }
    }
    changed
}
