//! A grouped output choice belongs to one pending mana-production event.
use super::*;

pub(super) fn applies_to(event: &ReplacementEvent, body: &Program) -> bool {
    matches!(
        body.instructions.as_slice(),
        [Instruction::Effect {
            operation: Operation::ModifyEvent {
                change: EventChange::ReplaceManaChoices(_),
            },
            ..
        }]
    ) && applies_at(event, 0, &mut 1024)
}

fn applies_at(event: &ReplacementEvent, depth: usize, remaining: &mut usize) -> bool {
    if depth > 64 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    match event {
        ReplacementEvent::ProduceMana { .. } => true,
        ReplacementEvent::FirstEachTurn(event)
        | ReplacementEvent::Measured { event, .. }
        | ReplacementEvent::Caused { event, .. } => applies_at(event, depth + 1, remaining),
        ReplacementEvent::Either(events) => {
            !events.is_empty()
                && events
                    .iter()
                    .all(|event| applies_at(event, depth + 1, remaining))
        }
        _ => false,
    }
}

impl Scope<'_> {
    pub(super) fn grouped_mana(&self, options: &[ManaReplacementOption]) -> Result {
        if !self.mana_replacement {
            return Err(
                "grouped mana outputs require one closed mana-production replacement body".into(),
            );
        }
        if options.is_empty() || options.len() > 64 {
            return Err("a grouped mana replacement requires one to 64 options".into());
        }
        for option in options {
            if option.color == ManaColor::Chosen {
                return Err("a grouped mana output must name an explicit mana type".into());
            }
            self.filter(&option.sources)?;
        }
        Ok(())
    }
}
