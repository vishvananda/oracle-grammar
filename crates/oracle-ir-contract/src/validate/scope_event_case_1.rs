//! Scoped validation of portable operands.
use super::*;
impl<'a> Scope<'a> {
    pub(super) fn event_case_1(&self, event: &EventSpec) -> Result {
        match event {
            EventSpec::Player {
                players, objects, ..
            } => {
                let mut scoped = self.clone();
                scoped.event = crate::semantics::with_event(&self.event, event);
                scoped.players(players)?;
                objects.iter().try_for_each(|filter| scoped.filter(filter))
            }
            EventSpec::NthEachTurn { event, test } => {
                self.event(event)?;
                self.number_test(test)
            }
            EventSpec::Pair {
                objects, others, ..
            } => {
                let mut scoped = self.clone();
                scoped.event = crate::semantics::with_event(&self.event, event);
                scoped.filter(objects)?;
                scoped.filter(others)
            }
            EventSpec::BecomesTarget { objects, by, .. } => {
                let mut scoped = self.clone();
                scoped.event = crate::semantics::with_event(&self.event, event);
                scoped.filter(objects)?;
                scoped.operand(by)
            }
            EventSpec::Attack {
                attackers,
                by,
                defenders,
            } => {
                let mut scoped = self.clone();
                scoped.event = crate::semantics::with_event(&self.event, event);
                scoped.filter(attackers)?;
                if let Some(by) = by {
                    self.players(by)?;
                }
                self.defenders(defenders)
            }
            EventSpec::Batch {
                event: inner,
                count,
                including,
                ..
            } => {
                self.event(inner)?;
                if let Some((_, value)) = count {
                    self.expr(value)?;
                }
                match including {
                    Some(object) => self.object(object),
                    None => Ok(()),
                }
            }
            EventSpec::Counters { objects, by, .. } => {
                let mut scoped = self.clone();
                scoped.event = crate::semantics::with_event(&self.event, event);
                scoped.filter(objects)?;
                match by {
                    Some(by) => self.players(by),
                    None => Ok(()),
                }
            }
            EventSpec::Activated { players, abilities } => {
                self.players(players)?;
                let mut scoped = self.clone();
                scoped.event = crate::semantics::with_event(&self.event, event);
                scoped.filter(abilities)
            }
            EventSpec::SagaChapter(_) => Ok(()),
            EventSpec::Discard { players, cards, .. } => {
                self.players(players)?;
                self.filter(cards)
            }
            EventSpec::ZoneChange { objects, .. } => self.filter(objects),
            EventSpec::Object { objects, .. } => self.filter(objects),
            EventSpec::Spell {
                players, spells, ..
            } => {
                self.players(players)?;
                self.filter(spells)
            }
            EventSpec::Turn { players, .. } => self.players(players),
            EventSpec::Damage(event) => {
                self.filter(&event.sources)?;
                // A recipient can be named by the source that dealt the damage.
                let bound = crate::semantics::event_bindings(&EventSpec::Damage(event.clone()));
                let mut scoped = self.clone();
                scoped.event.damage_source |= bound.damage_source;
                match &event.recipients {
                    DamageRecipients::Any => Ok(()),
                    DamageRecipients::Target { slot, .. } | DamageRecipients::Targets(slot) => {
                        self.recipients(&Recipients::Targets(slot.clone()))
                    }
                    DamageRecipients::Objects(f) => scoped.filter(f),
                    DamageRecipients::Players(p) => scoped.players(p),
                    DamageRecipients::ObjectsAndPlayers { objects, players } => {
                        scoped.filter(objects)?;
                        scoped.players(players)
                    }
                }
            }
            EventSpec::State { condition } => self.condition(condition),
            EventSpec::StateWithPlayer { condition, player } => {
                self.condition(condition)?;
                self.player(player)
            }
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn event_case_2(&self, event: &EventSpec) -> Result {
        match event {
            EventSpec::WithOtherObject { event, object } => {
                self.event(event)?;
                let bindings = crate::semantics::event_bindings(event);
                if bindings.other {
                    return Err("captured secondary object would overwrite an event binding".into());
                }
                let mut matched = self.clone();
                matched.event = bindings;
                matched.object(object)
            }
            EventSpec::AnyOf(events) => {
                if events.is_empty() {
                    return Err("alternative event matcher is empty".into());
                }
                for event in events {
                    self.event(event)?;
                }
                Ok(())
            }
            EventSpec::Filtered { event, condition } => {
                self.event(event)?;
                let mut event_scope = self.clone();
                event_scope.event = crate::semantics::event_bindings(event);
                event_scope.condition(condition)
            }
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn target(&self, slot: &TargetSlot, index: Option<u16>, kind: TargetKind) -> Result {
        let spec = self
            .slots
            .get(&slot.0)
            .ok_or_else(|| format!("unbound target slot {}", slot.0))?;
        if std::mem::discriminant(&spec.kind) != std::mem::discriminant(&kind) {
            return Err(format!("target slot {} has the wrong entity kind", slot.0));
        }
        if let (Some(index), Count::Exactly(Expr::Constant(max)) | Count::UpTo(Expr::Constant(max))) =
            (index, &spec.count)
            && i64::from(index) >= *max
        {
            return Err(format!("target index {index} exceeds slot {}", slot.0));
        }
        Ok(())
    }

    pub(super) fn object(&self, object: &ObjectRef) -> Result {
        match object {
            ObjectRef::Scoped => Ok(()),
            ObjectRef::RingBearer(player) => self.player(player),
            ObjectRef::HauntedBy(object) => self.object(object),
            ObjectRef::TheOther { among, excluding } => {
                self.objects(among)?;
                self.objects(excluding)
            }
            ObjectRef::AtEnd { objects, .. } => self.objects(objects),
            ObjectRef::AuraAfterEnchantedLeft { aura, host } => {
                if !self.event.enchanted_left {
                    return Err("Aura graveyard reference has no enchanted-permanent departure in this scope".into());
                }
                self.object(aura)?;
                self.object(host)
            }
            ObjectRef::NthChosen { by, .. } => by.iter().try_for_each(|player| self.player(player)),
            ObjectRef::Recorded { result, by } => {
                if !self.results.contains_key(&result.0) {
                    return Err(format!("result {} has not run", result.0));
                }
                by.iter().try_for_each(|player| self.player(player))
            }
            ObjectRef::Earlier { by, .. } => by.iter().try_for_each(|player| self.player(player)),
            ObjectRef::SourceOf(ability) => self.object(ability),
            ObjectRef::Discarded if self.event.discarded => Ok(()),
            ObjectRef::Discarded => {
                Err("discard event does not bind a singular discarded card".into())
            }
            ObjectRef::Damage(role) => match role {
                DamageSubject::Source if self.event.damage_source => Ok(()),
                DamageSubject::Recipient if self.event.damage_recipient => Ok(()),
                _ => Err("damage event does not bind a singular object for this role".into()),
            },
            ObjectRef::Query { objects, index } => {
                self.objects(objects)?;
                if !self.ordered(objects) && !self.singular_history(objects, *index) {
                    return Err("indexed query requires one ordered zone".into());
                }
                Ok(())
            }
            ObjectRef::Candidate if self.candidate => Ok(()),
            ObjectRef::Candidate => Err("filter candidate used outside an object predicate".into()),
            ObjectRef::Source => Ok(()),
            ObjectRef::MadeHere | ObjectRef::GrantingObject => Ok(()),
            ObjectRef::ChosenBy { result, chooser } => {
                self.player(chooser)?;
                match self.results.get(&result.0) {
                    Some(ResultKind::Selection | ResultKind::Search) => Ok(()),
                    _ => Err(format!(
                        "result {} is not an available object selection",
                        result.0
                    )),
                }
            }
            ObjectRef::AbilityHolder if self.keyword_host => Ok(()),
            ObjectRef::AbilityHolder => {
                Err("keyword host used outside its keyword definition".into())
            }
            ObjectRef::SoleTargetOf(spell) => self.object(spell),
            ObjectRef::AttachedTo(attachment) | ObjectRef::PairedWith(attachment) => {
                self.object(attachment)
            }
            ObjectRef::Affected if self.affected => Ok(()),
            ObjectRef::Affected => {
                Err("affected object used outside a characteristic change".into())
            }
            ObjectRef::Target { slot, index } => self.target(
                slot,
                Some(*index),
                TargetKind::Object(ObjectDomain::permanent()),
            ),
            ObjectRef::EventSubject if self.event.object => Ok(()),
            ObjectRef::EventOther if self.event.other => Ok(()),
            ObjectRef::EventOther => Err("this event names no second object".into()),
            ObjectRef::EventStackObject if self.event.stack_object => Ok(()),
            ObjectRef::EventStackObject => Err("this event names no spell or ability".into()),
            ObjectRef::RecipientObject { slot, index } => {
                self.target(slot, Some(*index), TargetKind::DamageRecipient)
            }
            ObjectRef::BlockPair(_) if self.event.block_pair => Ok(()),
            ObjectRef::BlockPair(_) => Err("this event pairs no attacker and blocker".into()),
            ObjectRef::EventDestination if self.event.moved => Ok(()),
            ObjectRef::EventDestination => Err("this event moves no object".into()),
            ObjectRef::Chosen => Ok(()),
            ObjectRef::Primitive { name, operands } => self.primitive(name, operands),
            ObjectRef::Result {
                result,
                phase: ObjectPhase::After,
                ..
            } if matches!(
                self.results.get(&result.0),
                Some(ResultKind::CreatedObjects)
            ) =>
            {
                Ok(())
            }
            ObjectRef::Result { result, .. } if self.gathered.contains(&result.0) => {
                Err("a player loop's choice is a set, not one object".into())
            }
            ObjectRef::Result { result, .. }
                if matches!(
                    self.results.get(&result.0),
                    Some(ResultKind::Selection | ResultKind::Search | ResultKind::ObjectTransition)
                ) =>
            {
                Ok(())
            }
            ObjectRef::Bound(binding) if self.bound_objects.contains(&binding.0) => Ok(()),
            ObjectRef::Bound(_) => Err("object binding has no declaration".into()),
            ObjectRef::EventSubject => Err("event subject used outside a triggered ability".into()),
            ObjectRef::Result { result, .. } => Err(format!(
                "result {} does not provide objects in this scope",
                result.0
            )),
        }
    }

    pub(super) fn entering_changes(&self, changes: &[Change]) -> Result {
        let mut scope = self.clone();
        scope.affected = true;
        changes.iter().try_for_each(|c| scope.change(c))
    }

    pub(super) fn entry(&self, entry: &dyn crate::movement::ArrivalView) -> Result {
        let entry_view = entry.entry_view();
        let entry: &EntrySpec = &entry_view;
        if let Some(EntryController::Player(player)) = &entry.controller {
            self.player(player)?;
        }
        for (_, amount) in &entry.counters {
            self.count(&Count::Exactly(amount.clone()))?;
        }
        entry.states.iter().try_for_each(|state| match state {
            EntryState::CopyOf {
                objects,
                exceptions,
                until,
            } => {
                self.objects(objects)?;
                until.iter().try_for_each(|d| self.duration(d))?;
                self.copy_exceptions(exceptions)
            }
            EntryState::CountersOfChoice { amount, .. } => {
                self.count(&Count::Exactly(amount.clone()))
            }
            EntryState::LinkExiledCards(objects) => self.objects(objects),
            EntryState::CountersOf { object, .. } => self.object(object),
            EntryState::Transformed
            | EntryState::Prepared
            | EntryState::FaceDown
            | EntryState::Flipped
            | EntryState::NotAnAura
            | EntryState::NotedCounters
            | EntryState::ExilePile { .. } => Ok(()),
            EntryState::FaceDownAs(shape) => self.face_down_shape(shape),
            EntryState::FaceDownLand(_) => Ok(()),
            EntryState::Optional { decider, entry } => {
                self.player(decider)?;
                self.entry(entry)
            }
            EntryState::FaceDownWith {
                face_down,
                abilities,
                noted,
            } => {
                self.face_down_shape(face_down)?;
                abilities.iter().try_for_each(validate_ability)?;
                noted.iter().try_for_each(|card| self.object(card))
            }
            EntryState::Perpetual(changes) => {
                changes.iter().try_for_each(|change| self.change(change))
            }
            EntryState::ExiledFor(until) => self.duration(until),
            EntryState::ExiledForWith {
                until,
                objects,
                body,
            } => {
                self.duration(until)?;
                let mut returning = self.clone();
                returning.replacement = false;
                returning.open_searches.clear();
                returning.join_groups.clear();
                returning.event =
                    crate::semantics::event_bindings(&crate::semantics::entry_return_event());
                returning.objects(objects)?;
                returning.program(body)
            }
            EntryState::Attacking { player, .. } => player.iter().try_for_each(|p| self.player(p)),
            EntryState::Blocking(object) | EntryState::AttachedTo(object) => self.object(object),
            EntryState::AttachedToChosen(filter) => self.filter(filter),
            EntryState::AttachedToPlayer(player) => self.player(player),
            EntryState::Draft { name, operands } => {
                operands.iter().try_for_each(|o| self.operand(o))?;
                Err(format!("unresolved entry flag {name}"))
            }
            EntryState::Changes(changes) => self.entering_changes(changes),
            EntryState::ChooseChanges(options) => options
                .iter()
                .try_for_each(|changes| self.entering_changes(changes)),
            EntryState::When { filter, entry } => {
                self.filter(filter)?;
                self.entry(entry)
            }
            EntryState::ChangesUntil { changes, duration } => {
                self.duration(duration)?;
                self.entering_changes(changes)
            }
        })
    }

    pub(super) fn face_down_shape(&self, shape: &FaceDownAs) -> Result {
        self.expr(&shape.power)?;
        self.expr(&shape.toughness)
    }

    pub(super) fn duration(&self, duration: &Duration) -> Result {
        match duration {
            Duration::UntilEvent(event) => self.event(event),
            Duration::While(condition) => self.condition(condition),
            Duration::WhileItMatches(filter) => self.filter(filter),
            Duration::ThroughNextTurn | Duration::UntilStateBasedActionsChecked => Ok(()),
            Duration::DuringControllersNextUntapSteps(steps) => self.expr(steps),
            Duration::EndOfCombatOnNextTurn(player)
            | Duration::UntilNextUntapStep(player)
            | Duration::UntilLibraryTopChanges(player)
            | Duration::WhilePlayerHasCounter { player, .. } => self.player(player),
            Duration::UntilAnotherExiled { by, with } => {
                self.players(by)?;
                self.object(with)
            }
            Duration::ExtraCombat(result) if self.results.contains_key(&result.0) => Ok(()),
            Duration::ExtraCombat(result) => Err(format!("result {} is unavailable", result.0)),
            Duration::Primitive { name, operands } => self.primitive(name, operands),
            Duration::ExtraTurn(result) if self.results.contains_key(&result.0) => Ok(()),
            Duration::ExtraTurn(result) => Err(format!("result {} is unavailable", result.0)),
            Duration::EndOfTurn
            | Duration::EndOfCombat
            | Duration::Indefinite
            | Duration::DuringControllersNextUntapStep => Ok(()),
            Duration::UntilNextTurn(player)
            | Duration::UntilEndOfNextTurn(player)
            | Duration::UntilEndOfNextUpkeep(player)
            | Duration::DuringNextUntapStep(player)
            | Duration::DuringNextTurn(player) => self.player(player),
            Duration::WhileOnBattlefield(object) | Duration::WhileExiled(object) => {
                self.object(object)
            }
            Duration::WhileControlled { player, object } => {
                self.player(player)?;
                self.object(object)
            }
            Duration::UntilPaid { player, cost } => {
                self.player(player)?;
                self.cost(cost)
            }
            Duration::WhileAnyExiled(objects) => self.objects(objects),
            Duration::UntilCast(object) => self.object(object),
            Duration::WhileMatches { object, filter } => {
                self.object(object)?;
                self.filter(filter)
            }
            Duration::UntilNext { players, .. } => self.players(players),
            Duration::Either(durations) => durations.iter().try_for_each(|d| self.duration(d)),
        }
    }

    pub(super) fn player(&self, player: &PlayerRef) -> Result {
        match player {
            PlayerRef::NotedFor(object) => self.object(object),
            PlayerRef::OtherChosen(player) => self.player(player),
            PlayerRef::AsOf { moment, player } => {
                if let Moment::LastEvent { event, .. } = moment.as_ref() {
                    self.event(event)?;
                }
                self.player(player)
            }
            PlayerRef::InChosenDirection { from, among } => {
                from.iter().try_for_each(|player| self.player(player))?;
                among.iter().try_for_each(|players| self.players(players))
            }
            PlayerRef::Named { result, .. } if self.results.contains_key(&result.0) => Ok(()),
            PlayerRef::Named { result, .. } => Err(format!("result {} has not run", result.0)),
            PlayerRef::Prior(_) => Ok(()),
            PlayerRef::Seat { of, .. } => self.player(of),
            PlayerRef::AttackingPlayer | PlayerRef::DefendingPlayer | PlayerRef::Holder(_) => {
                Ok(())
            }
            PlayerRef::Extreme { players, .. } => self.players(players),
            PlayerRef::SoleTargetOf(object) => self.object(object),
            PlayerRef::WardPayer if self.ward_payer => Ok(()),
            PlayerRef::WardPayer => Err("ward payer used outside a ward cost".into()),
            PlayerRef::Affected if self.affected_player => Ok(()),
            PlayerRef::Affected => {
                Err("affected player used outside a player continuous effect".into())
            }
            PlayerRef::EnchantedBy(object) => self.object(object),
            PlayerRef::PlayerOrControllerOf(recipient) => self.recipients(recipient),
            PlayerRef::Acting | PlayerRef::GiftedPlayer => Ok(()),
            PlayerRef::Performer(result) => match self.results.get(&result.0) {
                Some(ResultKind::Payment | ResultKind::Selection) => Ok(()),
                _ => Err(format!("result {} has no one who performed it", result.0)),
            },
            PlayerRef::ClashOpponent if self.clashing => Ok(()),
            PlayerRef::ClashOpponent => Err("a clash opponent outside a clash".into()),
            PlayerRef::DeclaringPlayer if self.declaration => Ok(()),
            PlayerRef::DeclaringPlayer => {
                Err("declaring player used outside a combat declaration cost".into())
            }
            PlayerRef::AbilityController | PlayerRef::Active => Ok(()),
            PlayerRef::Chosen | PlayerRef::ModeChooser => Ok(()),
            PlayerRef::Primitive { name, operands } => self.primitive(name, operands),
            PlayerRef::FirstWilling(players) | PlayerRef::OneOf(players) => self.players(players),
            PlayerRef::CasterOf(object)
            | PlayerRef::ControllerOf(object)
            | PlayerRef::OwnerOf(object)
            | PlayerRef::DefendingPlayerOf(object) => self.object(object),
            PlayerRef::Target { slot, index } => {
                self.target(slot, Some(*index), TargetKind::Player)
            }
            PlayerRef::EventOtherPlayer if self.event.other_player => Ok(()),
            PlayerRef::EventOtherPlayer => Err("this event names no second player".into()),
            PlayerRef::EventSubject if self.event.player => Ok(()),
            PlayerRef::EventSubject => Err("this event does not bind a player subject".into()),
            PlayerRef::Bound(binding) if self.players.contains(&binding.0) => Ok(()),
            PlayerRef::Bound(_) => Err("player binding has no declaration".into()),
        }
    }

    pub(super) fn players(&self, players: &PlayerSet) -> Result {
        match players {
            PlayerSet::Scoped => Ok(()),
            PlayerSet::Intersection(sets) => sets.iter().try_for_each(|s| self.players(s)),
            PlayerSet::Matching { binding, condition } => {
                let mut child = self.clone();
                if !child.players.insert(binding.0) {
                    return Err("player predicate redeclares a binding".into());
                }
                child.condition(condition)
            }
            PlayerSet::Union(sets) => {
                for players in sets {
                    self.players(players)?;
                }
                Ok(())
            }
            PlayerSet::DamageRecipients if self.event.damage_players => Ok(()),
            PlayerSet::DamageRecipients => {
                Err("damage event does not bind player recipients".into())
            }
            PlayerSet::One(p) | PlayerSet::OpponentsOf(p) | PlayerSet::OthersThan(p) => {
                self.player(p)
            }
            PlayerSet::DamagedBy(result) if self.results.contains_key(&result.0) => Ok(()),
            PlayerSet::DamagedBy(result) => Err(format!("result {} is unavailable", result.0)),
            PlayerSet::Targets(slot) => self.target(slot, None, TargetKind::Player),
            PlayerSet::All => Ok(()),
        }
    }
}
