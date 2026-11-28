//! Printed stars need a corresponding definition in this face's rules.
use crate::model::ir::{
    Ability, AsEvent, BattlefieldState, Change, Characteristics, EffectSubjects, EventChange, Expr,
    Instruction, ObjectFilter, ObjectRef, ObjectSet, Operand, Operation, Program,
    ReplacementActive, ReplacementEvent,
};
use serde::{Deserialize, Serialize};

/// A printed component and its fixed offset when its star is undetermined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum PrintedStat {
    Fixed(i32),
    Star { offset: i32, subtract: bool },
}

impl PrintedStat {
    fn offset(self) -> i32 {
        match self {
            Self::Fixed(value) | Self::Star { offset: value, .. } => value,
        }
    }

    fn defined_by(self, expression: &Expr) -> bool {
        let Self::Star { offset, subtract } = self else {
            return false;
        };
        let same_offset =
            |value: &Expr| matches!(value, Expr::Constant(n) if *n == i64::from(offset));
        let dynamic = |value: &Expr| !matches!(value, Expr::Constant(_));
        match expression {
            Expr::Subtract(left, right) if subtract => same_offset(left) && dynamic(right),
            Expr::Add(left, right) if !subtract && offset != 0 => {
                (same_offset(left) && dynamic(right)) || (same_offset(right) && dynamic(left))
            }
            _ => !subtract && offset == 0 && dynamic(expression),
        }
    }
}

/// Availability of original printed fields is distinct from an absent box.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum PrintedPowerToughness {
    Unavailable,
    Absent,
    /// The Oracle record prints ?/? without assigning numeric base values.
    Indeterminate,
    Known {
        power: i32,
        toughness: i32,
    },
    Variable {
        power: PrintedStat,
        toughness: PrintedStat,
    },
    Unsupported,
}

impl PrintedPowerToughness {
    pub(super) fn fixed_toughness(&self) -> Option<i32> {
        match self {
            Self::Known { toughness, .. }
            | Self::Variable {
                toughness: PrintedStat::Fixed(toughness),
                ..
            } => Some(*toughness),
            _ => None,
        }
    }

    pub fn values(&self) -> Option<(i32, i32)> {
        match *self {
            Self::Known { power, toughness } => Some((power, toughness)),
            _ => None,
        }
    }

    pub(crate) fn base(&self) -> Option<(i32, i32)> {
        match *self {
            Self::Variable { power, toughness } => Some((power.offset(), toughness.offset())),
            _ => self.values(),
        }
    }

    pub(super) fn check_definition(
        &self,
        characteristics: &Characteristics,
        abilities: &[Ability],
    ) -> Result<(), String> {
        let Self::Variable { power, toughness } = *self else {
            return Ok(());
        };
        if characteristics.power_toughness != self.base()
            || [power, toughness]
                .iter()
                .all(|stat| matches!(stat, PrintedStat::Fixed(_)))
        {
            return Err("variable printed P/T disagrees with the base characteristics".into());
        }
        let stats = [power, toughness];
        let mut found = stats.map(|s| matches!(s, PrintedStat::Fixed(_)));
        source_definitions(abilities, stats, &mut found, 0, &mut 8192);
        if found == [true; 2] {
            Ok(())
        } else {
            Err("printed variable P/T has no matching source definition".into())
        }
    }
}

fn within_budget(depth: usize, remaining: &mut usize) -> bool {
    if depth >= 64 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    true
}

fn covers_changes(
    changes: &[Change],
    accepts: &impl Fn(usize, &Expr) -> bool,
    found: &mut [bool; 2],
    depth: usize,
    remaining: &mut usize,
) {
    for change in changes {
        if *found == [true; 2] || !within_budget(depth, remaining) {
            break;
        }
        match change {
            Change::SetPower(expr) => found[0] |= accepts(0, expr),
            Change::SetToughness(expr) => found[1] |= accepts(1, expr),
            Change::SetBasePowerToughness { power, toughness } => {
                found[0] |= accepts(0, power);
                found[1] |= accepts(1, toughness);
            }
            Change::Copiable(changes) => {
                covers_changes(changes, accepts, found, depth + 1, remaining)
            }
            _ => {}
        }
    }
}

fn source_definitions(
    abilities: &[Ability],
    stats: [PrintedStat; 2],
    found: &mut [bool; 2],
    depth: usize,
    remaining: &mut usize,
) {
    let two_stars = stats
        == [PrintedStat::Star {
            offset: 0,
            subtract: false,
        }; 2];
    for ability in abilities {
        if *found == [true; 2] || !within_budget(depth, remaining) {
            break;
        }
        match ability {
            Ability::WithPresentation { ability, .. } => {
                source_definitions(
                    std::slice::from_ref(ability.as_ref()),
                    stats,
                    found,
                    depth + 1,
                    remaining,
                );
            }
            Ability::Conditional {
                then, otherwise, ..
            } => {
                source_definitions(then, stats, found, depth + 1, remaining);
                source_definitions(otherwise, stats, found, depth + 1, remaining);
            }
            Ability::Static {
                affected: EffectSubjects::Objects(ObjectSet::One(ObjectRef::Source)),
                changes,
                ..
            } => covers_changes(
                changes,
                &|index, expr| stats[index].defined_by(expr),
                found,
                depth,
                remaining,
            ),
            Ability::Replacement {
                active: ReplacementActive::SourceEntering,
                event:
                    ReplacementEvent::Enter {
                        objects: ObjectFilter::SameObject(ObjectRef::Source),
                    },
                body,
            } if two_stars && entry_defines_both(body, depth + 1, remaining) => *found = [true; 2],
            Ability::AsEvent {
                object: ObjectRef::Source,
                event: AsEvent::EntersOrTurnsFaceUp,
                actions,
            } if two_stars && face_up_defines_both(actions) => *found = [true; 2],
            _ => {}
        }
    }
}

fn entry_defines_both(body: &Program, depth: usize, remaining: &mut usize) -> bool {
    for step in &body.instructions {
        if !within_budget(depth, remaining) {
            return false;
        }
        let covered = |changes: &[Change], remaining: &mut usize| {
            let mut found = [false; 2];
            covers_changes(changes, &|_, _| true, &mut found, depth + 1, remaining);
            found == [true; 2]
        };
        let defines = match step {
            Instruction::Group(body) => entry_defines_both(body, depth + 1, remaining),
            Instruction::If {
                then, otherwise, ..
            } => {
                entry_defines_both(then, depth + 1, remaining)
                    && entry_defines_both(otherwise, depth + 1, remaining)
            }
            Instruction::Effect {
                operation:
                    Operation::ModifyEvent {
                        change: EventChange::Entry(state),
                    },
                ..
            } => match state {
                BattlefieldState::Changes(changes) => covered(changes, remaining),
                BattlefieldState::ChooseChanges(options) => {
                    (2..=64).contains(&options.len())
                        && options.iter().all(|option| covered(option, remaining))
                }
                _ => false,
            },
            _ => false,
        };
        if defines {
            return true;
        }
    }
    false
}

fn face_up_defines_both(actions: &Operand) -> bool {
    let Operand::List(actions) = actions else {
        return false;
    };
    let [
        Operand::Term {
            family,
            form,
            operands,
        },
    ] = actions.as_slice()
    else {
        return false;
    };
    family == "EnterOrFaceUpAction" && form == "EntersWithPTOfChoice"
        && (2..=64).contains(&operands.len())
        && operands.iter().all(|option| matches!(option, Operand::Term { family, form, operands }
            if family == "PT" && form == "PT" && matches!(operands.as_slice(), [Operand::Integer(_), Operand::Integer(_)])))
}
