//! One unobserved, simultaneous literal mana action can address All directly.
use super::walk::{Uses, lossless};
use crate::ir::{Expr, Instruction, ManaColor, ManaProduction, Operation, PlayerRef, PlayerSet};
use serde_json::Value;

fn fixed(mana: &ManaProduction, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    match mana {
        ManaProduction::Amount { color, amount } => {
            matches!(amount, Expr::Constant(n) if *n >= 0)
                && matches!(
                    color,
                    ManaColor::White
                        | ManaColor::Blue
                        | ManaColor::Black
                        | ManaColor::Red
                        | ManaColor::Green
                        | ManaColor::Colorless
                )
        }
        ManaProduction::All(parts) => {
            !parts.is_empty() && parts.iter().all(|m| fixed(m, depth + 1))
        }
        _ => false,
    }
}

fn candidate(value: &Value, path: &str, uses: &Uses, tracing: bool) -> bool {
    if value["kind"] != "ForEachPlayer" {
        return false;
    }
    let Some(Instruction::ForEachPlayer {
        players: PlayerSet::All,
        binding,
        starting_with: None,
        in_chosen_direction: false,
        body,
    }) = lossless(value, tracing)
    else {
        return false;
    };
    let [
        Instruction::Effect {
            result,
            operation:
                Operation::AddMana {
                    players: PlayerSet::One(PlayerRef::Bound(player)),
                    mana,
                    restriction: None,
                },
        },
    ] = body.instructions.as_slice()
    else {
        return false;
    };
    if player.0 != binding.0
        || !fixed(mana, 0)
        || uses.definitions.get(&u64::from(result.0)) != Some(&1)
        || uses.total.get(&u64::from(result.0)) != Some(&1)
    {
        return false;
    }
    // Removing an invalid repeated player binding must not make it valid.
    // Sibling scopes may reuse labels; only an enclosing definition matters.
    !uses.bindings.iter().any(|(site, id)| {
        *id == u64::from(binding.0)
            && site.strip_suffix("/binding").is_some_and(|parent| {
                path.strip_prefix(parent)
                    .is_some_and(|tail| tail.starts_with('/'))
            })
    })
}

pub(super) fn flatten(items: &mut [Value], path: &str, uses: &Uses, tracing: bool) -> bool {
    let mut changed = false;
    for (index, item) in items.iter_mut().enumerate() {
        if item["kind"] == "ForEachPlayer"
            && candidate(item, &format!("{path}/{index}"), uses, tracing)
        {
            let mut effect = item["args"]["body"]["instructions"][0].clone();
            effect["args"]["operation"]["args"]["players"] = item["args"]["players"].clone();
            // Moving the original child retains its result's source origin.
            *item = effect;
            changed = true;
        }
    }
    changed
}

/// Positive literal units in one production share a color-total representation.
/// Choices, dynamic values, zero amounts and checked-arithmetic overflow refuse.
pub fn literal(production: &ManaProduction) -> Option<ManaProduction> {
    fn gather(
        mana: &ManaProduction,
        totals: &mut [i64; 6],
        depth: usize,
        work: &mut usize,
    ) -> Option<()> {
        if depth > 32 || *work == 0 {
            return None;
        }
        *work -= 1;
        match mana {
            ManaProduction::Amount {
                color,
                amount: Expr::Constant(n),
            } if *n > 0 => {
                let index = match color {
                    ManaColor::White => 0,
                    ManaColor::Blue => 1,
                    ManaColor::Black => 2,
                    ManaColor::Red => 3,
                    ManaColor::Green => 4,
                    ManaColor::Colorless => 5,
                    ManaColor::Chosen => return None,
                };
                totals[index] = totals[index].checked_add(*n)?;
            }
            ManaProduction::All(parts) if !parts.is_empty() => {
                for part in parts {
                    gather(part, totals, depth + 1, work)?;
                }
            }
            _ => return None,
        }
        Some(())
    }
    let mut totals = [0; 6];
    gather(production, &mut totals, 0, &mut 32_768)?;
    let mut parts: Vec<_> = [
        ManaColor::White,
        ManaColor::Blue,
        ManaColor::Black,
        ManaColor::Red,
        ManaColor::Green,
        ManaColor::Colorless,
    ]
    .into_iter()
    .zip(totals)
    .filter(|(_, n)| *n > 0)
    .map(|(color, n)| ManaProduction::Amount {
        color,
        amount: Expr::Constant(n),
    })
    .collect();
    if parts.len() == 1 {
        parts.pop()
    } else {
        Some(ManaProduction::All(parts))
    }
}

pub(super) fn bundle(value: &mut Value, tracing: bool) -> bool {
    if value.get("All").is_none() {
        return false;
    }
    let Some(typed) = lossless::<ManaProduction>(value, tracing) else {
        return false;
    };
    let Some(normalized) = literal(&typed) else {
        return false;
    };
    let normalized = serde_json::to_value(normalized).expect("literal mana serialization");
    if *value == normalized {
        return false;
    }
    *value = normalized;
    true
}
