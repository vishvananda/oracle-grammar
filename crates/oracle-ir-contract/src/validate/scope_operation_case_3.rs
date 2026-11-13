//! Scoped validation of portable operands.
use super::*;
impl<'a> Scope<'a> {
    pub(super) fn operation_case_3(&self, operation: &Operation) -> Result {
        match operation {
            Operation::TurnFaceDownAs {
                objects,
                as_creature,
            } => {
                self.objects(objects)?;
                self.face_down_shape(as_creature)
            }
            Operation::Exchange {
                first,
                second,
                until,
                ..
            } => {
                self.operand(first)?;
                self.operand(second)?;
                until.iter().try_for_each(|until| self.duration(until))
            }
            Operation::ChooseValue {
                choosers: actor,
                terms,
                ..
            } => {
                self.players(actor)?;
                terms.options.iter().try_for_each(|o| self.operand(o))?;
                terms.except.iter().try_for_each(|o| self.operand(o))?;
                match &terms.count {
                    ChoiceCount::Exactly(n) | ChoiceCount::UpTo(n) => self.expr(n),
                    ChoiceCount::AnyNumber => Ok(()),
                }
            }
            Operation::RevealChoices { .. } => Ok(()),
            Operation::FlipCoinSeries {
                players,
                repetition,
                ..
            } => {
                self.players(players)?;
                match repetition {
                    CoinRepetition::Count(n) => self.count(&Count::Exactly(n.clone())),
                    CoinRepetition::EachObject(objects) => self.objects(objects),
                    CoinRepetition::UpTo { extent, .. } => match extent {
                        CoinExtent::Count(n) => self.count(&Count::Exactly(n.clone())),
                        CoinExtent::Objects(objects) => self.objects(objects),
                    },
                    CoinRepetition::Until(_) => Ok(()),
                }
            }
            Operation::AttachEach {
                actor,
                attachments,
                count,
                hosts,
                ..
            } => {
                self.players(actor)?;
                self.objects(attachments)?;
                count.iter().try_for_each(|c| self.count(c))?;
                self.recipients(hosts)
            }
            Operation::CopyEach {
                actor,
                objects,
                targets,
            }
            | Operation::RetargetEach {
                actor,
                objects,
                targets,
            } => {
                self.players(actor)?;
                self.filter(objects)?;
                self.new_targets(targets)
            }
            Operation::TakeSpell {
                actor,
                object,
                targets,
            } => {
                self.players(actor)?;
                self.object(object)?;
                self.new_targets(targets)
            }
            Operation::SetX { object, value } => {
                self.object(object)?;
                self.expr(value)
            }
            Operation::ChangeCombat { change } => match change {
                CombatChange::Reselect { objects, .. } | CombatChange::OnlyBlockers(objects) => {
                    self.objects(objects)
                }
                CombatChange::OnlyAttackers { objects, .. } => self.objects(objects),
                CombatChange::AssignAttack { objects, to } => {
                    self.objects(objects)?;
                    self.operand(to)
                }
                CombatChange::Block { blocker, attackers } => {
                    self.object(blocker)?;
                    self.objects(attackers)
                }
                CombatChange::SwapBlocks { first, second, .. } => {
                    self.object(first)?;
                    self.object(second)
                }
            },
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn operation_case_4(&self, operation: &Operation) -> Result {
        match operation {
            Operation::PlayersDo {
                players,
                body,
                otherwise,
                ..
            } => {
                self.players(players)?;
                body.iter().try_for_each(|o| self.operand(o))?;
                otherwise.iter().try_for_each(|o| self.operand(o))
            }
            Operation::CreateBoon {
                actor,
                event,
                condition,
                times,
                body,
            } => {
                self.players(actor)?;
                self.event(event)?;
                self.expr(times)?;
                let mut child = self.clone();
                child.replacement = false;
                child.open_searches.clear();
                child.join_groups.clear();
                child.event = crate::semantics::event_bindings(event);
                condition.iter().try_for_each(|c| child.condition(c))?;
                child.operand(body)
            }
            Operation::AddPhases {
                actor,
                count,
                at_start,
                ..
            } => {
                self.players(actor)?;
                self.expr(count)?;
                at_start.iter().try_for_each(|o| self.operand(o))
            }
            Operation::ChooseAndPay {
                actor,
                objects,
                cost,
            } => {
                self.players(actor)?;
                self.objects(objects)?;
                self.cost(cost)
            }
            Operation::DuplicateCounters { from, to, .. } => {
                match from {
                    CounterSource::On(recipients) => self.recipients(recipients)?,
                    CounterSource::EventPut if self.event.counters_placed => (),
                    CounterSource::EventPut => {
                        return Err("counter source requires a counter-placement event".into());
                    }
                }
                self.recipients(to)
            }
            Operation::EffectOn {
                subject,
                effects,
                until,
            } => {
                match subject {
                    EffectSubject::Objects(objects) => self.objects(objects)?,
                    EffectSubject::EachSpell(spells) => self.filter(spells)?,
                    EffectSubject::NextSpell { spells, caster, .. } => {
                        self.filter(spells)?;
                        self.player(caster)?;
                    }
                    EffectSubject::Players(players) => self.players(players)?,
                }
                if let Some(until) = until {
                    self.duration(until)?;
                }
                effects.iter().try_for_each(|effect| self.operand(effect))
            }
            Operation::GameEffect { until, effect } => {
                self.duration(until)?;
                self.operand(effect)
            }
            Operation::AttachChosen {
                actor,
                attachments,
                count,
                hosts,
            } => {
                self.players(actor)?;
                self.objects(attachments)?;
                self.count(count)?;
                self.objects(hosts)
            }
            Operation::DrawUpTo { players, amount } => {
                self.players(players)?;
                self.expr(amount)
            }
            Operation::DiscardAllBut { players, keep } => {
                self.players(players)?;
                self.expr(keep)
            }
            Operation::SacrificeAllBut {
                players,
                objects,
                keep,
            } => {
                self.players(players)?;
                self.objects(objects)?;
                self.expr(keep)
            }
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn operation_case_5(&self, operation: &Operation) -> Result {
        match operation {
            Operation::TowardPlayer { actor, player, .. } => {
                self.players(actor)?;
                self.player(player)
            }
            Operation::Intensify { cards, owner } => {
                self.filter(cards)?;
                self.player(owner)
            }
            Operation::FlipCoins { players, count, .. } => {
                self.players(players)?;
                self.count(&Count::Exactly(count.clone()))
            }
            Operation::Wish { actor, filter, .. } => {
                self.players(actor)?;
                self.filter(filter)
            }
            Operation::PutSticker { actor, objects, .. } => {
                self.players(actor)?;
                self.objects(objects)
            }
            Operation::BeginOnBattlefield { actor, flags } => {
                self.players(actor)?;
                self.operand(flags)
            }
            Operation::Manifest {
                actor,
                source,
                entry,
                ..
            } => {
                self.players(actor)?;
                self.entry(entry)?;
                arrival::battlefield(entry)?;
                match source {
                    ManifestSource::TopOfLibrary { player, count } => {
                        self.player(player)?;
                        self.expr(count)
                    }
                    ManifestSource::Cards(cards) => self.objects(cards),
                }
            }
            Operation::Vote { actor, ballot, .. } => {
                self.players(actor)?;
                match ballot {
                    Ballot::Words(_) => Ok(()),
                    Ballot::Players(players) => self.players(players),
                    Ballot::Objects(objects) | Ballot::UpToOne(objects) => self.objects(objects),
                    Ballot::GraveyardCards(cards) => self.filter(cards),
                }
            }
            Operation::Clash {
                actor,
                opponents,
                won,
                lost,
            } => {
                self.players(actor)?;
                self.players(opponents)?;
                let mut child = self.clone();
                child.open_searches.clear();
                child.join_groups.clear();
                child.clashing = true;
                won.iter()
                    .chain(lost)
                    .try_for_each(|p| child.clone().program(p))
            }
            Operation::DraftFromSpellbook { actor, times, .. } => {
                self.players(actor)?;
                self.expr(times)
            }
            Operation::CreateEmblem { actor, abilities } => {
                self.players(actor)?;
                abilities
                    .iter()
                    .try_for_each(|a| self.operand(&Operand::Ability(Box::new(a.clone()))))
            }
            Operation::MoveCounters {
                actor,
                from,
                to,
                amount,
                ..
            } => {
                self.players(actor)?;
                self.objects(from)?;
                self.objects(to)?;
                match amount {
                    MovedCounters::Exactly(amount) | MovedCounters::AtLeast(amount) => {
                        self.expr(amount)
                    }
                    _ => Ok(()),
                }
            }
            Operation::EachCounterKind {
                actor,
                among,
                options,
                ..
            } => {
                self.players(actor)?;
                self.objects(among)?;
                options.iter().try_for_each(|option| match option {
                    CounterStep::PutThatKind(object)
                    | CounterStep::Put(_, object)
                    | CounterStep::RemoveThatKind(object) => self.object(object),
                })
            }
            Operation::DoubleCounters { objects, .. } => self.objects(objects),
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn operation_case_6(&self, operation: &Operation) -> Result {
        match operation {
            Operation::Conjure {
                actor,
                card,
                count,
                owner,
                destination,
            } => {
                let (zone, spot, entry) = crate::movement::conjure_view(destination);
                let (zone, spot, entry) = (&zone, &spot, &entry);
                self.players(actor)?;
                self.expr(count)?;
                if let ConjuredCard::Duplicate(objects) = card {
                    self.objects(objects)?;
                }
                if let ConjuredCard::DuplicateLastKnown(object) = card {
                    self.object(object)?;
                }
                if let Some(owner) = owner {
                    self.player(owner)?;
                }
                if let Some(ConjureSpot::AmongTop(n) | ConjureSpot::FromTop(n)) = spot {
                    self.expr(n)?;
                }
                self.entry(entry)?;
                arrival::conjure(entry, zone, spot)
            }
            Operation::ChangeSpell { subject, changes } => {
                match subject {
                    SpellSubject::One(spell) => self.object(spell)?,
                    SpellSubject::Next { spells, player, .. } => {
                        self.filter(spells)?;
                        self.player(player)?;
                    }
                }
                changes.iter().try_for_each(|change| self.change(change))
            }
            Operation::PlayCards {
                actor,
                cards,
                count,
                ..
            } => {
                self.players(actor)?;
                self.objects(cards)?;
                self.count(count)
            }
            Operation::PlayWith {
                actor,
                cards,
                count,
                terms,
                group,
                ..
            } => {
                self.players(actor)?;
                self.objects(cards)?;
                self.count(count)?;
                self.play_cost(&terms.cost)?;
                if let Some(then) = &terms.then {
                    self.destination(then)?;
                }
                terms.effects.iter().try_for_each(|o| self.operand(o))?;
                group.iter().try_for_each(|o| self.operand(o))
            }
            Operation::DraftChangeZone { .. } => {
                Err("a draft zone change must be elaborated before validation".into())
            }
            Operation::CopyCards {
                actor,
                objects,
                times,
            } => {
                self.players(actor)?;
                self.objects(objects)?;
                self.expr(times)
            }
            Operation::ChooseAtRandom { options, .. } => {
                options.iter().try_for_each(|o| self.operand(o))
            }
            Operation::ChooseAction { actor, options, .. } => {
                self.players(actor)?;
                options.iter().try_for_each(|o| self.operand(o))
            }
            Operation::Keyword { actor, action } => {
                self.players(actor)?;
                let parts = crate::keyword_actions::parts(action);
                parts
                    .object
                    .into_iter()
                    .chain(parts.other)
                    .try_for_each(|o| self.object(o))?;
                parts.amount.map_or(Ok(()), |n| self.expr(n))
            }
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn operation_case_7(&self, operation: &Operation) -> Result {
        match operation {
            Operation::CreateDelayedTrigger {
                event,
                start,
                expires,
                intervening_if,
                body,
            } => {
                self.event(event)?;
                if let DelayedStart::PlayersNextTurn(player) = start {
                    self.player(player)?;
                }
                if let DelayedStart::ExtraTurn(result) = start
                    && !self.results.contains_key(&result.0)
                {
                    return Err(format!("result {} created no extra turn", result.0));
                }
                if let Some(duration) = expires {
                    self.duration(duration)?;
                    if matches!(duration, Duration::Indefinite) {
                        return Err("indefinite delayed expiration should be absent".into());
                    }
                }
                // Lexical closure: preserve creator bindings, not the turn
                // event's bindings. New results/searches are local to this
                // future invocation; its mutations do not occur at creation.
                let mut child = self.clone();
                child.replacement = false;
                child.open_searches.clear();
                child.join_groups.clear();
                // A row's event says what it binds, as a typed one's creator did.
                if crate::semantics::delayed_reads_event(event) {
                    child.event = crate::semantics::with_event(&child.event, event);
                }
                if let Some(condition) = intervening_if {
                    child.condition(condition)?;
                }
                child.program(body)
            }
            Operation::LookAt { player, objects } => {
                self.player(player)?;
                self.objects(objects)
            }
            Operation::FlipCoin { player, .. } => self.player(player),
            Operation::CreateRegenerationShields { objects } => {
                self.require_zone(objects, EventZone::Battlefield)
            }
            Operation::Search {
                searcher: player,
                library_owner: library,
                top,
                choices,
                ..
            } => {
                self.player(player)?;
                self.player(library)?;
                if let Some(n) = top {
                    self.count(&Count::Exactly(n.clone()))?;
                }
                for choice in choices {
                    self.selection_constraints(&choice.constraints)?;
                    self.filter(&choice.filter)?;
                    self.count(&choice.count)?;
                    let whole_group =
                        matches!(choice.count, Count::Exactly(_)) && !choice.constraints.is_empty();
                    if !whole_group
                        && !matches!(
                            choice.count,
                            Count::UpTo(_) | Count::AsManyAsCan(_) | Count::AnyNumber
                        )
                    {
                        return Err(
                            "search choice requires a fail-to-find or as-many-as-possible policy"
                                .into(),
                        );
                    }
                }
                Ok(())
            }
            Operation::Reveal { objects } => self.objects(objects),
            Operation::RevealUntil {
                library,
                filter,
                count,
                at_most,
            } => {
                self.player(library)?;
                self.expr(count)?;
                if let Some(at_most) = at_most {
                    self.expr(at_most)?;
                }
                self.filter(filter)
            }
            Operation::RevealUntilGroup {
                library,
                constraints,
            } => {
                self.player(library)?;
                self.selection_constraints(constraints)
            }
            Operation::Shuffle { players, excluding } => {
                self.players(players)?;
                if let Some(objects) = excluding {
                    self.require_zone(objects, EventZone::Library)?;
                }
                Ok(())
            }
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn operation_case_8(&self, operation: &Operation) -> Result {
        match operation {
            Operation::ReorderLibrary {
                chooser: player,
                library_owner: library,
                objects,
                ..
            } => {
                self.player(player)?;
                self.player(library)?;
                self.require_zone(objects, EventZone::Library)
            }
            Operation::Scry { players, amount } | Operation::Surveil { players, amount } => {
                self.players(players)?;
                self.count(&Count::Exactly(amount.clone()))
            }
            Operation::JoinResults { results } if results.len() < 2 => {
                Err("a join needs at least two branches".into())
            }
            Operation::JoinResults { results } => {
                if self.join_groups.iter().rev().any(|group| {
                    results
                        .iter()
                        .all(|r| self.results.contains_key(&r.0) || group.contains(&r.0))
                }) {
                    Ok(())
                } else {
                    Err("a join references unavailable or unrelated branch receipts".into())
                }
            }
            Operation::ChoosePowerToughness { player, .. } => self.player(player),
            Operation::Move {
                objects,
                destination,
                qualifiers,
            } => {
                let crate::movement::DestinationView {
                    destination,
                    entry,
                    face_down,
                    terms,
                } = crate::movement::destination_view(destination, qualifiers);
                if entry.controller.is_some() {
                    return Err("a move's controller belongs to its destination".into());
                }
                self.objects(objects)?;
                self.destination(&destination)?;
                self.selection_constraints(&terms.qualifiers)?;
                if let MovementPlacement::By(players) = &terms.placement {
                    self.players(players)?;
                    if !matches!(destination, Destination::OwnersLibraryTopOrBottom) {
                        return Err(
                            "a placement chooser requires a top-or-bottom destination".into()
                        );
                    }
                }
                if face_down && !matches!(destination, Destination::Exile) {
                    return Err("face-down movement currently requires exile".into());
                }
                self.entry(&entry)?;
                arrival::destination(&entry, &destination)
            }
            _ => unreachable!("validated dispatch"),
        }
    }
}
