//! Scoped validation of portable operands.
use super::*;
impl<'a> Scope<'a> {
    #[inline]
    pub(super) fn operation_dispatch_6(&self, operation: &Operation) -> Result {
        match operation {
            Operation::Conjure {
                actor: _,
                card: _,
                count: _,
                owner: _,
                destination: _,
            }
            | Operation::ChangeSpell {
                subject: _,
                changes: _,
            }
            | Operation::PlayCards { .. }
            | Operation::PlayWith { .. }
            | Operation::DraftChangeZone { .. }
            | Operation::CopyCards {
                actor: _,
                objects: _,
                times: _,
            }
            | Operation::ChooseAtRandom { .. }
            | Operation::ChooseAction { .. }
            | Operation::Keyword {
                actor: _,
                action: _,
            } => self.operation_case_6(operation),
            _ => self.operation_dispatch_7(operation),
        }
    }

    #[inline]
    pub(super) fn operation_dispatch_7(&self, operation: &Operation) -> Result {
        match operation {
            Operation::CreateDelayedTrigger {
                event: _,
                start: _,
                expires: _,
                intervening_if: _,
                body: _,
            }
            | Operation::LookAt {
                player: _,
                objects: _,
            }
            | Operation::FlipCoin { .. }
            | Operation::CreateRegenerationShields { objects: _ }
            | Operation::Search { .. }
            | Operation::Reveal { objects: _ }
            | Operation::RevealUntil {
                library: _,
                filter: _,
                count: _,
                at_most: _,
            }
            | Operation::RevealUntilGroup {
                library: _,
                constraints: _,
            }
            | Operation::Shuffle {
                players: _,
                excluding: _,
            } => self.operation_case_7(operation),
            _ => self.operation_dispatch_8(operation),
        }
    }

    #[inline]
    pub(super) fn operation_dispatch_8(&self, operation: &Operation) -> Result {
        match operation {
            Operation::ReorderLibrary { .. } => self.operation_case_8(operation),
            Operation::Scry {
                players: _,
                amount: _,
            }
            | Operation::Surveil {
                players: _,
                amount: _,
            } => self.operation_case_8(operation),
            Operation::JoinResults { results } if results.len() < 2 => {
                self.operation_case_8(operation)
            }
            Operation::JoinResults { results: _ } => self.operation_case_8(operation),
            Operation::ChoosePowerToughness { .. } => self.operation_case_8(operation),
            Operation::Move {
                objects: _,
                destination: _,
                qualifiers: _,
            } => self.operation_case_8(operation),
            _ => self.operation_dispatch_9(operation),
        }
    }

    #[inline]
    pub(super) fn operation_dispatch_9(&self, operation: &Operation) -> Result {
        match operation {
            Operation::ModifyEvent { change: _ }
            | Operation::CreateTrigger { .. }
            | Operation::BindValue { value: _ }
            | Operation::RepeatProcess
            | Operation::DiscardObjects { objects: _ } => self.operation_case_9(operation),
            _ => self.operation_dispatch_10(operation),
        }
    }

    #[inline]
    pub(super) fn operation_dispatch_10(&self, operation: &Operation) -> Result {
        match operation {
            Operation::ReflexiveTrigger {
                performed: _,
                condition: _,
                announcement: _,
                body: _,
            }
            | Operation::ReturnAfter {
                objects: _,
                duration: _,
            }
            | Operation::CreateReplacement {
                event: _,
                limit: _,
                duration: _,
                body: _,
            }
            | Operation::PayCost { .. }
            | Operation::PayRepeatedly {
                player: _,
                cost: _,
                at_most: _,
            }
            | Operation::RemoveCounters { removals: _ }
            | Operation::Attach {
                attachments: _,
                host: _,
            } => self.operation_case_10(operation),
            _ => self.operation_dispatch_11(operation),
        }
    }

    #[inline]
    pub(super) fn operation_dispatch_11(&self, operation: &Operation) -> Result {
        match operation {
            Operation::CreateTokens {
                players: _,
                tokens: _,
                entry: _,
            }
            | Operation::SeparatePiles {
                separator: _,
                chooser: _,
                objects: _,
            }
            | Operation::ChooseObjects {
                player: _,
                objects: _,
                count: _,
                constraints: _,
            }
            | Operation::PutCounters { placements: _ } => self.operation_case_11(operation),
            _ => self.operation_dispatch_12(operation),
        }
    }

    #[inline]
    pub(super) fn operation_dispatch_12(&self, operation: &Operation) -> Result {
        match operation {
            Operation::Draw {
                players: _,
                amount: _,
            }
            | Operation::Discard { .. }
            | Operation::SetLifeTotal {
                players: _,
                amount: _,
            }
            | Operation::GainLife {
                players: _,
                amount: _,
            }
            | Operation::LoseLife {
                players: _,
                amount: _,
            }
            | Operation::Mill {
                players: _,
                amount: _,
            }
            | Operation::AddMana {
                players: _,
                mana: _,
                restriction: _,
            }
            | Operation::DealDamage { .. }
            | Operation::DealDamageWith {
                source: _,
                recipients: _,
                amount: _,
                rider: _,
            }
            | Operation::DealDamageParts { .. }
            | Operation::DealDamageFromEach {
                sources: _,
                recipients: _,
                amount: _,
            }
            | Operation::Destroy { .. }
            | Operation::SetTapped { .. }
            | Operation::Sacrifice {
                players: _,
                objects: _,
                choose: _,
            }
            | Operation::CreateContinuousEffect { .. } => self.operation_case_12(operation),
            _ => Err("unhandled operation in schema contract".into()),
        }
    }

    pub(super) fn operation_case_0(&self, operation: &Operation) -> Result {
        match operation {
            Operation::InOrder(actions) => {
                actions.iter().try_for_each(|action| self.operation(action))
            }
            Operation::WithSelections { steps, action } => {
                let mut scope = self.clone();
                let mut own = BTreeSet::new();
                for step in steps {
                    if !own.insert(step.binding.0) {
                        return Err("duplicate local selection binding".into());
                    }
                    if let SelectionChooser::Players(players) = &step.chooser {
                        scope.players(players)?;
                    }
                    scope.objects(&step.objects)?;
                    scope.count(&step.count)?;
                    scope.selection_constraints(&step.constraints)?;
                    let zones = scope.known_zones(&step.objects);
                    scope.prepared.insert(step.binding.0, zones);
                }
                scope.operation(action)
            }
            Operation::Meld { objects, entry, .. } => {
                self.objects(objects)?;
                self.entry(entry)?;
                arrival::battlefield(entry)
            }
            Operation::PutStickers {
                actor,
                objects,
                amount,
                ticket_cost,
                ..
            } => {
                self.players(actor)?;
                self.objects(objects)?;
                ticket_cost.iter().try_for_each(|(_, n)| self.expr(n))?;
                amount.iter().try_for_each(|n| self.expr(n))
            }
            Operation::Piles {
                actor,
                objects,
                count,
                chooser,
                ..
            } => {
                self.players(actor)?;
                self.objects(objects)?;
                count.iter().try_for_each(|n| self.expr(n))?;
                chooser.iter().try_for_each(|p| self.players(p))
            }
            Operation::ChoosePile {
                actor,
                from,
                object,
                ..
            } => {
                self.players(actor)?;
                from.iter().try_for_each(|p| self.players(p))?;
                object.iter().try_for_each(|o| self.object(o))
            }
            Operation::Note(kind) => match kind {
                NoteKind::Number(value) => self.expr(value),
                NoteKind::Player(player) => self.player(player),
                NoteKind::CountersOn(object) => self.object(object),
                NoteKind::Numbered(objects) => self.objects(objects),
                NoteKind::MostCommonCreatureType(players) => self.players(players),
                NoteKind::LifeTotal | NoteKind::ManaSpent { .. } => Ok(()),
            },
            Operation::Bid { players, start } => {
                self.players(players)?;
                start.iter().try_for_each(|n| self.expr(n))
            }
            Operation::Guess {
                actor,
                object,
                filter,
                player,
            } => {
                self.players(actor)?;
                object.iter().try_for_each(|o| self.object(o))?;
                filter.iter().try_for_each(|f| self.filter(f))?;
                player.iter().try_for_each(|p| self.player(p))
            }
            Operation::Seek {
                actor,
                cards,
                count,
                top,
            } => {
                self.players(actor)?;
                cards.iter().try_for_each(|f| self.filter(f))?;
                self.expr(count)?;
                top.iter().try_for_each(|n| self.expr(n))
            }
            Operation::DrawThenDiscard {
                players,
                draw,
                discard,
            } => {
                self.players(players)?;
                self.expr(draw)?;
                self.expr(discard)
            }
            Operation::RedistributeLife(players) => self.players(players),
            Operation::WithReflexive { action, .. } => self.operation(action),
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn operation_case_1(&self, operation: &Operation) -> Result {
        match operation {
            Operation::GiveControl {
                players, objects, ..
            } => {
                self.players(players)?;
                self.objects(objects)
            }
            Operation::AnyPlayerMayPay { players, .. } => self.players(players),
            Operation::EachPlayer { players, action } => {
                self.players(players)?;
                self.operation(action)
            }
            Operation::EachObject { objects, action } => {
                self.objects(objects)?;
                let mut each = self.clone();
                each.affected = true;
                each.operation(action)
            }
            Operation::LimitX { test } => self.number_test(test),
            Operation::RestartGame { keep, then } => {
                self.objects(keep)?;
                self.operation(then)
            }
            Operation::EachNumber { from, to, action } => {
                self.expr(from)?;
                self.expr(to)?;
                self.operation(action)
            }
            Operation::Counter {
                objects,
                destination_when: when,
                destination,
            } => {
                let view = crate::movement::entry_destination_view(destination);
                let (destination, entry) = (&view.destination, &view.entry);
                if let MovementPlacement::By(players) = &view.terms.placement {
                    self.players(players)?;
                }
                self.require_zone(objects, EventZone::Stack)?;
                when.iter().try_for_each(|f| self.filter(f))?;
                self.destination(destination)?;
                self.entry(entry)?;
                arrival::destination(entry, destination)
            }
            Operation::Populate { actor, entry } => {
                self.players(actor)?;
                self.entry(entry)?;
                arrival::battlefield(entry)
            }
            Operation::ReduceSpeed {
                player,
                amount,
                floor,
            } => {
                self.player(player)?;
                self.expr(amount)?;
                self.expr(floor)
            }
            Operation::PutCountersCapped {
                amount,
                object,
                at_most,
                ..
            } => {
                self.expr(amount)?;
                self.object(object)?;
                self.expr(at_most)
            }
            Operation::MillUntil {
                actor,
                cards,
                at_most,
            } => {
                self.players(actor)?;
                self.filter(cards)?;
                self.expr(at_most)
            }
            Operation::ChooseDefender { actor, attacker } => {
                self.players(actor)?;
                self.player(attacker)
            }
            Operation::PlayersPayRepeatedly {
                players,
                cost,
                at_most,
            } => {
                self.players(players)?;
                self.cost(cost)?;
                self.expr(at_most)
            }
            Operation::DelayedUnlessPaid { payer, .. } => self.player(payer),
            Operation::FightEach { fighters, others } => {
                self.objects(fighters)?;
                self.objects(others)
            }
            Operation::PickFromHand {
                actor,
                player,
                filter,
                ..
            } => {
                self.players(actor)?;
                self.player(player)?;
                self.filter(filter)
            }
            Operation::DistributeCounters {
                amount, objects, ..
            } => {
                self.expr(amount)?;
                self.objects(objects)
            }
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn operation_case_2(&self, operation: &Operation) -> Result {
        match operation {
            Operation::PutCountersEach {
                amount, objects, ..
            } => {
                self.objects(objects)?;
                let mut each = self.clone();
                each.affected = true;
                each.expr(amount)
            }
            Operation::PutChosenCounters { objects, choice } => {
                self.objects(objects)?;
                match choice {
                    CounterChoice::PerAbility { of, kinds, .. } => {
                        of.iter().try_for_each(|object| self.object(object))?;
                        kinds.iter().try_for_each(|kind| self.operand(kind))
                    }
                    _ => Ok(()),
                }
            }
            Operation::NoteObjects { objects } => self.objects(objects),
            Operation::Named { actor, form } => {
                self.players(actor)?;
                self.named(form)
            }
            Operation::Primitive {
                actor,
                name,
                operands,
            } => {
                self.players(actor)?;
                self.primitive(name, operands)
            }
            Operation::CreatePerpetualEffect { objects, changes } => {
                self.objects(objects)?;
                self.entering_changes(changes)
            }
            Operation::KeywordEach {
                actor,
                objects,
                action,
            } => {
                self.players(actor)?;
                self.objects(objects)?;
                crate::keyword_actions::each_parts(action)
                    .1
                    .map_or(Ok(()), |until| self.duration(until))
            }
            Operation::MoveTopCard {
                actor,
                cards,
                destination,
                ..
            } => {
                let view = crate::movement::entry_destination_view(destination);
                let (destination, entry) = (&view.destination, &view.entry);
                if let MovementPlacement::By(players) = &view.terms.placement {
                    self.players(players)?;
                }
                self.players(actor)?;
                self.filter(cards)?;
                self.destination(destination)?;
                self.entry(entry)?;
                arrival::destination(entry, destination)
            }
            Operation::PayAnyAmount { actor, .. } => self.players(actor),
            Operation::RevealFromHand {
                actor,
                cards,
                count,
                ..
            } => {
                self.players(actor)?;
                self.filter(cards)?;
                count.iter().try_for_each(|n| self.expr(n))
            }
            Operation::CopyOnStack {
                actor,
                object,
                times,
                targets,
                exceptions,
                card,
            } => {
                self.players(actor)?;
                self.object(object)?;
                self.expr(times)?;
                self.new_targets(targets)?;
                card.iter().try_for_each(|o| self.object(o))?;
                self.copy_exceptions(exceptions)
            }
            Operation::Retarget {
                actor,
                object,
                targets,
            } => {
                self.players(actor)?;
                self.object(object)?;
                self.new_targets(targets)
            }
            Operation::AbandonScheme { actor } => self.players(actor),
            Operation::Together(operations) => {
                operations.iter().try_for_each(|o| self.operation(o))
            }
            Operation::Repeat { times, body } => {
                self.expr(times)?;
                self.operation(body)
            }
            _ => unreachable!("validated dispatch"),
        }
    }
}
