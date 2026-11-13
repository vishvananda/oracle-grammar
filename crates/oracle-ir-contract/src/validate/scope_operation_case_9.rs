//! Scoped validation of portable operands.
use super::*;
impl<'a> Scope<'a> {
    pub(super) fn operation_case_9(&self, operation: &Operation) -> Result {
        match operation {
            Operation::ModifyEvent { change } => {
                if !self.replacement {
                    return Err("event change used outside a replacement program".into());
                }
                match change {
                    EventChange::SetTapped(_)
                    | EventChange::AddCopy { .. }
                    | EventChange::FlipExtraCoin
                    | EventChange::DamageAsInfect
                    | EventChange::IgnoreCounters(_) => Ok(()),
                    EventChange::LeaveLoyalty(amount) => {
                        if !self.event.damage_objects {
                            return Err(
                                "loyalty-removal change requires damage to an object".into()
                            );
                        }
                        self.count(&Count::Exactly(amount.clone()))
                    }
                    EventChange::SearchTop(amount) | EventChange::Repeats(amount) => {
                        self.expr(amount)
                    }
                    EventChange::AddCounters { amount, .. } => {
                        self.count(&Count::Exactly(amount.clone()))
                    }
                    EventChange::SetController(player) => self.player(player),
                    EventChange::ExchangeDieResult {
                        decider,
                        object,
                        properties,
                    } => {
                        self.player(decider)?;
                        self.object(object)?;
                        if properties.len() < 2 {
                            return Err("an exchange offers at least two properties".into());
                        }
                        Ok(())
                    }
                    EventChange::PreventDamage
                    | EventChange::Cancel
                    | EventChange::SetDestination(_) => Ok(()),
                    EventChange::PreventSomeDamage(amount)
                    | EventChange::PreventAllButDamage(amount)
                    | EventChange::SetAmount(amount) => self.count(&Count::Exactly(amount.clone())),
                    EventChange::RedirectDamage(recipients) => self.recipients(recipients),
                    EventChange::RedirectSomeDamage(amount, recipients) => {
                        self.expr(amount)?;
                        self.recipients(recipients)
                    }
                    EventChange::Entry(state) => self.entry(&BattlefieldArrival {
                        states: vec![state.clone()],
                        ..BattlefieldArrival::default()
                    }),
                    EventChange::AddMana(mana)
                    | EventChange::ReplaceMana(mana)
                    | EventChange::ReplaceColoredMana(mana) => self.mana(mana),
                    EventChange::RollExtraDie(DiceIgnore::Lowest) => Ok(()),
                    EventChange::RollExtraDie(DiceIgnore::ChosenBy(player)) => self.player(player),
                    EventChange::Then(body) => {
                        // The event is over, so nothing here can change it.
                        let mut child = self.clone();
                        child.replacement = false;
                        child.program(body)
                    }
                    EventChange::ReplaceTokens { tokens, .. } => tokens
                        .iter()
                        .try_for_each(|token| self.count(&Count::Exactly(token.amount.clone()))),
                }
            }
            Operation::CreateTrigger {
                event,
                duration,
                announcement,
                body,
                intervening_if,
                ..
            } => {
                self.duration(duration)?;
                self.event(event)?;
                let mut child = self.clone();
                child.replacement = false;
                child.open_searches.clear();
                child.join_groups.clear();
                child.event = crate::semantics::event_bindings(event);
                intervening_if
                    .iter()
                    .try_for_each(|condition| child.condition(condition))?;
                child.announcement(announcement)?;
                child.program(body)
            }
            Operation::BindValue { value } => self.expr(value),
            Operation::RepeatProcess => {
                if self.repeatable {
                    Ok(())
                } else {
                    Err("nothing to repeat in this scope".into())
                }
            }
            Operation::DiscardObjects { objects } => self.objects(objects),
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn operation_case_10(&self, operation: &Operation) -> Result {
        match operation {
            Operation::ReflexiveTrigger {
                performed,
                condition,
                announcement,
                body,
            } => {
                if performed
                    .as_ref()
                    .is_some_and(|performed| !self.results.contains_key(&performed.0))
                {
                    return Err("a reflexive trigger reads an action that has not run".into());
                }
                if let Some(condition) = condition {
                    self.condition(condition)?;
                }
                let mut child = self.clone();
                child.replacement = false;
                child.open_searches.clear();
                child.join_groups.clear();
                child.announcement(announcement)?;
                child.program(body)
            }
            Operation::ReturnAfter { objects, duration } => {
                self.duration(duration)?;
                self.objects(objects)
            }
            Operation::CreateReplacement {
                event,
                limit,
                duration,
                body,
            } => {
                self.duration(duration)?;
                if let ReplacementLimit::TotalDamage(amount)
                | ReplacementLimit::DamageToEach(amount) = limit
                {
                    self.count(&Count::Exactly(amount.clone()))?;
                }
                self.replacement_event(event)?;
                let mut child = self.clone();
                child.replacement = true;
                child.open_searches.clear();
                child.join_groups.clear();
                child.event = crate::semantics::replacement_bindings(event);
                child.program(body)
            }
            Operation::PayCost { player, cost, .. } => {
                self.player(player)?;
                self.cost(cost)
            }
            Operation::PayRepeatedly {
                player,
                cost,
                at_most,
            } => {
                self.player(player)?;
                self.cost(cost)?;
                self.expr(at_most)
            }
            Operation::RemoveCounters { removals } => self.removals(removals),
            Operation::Attach { attachments, host } => {
                self.require_zone(attachments, EventZone::Battlefield)?;
                self.require_zone(&ObjectSet::One(host.clone()), EventZone::Battlefield)
            }
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn operation_case_11(&self, operation: &Operation) -> Result {
        match operation {
            Operation::CreateTokens {
                players,
                tokens,
                entry,
            } => {
                self.players(players)?;
                if tokens.is_empty() {
                    return Err("empty token creation batch".into());
                }
                for (i, token) in tokens.iter().enumerate() {
                    self.count(&Count::Exactly(token.amount.clone()))?;
                    let definition = match &token.recipe {
                        TokenRecipe::Defined(definition) => definition,
                        TokenRecipe::Copy { object, exceptions } => {
                            self.object(object)?;
                            self.copy_exceptions(exceptions)?;
                            continue;
                        }
                        TokenRecipe::CopyEach {
                            objects,
                            exceptions,
                        }
                        | TokenRecipe::CopyOne {
                            objects,
                            exceptions,
                        } => {
                            self.objects(objects)?;
                            self.copy_exceptions(exceptions)?;
                            continue;
                        }
                        TokenRecipe::Named(name) if name.is_empty() => {
                            return Err("named token recipe is empty".into());
                        }
                        TokenRecipe::CopyNamed { name, .. } if name.is_empty() => {
                            return Err("named token recipe is empty".into());
                        }
                        TokenRecipe::CopyNamed { exceptions, .. } => {
                            self.copy_exceptions(exceptions)?;
                            continue;
                        }
                        TokenRecipe::CopyAtRandom { cards } => {
                            self.operand(cards)?;
                            continue;
                        }
                        TokenRecipe::Named(_) => continue,
                    };
                    if matches!(&definition.name, TokenName::Explicit(name) if name.is_empty()) {
                        return Err("explicit token name is empty".into());
                    }
                    if definition
                        .colors
                        .iter()
                        .any(|c| matches!(c, ManaColor::Colorless))
                    {
                        return Err("colorless is not a token color".into());
                    }
                    if let Some((objects, kinds)) = &definition.abilities_of {
                        self.objects(objects)?;
                        self.operand(kinds)?;
                    }
                    if let Some((power, toughness)) = &definition.power_toughness {
                        self.expr(power)?;
                        self.expr(toughness)?;
                    }
                    for (j, ability) in definition.abilities.iter().enumerate() {
                        validate_ability(ability)
                            .map_err(|e| format!("tokens[{i}].definition.abilities[{j}]: {e}"))?;
                    }
                }
                self.entry(entry)?;
                arrival::battlefield(entry)?;
                Ok(())
            }
            Operation::SeparatePiles {
                separator,
                chooser,
                objects,
            } => {
                self.player(separator)?;
                self.player(chooser)?;
                self.objects(objects)
            }
            Operation::ChooseObjects {
                player,
                objects,
                count,
                constraints,
            } => {
                self.player(player)?;
                self.objects(objects)?;
                self.count(count)?;
                self.selection_constraints(constraints)
            }
            Operation::PutCounters { placements } => {
                if placements.is_empty() {
                    return Err("empty counter placement batch".into());
                }
                for placement in placements {
                    self.recipients(&placement.recipients)?;
                    self.count(&Count::Exactly(placement.amount.clone()))?;
                    if let Some(selection) = &placement.selection {
                        self.player(&selection.player)?;
                        self.count(&selection.count)?;
                    }
                }
                Ok(())
            }
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn operation_case_12(&self, operation: &Operation) -> Result {
        match operation {
            Operation::Draw { players, amount }
            | Operation::Discard {
                players, amount, ..
            }
            | Operation::SetLifeTotal { players, amount }
            | Operation::GainLife { players, amount }
            | Operation::LoseLife { players, amount }
            | Operation::Mill { players, amount } => {
                self.players(players)?;
                self.expr(amount)
            }
            Operation::AddMana {
                players,
                mana,
                restriction,
            } => {
                self.players(players)?;
                self.mana(mana)?;
                if let Some(r) = restriction {
                    self.mana_use(r)?;
                }
                Ok(())
            }
            Operation::DealDamage {
                source,
                recipients,
                amount,
                ..
            } => {
                self.object(source)?;
                self.recipients(recipients)?;
                self.expr(amount)
            }
            Operation::DealDamageWith {
                source,
                recipients,
                amount,
                rider,
            } => {
                self.object(source)?;
                self.recipients(recipients)?;
                self.expr(amount)?;
                match rider {
                    DamageRider::DividedEvenly(_) => Ok(()),
                    DamageRider::ExcessTo(_) if matches!(recipients, Recipients::Players(_)) => {
                        Err("a player has no lethal damage to exceed".into())
                    }
                    DamageRider::ExcessTo(player) | DamageRider::DividedBy(player) => {
                        self.player(player)
                    }
                    DamageRider::Prevented(prevented) => self.expr(prevented),
                }
            }
            Operation::DealDamageParts { source, parts, .. } => {
                self.object(source)?;
                for part in parts {
                    self.recipients(&part.recipients)?;
                    self.expr(&part.amount)?;
                }
                Ok(())
            }
            Operation::DealDamageFromEach {
                sources,
                recipients,
                amount,
            } => {
                self.require_zone(sources, EventZone::Battlefield)?;
                let mut per_object = self.clone();
                per_object.affected = true;
                per_object.recipients(recipients)?;
                per_object.expr(amount)
            }
            Operation::Destroy { objects, .. } | Operation::SetTapped { objects, .. } => {
                self.require_zone(objects, EventZone::Battlefield)
            }
            Operation::Sacrifice {
                players,
                objects,
                choose,
            } => {
                self.players(players)?;
                self.require_zone(objects, EventZone::Battlefield)?;
                if let Some(count) = choose {
                    self.count(count)?;
                }
                Ok(())
            }
            Operation::CreateContinuousEffect {
                source,
                affected,
                changes,
                duration,
                evaluation,
                ..
            } => {
                let mut per_object = self.clone();
                per_object.affected = matches!(affected, EffectSubjects::Objects(_));
                per_object.duration(duration)?;
                let rule = |c: &Change| matches!(c, Change::Rule(_) | Change::PlayerRule(_));
                let has_rules = changes.iter().any(rule);
                if has_rules && changes.iter().any(|c| !rule(c)) {
                    return Err("mixed rule and characteristic changes require independently evaluated parts".into());
                }
                if has_rules != matches!(evaluation, EffectEvaluation::LiveObjects) {
                    return Err("rule changes require live subject queries with captured resolution variables".into());
                }
                self.object(source)?;
                self.subject_changes(affected, changes)?;
                Ok(())
            }
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn selection_constraints(&self, constraints: &[SelectionConstraint]) -> Result {
        for constraint in constraints {
            match constraint {
                SelectionConstraint::OneForEachCardType(CardTypeDomain::AmongObjects(objects)) => {
                    self.objects(objects)?;
                }
                SelectionConstraint::OneForEachCardType(CardTypeDomain::AmongCasts {
                    by,
                    filter,
                    ..
                }) => {
                    self.players(by)?;
                    self.filter(filter)?;
                }
                SelectionConstraint::Primitive { name, operands } => {
                    self.primitive(name, operands)?;
                }
                _ => {}
            }
            if let SelectionConstraint::TotalProperty { amount, .. }
            | SelectionConstraint::DistinctCount { amount, .. }
            | SelectionConstraint::TotalManaSymbols { amount, .. } = constraint
            {
                self.expr(amount)?;
            }
            if let SelectionConstraint::NamedAfterOneEach(objects) = constraint {
                self.objects(objects)?;
            }
            if let SelectionConstraint::OneForEach(filters) = constraint {
                for filter in filters {
                    self.filter(filter)?;
                }
            }
            if let SelectionConstraint::CanMoveTo { destination } = constraint {
                let view = crate::movement::entry_destination_view(destination);
                let (destination, entry) = (&view.destination, &view.entry);
                if let MovementPlacement::By(players) = &view.terms.placement {
                    self.players(players)?;
                }
                self.destination(destination)?;
                self.entry(entry)?;
                arrival::destination(entry, destination)?;
            }
        }
        Ok(())
    }

    pub(super) fn mana(&self, mana: &ManaProduction) -> Result {
        match mana {
            ManaProduction::SameTypeAsProduced
            | ManaProduction::CommanderIdentity
            | ManaProduction::DifferentColors(_)
            | ManaProduction::OtherColor
            | ManaProduction::LastNoted { .. } => Ok(()),
            ManaProduction::DraftNotedColor(note) => self.player(&note.player),
            ManaProduction::ColorsAmong { objects, .. } => self.objects(objects),
            ManaProduction::ManaCostOf(object) | ManaProduction::SymbolsOf(object) => {
                self.object(object)
            }
            ManaProduction::LostThisWay(result) if self.results.contains_key(&result.0) => Ok(()),
            ManaProduction::LostThisWay(result) => {
                Err(format!("result {} is unavailable", result.0))
            }
            ManaProduction::CouldProduce { objects, .. } => self.objects(objects),
            ManaProduction::Primitive { name, operands } => self.primitive(name, operands),
            ManaProduction::Amount { amount, .. } => self.count(&Count::Exactly(amount.clone())),
            ManaProduction::All(v) | ManaProduction::ChooseOne(v) => {
                if v.is_empty() {
                    return Err("empty mana production expression".into());
                }
                for p in v {
                    self.mana(p)?;
                }
                Ok(())
            }
            ManaProduction::Repeat {
                amount, production, ..
            } => {
                self.count(&Count::Exactly(amount.clone()))?;
                self.mana(production)
            }
        }
    }

    pub(super) fn mana_use(&self, r: &ManaUse) -> Result {
        match r {
            ManaUse::CastSpell(f) | ManaUse::ActivateAbility { sources: f, .. } => self.filter(f),
            ManaUse::All(v) | ManaUse::Either(v) => {
                for r in v {
                    self.mana_use(r)?;
                }
                Ok(())
            }
            ManaUse::Not(r) => self.mana_use(r),
            ManaUse::Persists(until) => self.duration(until),
            ManaUse::SpentOn {
                spells,
                changes,
                until,
            } => {
                self.filter(spells)?;
                changes.iter().try_for_each(|c| self.change(c))?;
                self.duration(until)
            }
            ManaUse::Purpose(ManaPurpose::CostContaining(symbol)) => self.operand(symbol),
            ManaUse::Purpose(_) => Ok(()),
            ManaUse::WhenSpent { spells, body, .. } => {
                self.filter(spells)?;
                let mut scoped = self.clone();
                scoped.event = crate::semantics::event_bindings(&EventSpec::Spell {
                    event: SpellEvent::Cast,
                    players: PlayerSet::All,
                    spells: spells.clone(),
                });
                scoped.event.stack_object = true;
                scoped.operand(body)
            }
            ManaUse::SpentGives { spells, effects } => {
                self.filter(spells)?;
                effects.iter().try_for_each(|effect| self.operand(effect))
            }
            ManaUse::Primitive { name, operands } => self.primitive(name, operands),
        }
    }

    /// A move in a cost or an optional body stays readable; it reads nothing
    /// if that body never ran.
    pub(super) fn adopt_moves(&mut self, body: &Program) {
        for instruction in &body.instructions {
            match instruction {
                Instruction::Effect { result, operation } => {
                    let operation = operation.action();
                    if crate::semantics::singular_operation(operation, |r| {
                        self.singular.contains(&r.0)
                    }) {
                        self.singular.insert(result.0);
                    }
                    if matches!(
                        operation,
                        Operation::Move { .. }
                            | Operation::Destroy { .. }
                            | Operation::Sacrifice { .. }
                            | Operation::Discard { .. }
                            | Operation::DiscardObjects { .. }
                            | Operation::DiscardAllBut { .. }
                            | Operation::Mill { .. }
                    ) {
                        self.results.insert(result.0, ResultKind::ObjectTransition);
                    }
                    if matches!(operation, Operation::CreateTokens { .. }) {
                        self.results.insert(result.0, ResultKind::CreatedObjects);
                    }
                    if let Operation::ChooseObjects { objects, .. } = operation {
                        self.results.insert(result.0, ResultKind::Selection);
                        if let Some(zones) = self.known_zones(objects) {
                            self.result_zones.insert(result.0, zones);
                        }
                    }
                }
                Instruction::Group(body) | Instruction::Optional { body, .. } => {
                    self.adopt_moves(body)
                }
                _ => {}
            }
        }
    }
}
