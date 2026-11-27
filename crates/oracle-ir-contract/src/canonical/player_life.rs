//! The damage recipient or one announced player is the entire counted loop.
use super::walk::{Uses, lossless};
use crate::ir::{
    EventZone, Expr, Instruction, ObjectFilter, ObjectKind, ObjectSet, Operation, PlayerRef,
    PlayerSet, ZoneScope,
};
use serde_json::{Value, json};

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

/// A single event player or an announced permanent's controller can receive
/// one hand-to-battlefield offer. No intervening action changes that player.
fn optional_hand_entry(outer: &Value, uses: &Uses, tracing: bool) -> Option<Value> {
    if outer["kind"] != "ForEachPlayer" {
        return None;
    }
    let args = &outer["args"];
    let player = &args["players"]["args"];
    if args["players"]["kind"] != "One"
        || !(player["kind"] == "EventSubject"
            || player["kind"] == "ControllerOf" && player["args"]["kind"] == "Target")
        || !args["starting_with"].is_null()
        || args["in_chosen_direction"] != false
    {
        return None;
    }
    let binding = super::trace::number(&args["binding"])?;
    let [choice] = args["body"]["instructions"].as_array()?.as_slice() else {
        return None;
    };
    let bound = |value: &Value| {
        value["kind"] == "Bound" && super::trace::number(&value["args"]) == Some(binding)
    };
    if choice["kind"] != "Optional" || !bound(&choice["args"]["decider"]) {
        return None;
    }
    let [pick, put] = choice["args"]["body"]["instructions"]
        .as_array()?
        .as_slice()
    else {
        return None;
    };
    let selection = &pick["args"]["operation"];
    let move_card = &put["args"]["operation"];
    let hand = &selection["args"]["objects"];
    let zone = &hand["args"];
    let scope = &zone["scope"]["Players"];
    let arrival = &move_card["args"]["destination"]["Battlefield"];
    let filter = &zone["filter"];
    let selected = super::trace::number(&pick["args"]["result"])?;
    let returned = super::trace::number(&put["args"]["result"])?;
    if pick["kind"] != "Effect"
        || put["kind"] != "Effect"
        || selection["kind"] != "ChooseObjects"
        || !bound(&selection["args"]["player"])
        || selection["args"]["count"] != json!({"AsManyAsCan":{"kind":"Constant","args":1}})
        || selection["args"]["constraints"] != json!([])
        || hand["kind"] != "Zone"
        || zone["kind"] != "Card"
        || scope["zone"] != "Hand"
        || scope["players"]["kind"] != "One"
        || !bound(&scope["players"]["args"])
        || filter["kind"] != "AnyOf"
        || !filter["args"].as_array().is_some_and(|parts| {
            (1..=8).contains(&parts.len()) && parts.iter().all(|part| part["kind"] == "CardType")
        })
        || move_card["kind"] != "Move"
        || move_card["args"]["objects"]["kind"] != "Selected"
        || super::trace::number(&move_card["args"]["objects"]["args"]) != Some(selected)
        || move_card["args"]["qualifiers"] != json!([])
        || !bound(&arrival["controller"]["Player"])
        || !arrival["arrival"]["controller"].is_null()
        || arrival["arrival"]["counters"] != json!([])
        || arrival["arrival"]["states"] != json!([])
        || uses.definitions.get(&selected) != Some(&1)
        || uses.total.get(&selected) != Some(&2)
        || uses.definitions.get(&returned) != Some(&1)
        || uses.total.get(&returned) != Some(&1)
    {
        return None;
    }
    lossless::<Instruction>(outer, tracing)?;
    let mut direct = choice.clone();
    direct["args"]["decider"] = player.clone();
    let actions = &mut direct["args"]["body"]["instructions"];
    let select = &mut actions[0]["args"]["operation"]["args"];
    select["player"] = player.clone();
    select["objects"]["args"]["scope"]["Players"]["players"]["args"] = player.clone();
    actions[1]["args"]["operation"]["args"]["destination"]["Battlefield"]["controller"]["Player"] =
        player.clone();
    Some(direct)
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
        } else if let Some(offer) = optional_hand_entry(item, uses, tracing) {
            *item = offer;
            changed = true;
        }
    }
    changed
}
