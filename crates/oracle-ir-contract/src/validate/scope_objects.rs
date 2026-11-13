//! Scoped validation of portable operands.
use super::*;
impl<'a> Scope<'a> {
    pub(super) fn objects(&self, objects: &ObjectSet) -> Result {
        match objects {
            ObjectSet::Prepared(binding) => {
                if self.prepared.contains_key(&binding.0) {
                    Ok(())
                } else {
                    Err(format!(
                        "selection {} is outside its preparation scope",
                        binding.0
                    ))
                }
            }
            ObjectSet::Scoped => Ok(()),
            ObjectSet::Earlier { by, .. } => {
                by.iter().try_for_each(|players| self.players(players))
            }
            ObjectSet::Recorded(result) if self.results.contains_key(&result.0) => Ok(()),
            ObjectSet::Recorded(result) => Err(format!("result {} is unavailable", result.0)),
            ObjectSet::DamageSources(filter)
            | ObjectSet::AnyZone(filter)
            | ObjectSet::OutsideTheGame(filter) => self.filter(filter),
            ObjectSet::LibraryTops(players) => self.players(players),
            ObjectSet::SplicedOnto(object) | ObjectSet::TargetsOf(object) => self.object(object),
            ObjectSet::FromHistory { event, .. } => self.event(event),
            ObjectSet::DraftRandom { .. } | ObjectSet::DraftChosen { .. } => {
                Err("a draft selector has no canonical choice at this execution site".into())
            }
            ObjectSet::CostObjects {
                payment,
                component,
                view,
                phase,
            } => {
                self.paid_component(payment, component)?;
                if self.announcing {
                    let known = matches!(
                        (payment, view, phase),
                        (
                            PaymentRef::Activation
                                | PaymentRef::Casting { .. }
                                | PaymentRef::Branch,
                            CostObjectView::Inputs,
                            ObjectPhase::Before
                        )
                    );
                    if !known {
                        return Err("payment history is unavailable during announcement".into());
                    }
                }
                if matches!(view, CostObjectView::Inputs) && !matches!(phase, ObjectPhase::Before) {
                    return Err("payment input history requires original identities".into());
                }
                Ok(())
            }
            ObjectSet::CreationLinkedCards(source) => self.object(source),
            ObjectSet::Observed(result) => match self.results.get(&result.0) {
                Some(ResultKind::Observation) => Ok(()),
                _ => Err("observation requires an available observation result".into()),
            },
            ObjectSet::Matching { objects, filter } => {
                self.objects(objects)?;
                self.filter(filter)
            }
            ObjectSet::Difference { objects, excluding } => {
                self.objects(objects)?;
                self.objects(excluding)
            }
            ObjectSet::Damage(DamageSubject::Source) if self.event.damage => Ok(()),
            ObjectSet::Damage(DamageSubject::Recipient) if self.event.damage_objects => Ok(()),
            ObjectSet::EventGroup if self.event.group => Ok(()),
            ObjectSet::EventGroup => Err("this event groups no objects".into()),
            ObjectSet::Damage(_) => Err("damage event does not bind this object collection".into()),
            ObjectSet::Zone {
                scope,
                kind,
                filter,
            } => {
                self.zone_scope(scope)?;
                let zone = match scope {
                    ZoneScope::Shared(z) | ZoneScope::Players { zone: z, .. } => z.clone(),
                };
                self.domain(&ObjectDomain {
                    kind: kind.clone(),
                    zones: vec![zone],
                })?;
                self.filter(filter)
            }
            ObjectSet::Union(sets) => {
                for set in sets {
                    self.objects(set)?;
                }
                Ok(())
            }
            ObjectSet::Take { objects, count, .. } => {
                self.objects(objects)?;
                if !self.ordered(objects) {
                    return Err("top/bottom selection requires one ordered zone".into());
                }
                self.count(&Count::Exactly(count.clone()))
            }
            ObjectSet::Transition { result, .. } => match self.results.get(&result.0) {
                Some(ResultKind::ObjectTransition) => Ok(()),
                _ => Err("transition set requires an object-transition result".into()),
            },
            ObjectSet::MovedBy { result, player, .. } => {
                self.player(player)?;
                match self.results.get(&result.0) {
                    Some(ResultKind::ObjectTransition) => Ok(()),
                    _ => Err("a player's share requires an object-transition result".into()),
                }
            }
            ObjectSet::Primitive { name, operands } => self.primitive(name, operands),
            ObjectSet::One(object) => self.object(object),
            // A loop over "any target" binds each one, player or permanent.
            ObjectSet::Targets(slot) => self
                .target(slot, None, TargetKind::Object(ObjectDomain::permanent()))
                .or_else(|_| self.target(slot, None, TargetKind::DamageRecipient)),
            ObjectSet::InZone { objects, .. } => self.objects(objects),
            ObjectSet::Selected(result) | ObjectSet::Unselected(result) => {
                match self.results.get(&result.0) {
                    Some(ResultKind::Selection | ResultKind::Search) => Ok(()),
                    _ => Err(format!(
                        "result {} is not an available object selection",
                        result.0
                    )),
                }
            }
            ObjectSet::Affected(result) if self.results.contains_key(&result.0) => Ok(()),
            ObjectSet::Affected(result) => Err(format!("result {} is unavailable", result.0)),
            ObjectSet::Copies(result) | ObjectSet::Unpaid(result)
                if self.results.contains_key(&result.0) =>
            {
                Ok(())
            }
            ObjectSet::Copies(result) | ObjectSet::Unpaid(result) => {
                Err(format!("result {} is unavailable", result.0))
            }
            ObjectSet::Created(result) => match self.results.get(&result.0) {
                Some(ResultKind::CreatedObjects) => Ok(()),
                _ => Err(format!(
                    "result {} is not an available token-creation result",
                    result.0
                )),
            },
        }
    }

    pub(super) fn recipients(&self, recipients: &Recipients) -> Result {
        match recipients {
            Recipients::DamagedByEvent if self.event.damage_any_recipient => Ok(()),
            Recipients::DamagedByEvent => Err("this event names no one damaged thing".into()),
            Recipients::Objects(objects) => self.objects(objects),
            Recipients::Players(players) => self.players(players),
            Recipients::Targets(slot) | Recipients::Target { slot, .. } => {
                self.target(slot, None, TargetKind::DamageRecipient)
            }
            Recipients::Chosen => Ok(()),
            Recipients::AttackedBy(attacker) => self.object(attacker),
            Recipients::Union(items) => {
                for item in items {
                    self.recipients(item)?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn vote_option(&self, option: &VoteOption) -> Result {
        match option {
            VoteOption::Word(_) => Ok(()),
            VoteOption::Player(player) => self.player(player),
            VoteOption::Object(object) => self.object(object),
        }
    }

    pub(super) fn player_number(&self, number: &PlayerNumber) -> Result {
        match number {
            PlayerNumber::Permanents(filter) => self.filter(filter),
            _ => Ok(()),
        }
    }

    #[inline]
    pub(super) fn expr(&self, expression: &Expr) -> Result {
        match expression {
            Expr::DamageBy(_) | Expr::ModesChosen(_) => self.expr_case_0(expression),
            Expr::AdditionalCostPaid(_) => self.expr_case_0(expression),
            Expr::DraftNoted { .. } => self.expr_case_0(expression),
            Expr::TurnsSinceForetold | Expr::ActivationCostSymbols(_) => {
                self.expr_case_0(expression)
            }
            Expr::ChosenColorsOf(_) => self.expr_case_0(expression),
            Expr::AbilitiesAmong { .. } => self.expr_case_0(expression),
            Expr::Earlier { .. } => self.expr_case_0(expression),
            Expr::Dice(DiceMeasure::Matching(_, _)) => self.expr_case_0(expression),
            Expr::Dice(_) | Expr::Game(_) => self.expr_case_0(expression),
            Expr::Stickers { .. } => self.expr_case_0(expression),
            Expr::Noted { .. } => self.expr_case_0(expression),
            Expr::Difference(_, _) => self.expr_case_0(expression),
            Expr::LargestSharedGroup { .. } | Expr::SharingWithAnother { .. } => {
                self.expr_case_0(expression)
            }
            Expr::ManaSpentFrom {
                object: _,
                sources: _,
            } => self.expr_case_0(expression),
            Expr::AsOf { .. } => self.expr_case_0(expression),
            Expr::Exponentiate(_, _) => self.expr_case_0(expression),
            Expr::TimesActivatedThisTurn | Expr::TurnNumber => self.expr_case_0(expression),
            Expr::Primitive {
                name: _,
                operands: _,
            } => self.expr_case_0(expression),
            Expr::ChosenParity | Expr::ChosenNumber | Expr::TimesResolvedThisTurn => {
                self.expr_case_0(expression)
            }
            Expr::PartySize(_) | Expr::StartingLife(_) => self.expr_case_0(expression),
            Expr::Votes(_) => self.expr_case_0(expression),
            Expr::PlayerNumber {
                player: _,
                number: _,
            } => self.expr_case_0(expression),
            Expr::AmongPlayers { .. } => self.expr_case_0(expression),
            Expr::TimesKicked(_) => self.expr_case_0(expression),
            Expr::TimesMutated(_) => self.expr_case_0(expression),
            Expr::ManaOf { .. } => self.expr_case_0(expression),
            Expr::CountersPaid { .. } => self.expr_case_0(expression),
            Expr::SharedValues { .. } => self.expr_case_0(expression),
            Expr::RepeatedCostsUnpaid {
                result: _,
                player: _,
            } => self.expr_case_0(expression),
            Expr::EventHistory { .. } => self.expr_case_0(expression),
            Expr::CountPlayers(_) => self.expr_case_0(expression),
            Expr::ManaSpent { .. } => self.expr_case_0(expression),
            Expr::CountManaSymbols {
                objects: _,
                colors: _,
            } => self.expr_case_0(expression),
            Expr::BoundValue(_) | Expr::CountersRemoved(_) => self.expr_case_0(expression),
            Expr::AnnouncedX if self.definition => self.expr_case_0(expression),
            Expr::Aggregate { .. } | Expr::CountDistinct { .. } => self.expr_case_0(expression),
            Expr::EventActivationX if self.event.activation_x => self.expr_case_0(expression),
            Expr::EventActivationX => self.expr_case_0(expression),
            _ => self.expr_dispatch_1(expression),
        }
    }

    #[inline]
    pub(super) fn expr_dispatch_1(&self, expression: &Expr) -> Result {
        match expression {
            Expr::Constant(_) | Expr::AnnouncedX => self.expr_case_1(expression),
            Expr::CountCoinOutcomes {
                source: CoinCountSource::Latest,
                ..
            } => self.expr_case_1(expression),
            Expr::CountCoinOutcomes {
                source: CoinCountSource::Receipt(_),
                ..
            } => self.expr_case_1(expression),
            Expr::DieResult(_) => self.expr_case_1(expression),
            Expr::DistributedShare(_) => self.expr_case_1(expression),
            Expr::ExcessDamage { result, to: _ } if self.results.contains_key(&result.0) => {
                self.expr_case_1(expression)
            }
            Expr::ExcessDamage { .. } => self.expr_case_1(expression),
            Expr::SurveilKeptOnTop(_) => self.expr_case_1(expression),
            Expr::Performed(result)
            | Expr::PerformedTotal(result)
            | Expr::ClampedDamage(result)
            | Expr::ChosenPowerToughness { result, .. }
            | Expr::DieResultAt { result, .. }
                if self.results.contains_key(&result.0) =>
            {
                self.expr_case_1(expression)
            }
            Expr::ChosenPowerToughness { .. } | Expr::DieResultAt { .. } => {
                self.expr_case_1(expression)
            }
            Expr::Performed(_) | Expr::PerformedTotal(_) | Expr::ClampedDamage(_) => {
                self.expr_case_1(expression)
            }
            Expr::RequestedDrawCount {
                payment: _,
                component: _,
            } => self.expr_case_1(expression),
            Expr::PaidCountersRemoved {
                payment: _,
                component: _,
            } => self.expr_case_1(expression),
            Expr::EventAmount if self.event.amount => self.expr_case_1(expression),
            Expr::EventAmount => self.expr_case_1(expression),
            Expr::ObjectProperty { .. } => self.expr_case_1(expression),
            Expr::Add(_, _)
            | Expr::Subtract(_, _)
            | Expr::Multiply(_, _)
            | Expr::Minimum(_, _)
            | Expr::Maximum(_, _) => self.expr_case_1(expression),
            Expr::Conditional {
                condition: _,
                then: _,
                otherwise: _,
            } => self.expr_case_1(expression),
            Expr::Divide { .. } => self.expr_case_1(expression),
            Expr::CountObjects(_) => self.expr_case_1(expression),
            _ => self.expr_dispatch_2(expression),
        }
    }

    #[inline]
    pub(super) fn expr_dispatch_2(&self, expression: &Expr) -> Result {
        match expression {
            Expr::CountCounters { .. } | Expr::CastX(_) => self.expr_case_2(expression),
            _ => unreachable!("all current variants were dispatched"),
        }
    }

    pub(super) fn expr_case_0(&self, expression: &Expr) -> Result {
        match expression {
            Expr::DamageBy(object) | Expr::ModesChosen(object) => self.object(object),
            Expr::AdditionalCostPaid(symbols) => {
                symbols.iter().try_for_each(|symbol| self.operand(symbol))
            }
            Expr::DraftNoted { note, .. } => self.player(&note.player),
            Expr::TurnsSinceForetold | Expr::ActivationCostSymbols(_) => Ok(()),
            Expr::ChosenColorsOf(object) => self.object(object),
            Expr::AbilitiesAmong { objects, .. } => self.objects(objects),
            Expr::Earlier {
                result, filter, by, ..
            } => {
                if let Some(result) = result
                    && !self.results.contains_key(&result.0)
                {
                    return Err(format!("result {} has not run", result.0));
                }
                filter.iter().try_for_each(|f| self.filter(f))?;
                by.iter().try_for_each(|p| self.player(p))
            }
            Expr::Dice(DiceMeasure::Matching(_, value)) => self.expr(value),
            Expr::Dice(_) | Expr::Game(_) => Ok(()),
            Expr::Stickers { objects, .. } => self.objects(objects),
            Expr::Noted { object, .. } => object.iter().try_for_each(|o| self.object(o)),
            Expr::Difference(a, b) => {
                self.expr(a)?;
                self.expr(b)
            }
            Expr::LargestSharedGroup { objects, .. } | Expr::SharingWithAnother { objects, .. } => {
                self.objects(objects)
            }
            Expr::ManaSpentFrom { object, sources } => {
                self.object(object)?;
                self.filter(sources)
            }
            Expr::AsOf { value, .. } => self.expr(value),
            Expr::Exponentiate(base, exponent) => {
                self.expr(base)?;
                self.expr(exponent)
            }
            Expr::TimesActivatedThisTurn | Expr::TurnNumber => Ok(()),
            Expr::Primitive { name, operands } => self.primitive(name, operands),
            Expr::ChosenParity | Expr::ChosenNumber | Expr::TimesResolvedThisTurn => Ok(()),
            Expr::PartySize(player) | Expr::StartingLife(player) => self.player(player),
            Expr::Votes(option) => self.vote_option(option),
            Expr::PlayerNumber { player, number } => {
                self.player(player)?;
                self.player_number(number)
            }
            Expr::AmongPlayers {
                players, number, ..
            } => {
                self.players(players)?;
                self.player_number(number)
            }
            Expr::TimesKicked(object) => self.object(object),
            Expr::TimesMutated(object) => self.object(object),
            Expr::ManaOf { filter, .. } => {
                if filter.colors.as_ref().is_some_and(Vec::is_empty) {
                    return Err("spent-mana type filter cannot be empty".into());
                }
                Ok(())
            }
            Expr::CountersPaid { object, .. } => self.object(object),
            Expr::SharedValues { object, with, .. } => {
                self.object(object)?;
                self.objects(with)
            }
            Expr::RepeatedCostsUnpaid { result, player } => {
                if !self.results.contains_key(&result.0) {
                    return Err(format!("result {} is unavailable", result.0));
                }
                self.player(player)
            }
            Expr::EventHistory { event, .. } => self.event(event),
            Expr::CountPlayers(players) => self.players(players),
            Expr::ManaSpent { object, filter, .. } => {
                self.object(object)?;
                if let Some(colors) = &filter.colors {
                    if colors.is_empty() {
                        return Err("spent-mana type filter cannot be empty".into());
                    }
                    for (i, color) in colors.iter().enumerate() {
                        if colors[..i].contains(color) {
                            return Err("spent-mana type filter repeats a type".into());
                        }
                    }
                }
                Ok(())
            }
            Expr::CountManaSymbols { objects, colors } => {
                if colors.is_empty() || colors.iter().any(|c| matches!(c, ManaColor::Colorless)) {
                    return Err("mana symbol color query requires actual colors".into());
                }
                self.objects(objects)
            }
            Expr::BoundValue(result) | Expr::CountersRemoved(result) => {
                if self.results.contains_key(&result.0) {
                    Ok(())
                } else {
                    Err("a bound value is read before it is set".into())
                }
            }
            Expr::AnnouncedX if self.definition => {
                Err("characteristic definition cannot use an unrelated announced X".into())
            }
            Expr::Aggregate { objects, .. } | Expr::CountDistinct { objects, .. } => {
                self.objects(objects)
            }
            Expr::EventActivationX if self.event.activation_x => Ok(()),
            Expr::EventActivationX => Err("event activation X used outside a cycling event".into()),
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn expr_case_1(&self, expression: &Expr) -> Result {
        match expression {
            Expr::Constant(_) | Expr::AnnouncedX => Ok(()),
            Expr::CountCoinOutcomes {
                source: CoinCountSource::Latest,
                ..
            } => Ok(()),
            Expr::CountCoinOutcomes {
                source: CoinCountSource::Receipt(result),
                ..
            } => match self.results.get(&result.0) {
                Some(ResultKind::Coin | ResultKind::CoinBatch) => Ok(()),
                _ => Err(format!(
                    "result {} flipped no coins in this scope",
                    result.0
                )),
            },
            Expr::DieResult(result) => {
                if self.results.contains_key(&result.0) {
                    Ok(())
                } else {
                    Err(format!("result {} rolled no die in this scope", result.0))
                }
            }
            Expr::DistributedShare(slot) => match self.slots.get(&slot.0) {
                Some(target) if target.distribute.is_some() => Ok(()),
                _ => Err(format!("target slot {} announces no distribution", slot.0)),
            },
            Expr::ExcessDamage { result, to } if self.results.contains_key(&result.0) => {
                self.filter(to)
            }
            Expr::ExcessDamage { result, .. } => Err(format!("result {} is unavailable", result.0)),
            Expr::SurveilKeptOnTop(result) => match self.results.get(&result.0) {
                Some(ResultKind::Surveil) => Ok(()),
                _ => Err(format!("result {} did not surveil in this scope", result.0)),
            },
            Expr::Performed(result)
            | Expr::PerformedTotal(result)
            | Expr::ClampedDamage(result)
            | Expr::ChosenPowerToughness { result, .. }
            | Expr::DieResultAt { result, .. }
                if self.results.contains_key(&result.0) =>
            {
                Ok(())
            }
            Expr::ChosenPowerToughness { result, .. } | Expr::DieResultAt { result, .. } => {
                Err(format!("result {} is unavailable", result.0))
            }
            Expr::Performed(result)
            | Expr::PerformedTotal(result)
            | Expr::ClampedDamage(result) => Err(format!("result {} is unavailable", result.0)),
            Expr::RequestedDrawCount { payment, component } => {
                if self.announcing {
                    return Err("requested draw history is unavailable during announcement".into());
                }
                match self.paid_component(payment, component)? {
                    CostPart::Draw { .. } => Ok(()),
                    _ => Err("requested draw count requires a draw cost component".into()),
                }
            }
            Expr::PaidCountersRemoved { payment, component } => {
                self.paid_component(payment, component).map(|_| ())
            }
            Expr::EventAmount if self.event.amount => Ok(()),
            Expr::EventAmount => Err("event amount used outside a damage event".into()),
            Expr::ObjectProperty {
                object,
                information,
                ..
            } => {
                if matches!(information, Information::Captured(_)) {
                    return Err("captured information has no binding declaration".into());
                }
                self.object(object)
            }
            Expr::Add(a, b)
            | Expr::Subtract(a, b)
            | Expr::Multiply(a, b)
            | Expr::Minimum(a, b)
            | Expr::Maximum(a, b) => {
                self.expr(a)?;
                self.expr(b)
            }
            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => {
                self.condition(condition)?;
                self.expr(then)?;
                self.expr(otherwise)
            }
            Expr::Divide {
                numerator,
                denominator,
                ..
            } => {
                if matches!(denominator.as_ref(), Expr::Constant(0)) {
                    return Err("division by zero".into());
                }
                self.expr(numerator)?;
                self.expr(denominator)
            }
            Expr::CountObjects(objects) => self.objects(objects),
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn expr_case_2(&self, expression: &Expr) -> Result {
        match expression {
            Expr::CountCounters {
                recipients,
                information,
                ..
            } => {
                if matches!(information, Information::Captured(_)) {
                    return Err("captured counter information has no binding declaration".into());
                }
                self.recipients(recipients)
            }
            Expr::CastX(object) => self.object(object),
            _ => unreachable!("validated dispatch"),
        }
    }

    pub(super) fn count(&self, count: &Count) -> Result {
        let value = match count {
            Count::Exactly(n) | Count::UpTo(n) | Count::AtLeast(n) | Count::AsManyAsCan(n) => n,
            Count::AnyNumber => return Ok(()),
            Count::Between { minimum, maximum } => {
                bounds::interval(minimum, maximum)?;
                self.expr(minimum)?;
                maximum
            }
            Count::PerPlayer { players, each } => {
                self.players(players)?;
                return self.count(each);
            }
        };
        bounds::nonnegative(value)?;
        self.expr(value)
    }

    pub(super) fn filter(&self, filter: &ObjectFilter) -> Result {
        let mut scope = self.clone();
        scope.candidate = true;
        scope.filter_inner(filter)
    }
}
