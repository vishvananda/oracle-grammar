//! Scoped validation of portable operands.
use super::*;
impl<'a> Scope<'a> {
    pub(super) fn paid_component(&self, payment: &PaymentRef, path: &[u32]) -> Result<&CostPart> {
        let cost = match payment {
            PaymentRef::Activation => self.activation_cost.as_ref(),
            PaymentRef::Casting { ability } => self.casting_costs.get(ability),
            PaymentRef::Resolution(result) => self.payment_costs.get(&result.0),
            PaymentRef::Branch => self.casting_costs.get(&BRANCH_COST),
        }
        .ok_or("payment history has no declaration in this scope")?;
        crate::cost_history::component(cost, path)
            .ok_or("payment history has an invalid component path".into())
    }

    pub(super) fn at_most_one(&self, objects: &ObjectSet) -> bool {
        match objects {
            ObjectSet::One(_) => true,
            ObjectSet::Targets(slot) => self.slots.get(&slot.0).is_some_and(|spec| {
                matches!(
                    spec.count,
                    Count::Exactly(Expr::Constant(1))
                        | Count::UpTo(Expr::Constant(1))
                        | Count::AsManyAsCan(Expr::Constant(1))
                )
            }),
            ObjectSet::InZone { objects, .. }
            | ObjectSet::Matching { objects, .. }
            | ObjectSet::Difference { objects, .. } => self.at_most_one(objects),
            _ => false,
        }
    }

    pub(super) fn singular_history(&self, objects: &ObjectSet, index: u32) -> bool {
        // "The second card exiled this way": what was performed keeps its order.
        if let ObjectSet::CostObjects {
            payment,
            component,
            view: CostObjectView::Performed,
            phase: ObjectPhase::After,
        } = objects
        {
            let count = self
                .paid_component(payment, component)
                .ok()
                .and_then(crate::cost_history::exact_count);
            if let Some(count) = count {
                return i64::from(index) < count;
            }
        }
        if index != 0 {
            return false;
        }
        // An optional target population can contain no object, but never
        // more than one. Index zero therefore needs no ordering, including
        // through a zone or filtering qualification. Missing selects nothing.
        if self.at_most_one(objects) {
            return true;
        }
        // "The target card in that player's graveyard": one per owner.
        if let ObjectSet::Matching {
            objects,
            filter: ObjectFilter::OwnedBy(PlayerSet::One(_)),
        } = objects
            && matches!(**objects, ObjectSet::Targets(_))
        {
            return true;
        }
        // "The exiled card": one card held by a linked ability (607.2a).
        if matches!(
            objects,
            ObjectSet::Zone {
                filter: ObjectFilter::ExiledWith(ObjectRef::Source),
                ..
            }
        ) {
            return true;
        }
        if let ObjectSet::Transition { result, .. } | ObjectSet::Observed(result) = objects {
            return self.singular.contains(&result.0);
        }
        // "The spell cast this way".
        if matches!(objects, ObjectSet::Affected(_)) {
            return true;
        }
        let ObjectSet::CostObjects {
            payment,
            component,
            view,
            phase,
        } = objects
        else {
            return false;
        };
        if !matches!(
            (view, phase),
            (CostObjectView::Inputs, ObjectPhase::Before)
                | (CostObjectView::Performed, ObjectPhase::After)
        ) {
            return false;
        }
        self.paid_component(payment, component)
            .is_ok_and(crate::cost_history::singular)
    }

    pub(super) fn zone_scope(&self, scope: &ZoneScope) -> Result {
        match scope {
            ZoneScope::Shared(
                EventZone::Battlefield
                | EventZone::Exile
                | EventZone::Stack
                | EventZone::Command
                | EventZone::Ante,
            ) => Ok(()),
            ZoneScope::Players {
                zone:
                    EventZone::Graveyard | EventZone::Hand | EventZone::Library | EventZone::PlanarDeck,
                players,
            } => self.players(players),
            _ => Err("zone query uses a shared/private zone with the wrong scope".into()),
        }
    }

    pub(super) fn domain(&self, domain: &ObjectDomain) -> Result {
        if domain.zones.is_empty() {
            return Err("object target domain has no zones".into());
        }
        for (i, zone) in domain.zones.iter().enumerate() {
            if domain.zones[..i].contains(zone) {
                return Err("object target domain repeats a zone".into());
            }
        }
        match domain.kind {
            ObjectKind::Permanent if *domain != ObjectDomain::permanent() => {
                Err("permanent domain must be the battlefield".into())
            }
            ObjectKind::Spell if *domain != ObjectDomain::spell() => {
                Err("spell domain must be the stack".into())
            }
            _ => Ok(()),
        }
    }

    pub(super) fn ordered(&self, objects: &ObjectSet) -> bool {
        match objects {
            ObjectSet::Observed(result) | ObjectSet::Transition { result, .. } => {
                self.ordered_results.contains(&result.0)
            }
            ObjectSet::Difference { objects, .. }
            | ObjectSet::Matching { objects, .. }
            | ObjectSet::InZone { objects, .. } => self.ordered(objects),
            ObjectSet::Zone {
                scope:
                    ZoneScope::Players {
                        zone: EventZone::Library | EventZone::Graveyard | EventZone::PlanarDeck,
                        players: PlayerSet::One(_),
                    },
                ..
            } => true,
            ObjectSet::Take { objects, .. } => self.ordered(objects),
            _ => false,
        }
    }

    pub(super) fn known_zones(&self, objects: &ObjectSet) -> Option<Vec<EventZone>> {
        match objects {
            ObjectSet::Prepared(binding) => self.prepared.get(&binding.0).cloned().flatten(),
            ObjectSet::CostObjects {
                payment,
                component,
                phase,
                ..
            } => crate::cost_history::zone(self.paid_component(payment, component).ok()?, phase)
                .map(|z| vec![z]),
            ObjectSet::Transition { result, .. }
            | ObjectSet::MovedBy { result, .. }
            | ObjectSet::Observed(result)
            | ObjectSet::Unselected(result)
            | ObjectSet::Selected(result) => self.result_zones.get(&result.0).cloned(),
            ObjectSet::Difference { objects, .. } | ObjectSet::Matching { objects, .. } => {
                self.known_zones(objects)
            }
            ObjectSet::One(ObjectRef::Result { result, .. })
                if matches!(
                    self.results.get(&result.0),
                    Some(ResultKind::Selection | ResultKind::Search)
                ) =>
            {
                self.result_zones.get(&result.0).cloned()
            }
            ObjectSet::Created(_) => Some(vec![EventZone::Battlefield]),
            ObjectSet::Zone { scope, .. } => Some(vec![match scope {
                ZoneScope::Shared(z) | ZoneScope::Players { zone: z, .. } => z.clone(),
            }]),
            ObjectSet::InZone { zone, .. } => Some(vec![zone.clone()]),
            ObjectSet::Take { objects, .. } => self.known_zones(objects),
            ObjectSet::Targets(slot) | ObjectSet::One(ObjectRef::Target { slot, .. }) => {
                match &self.slots.get(&slot.0)?.kind {
                    TargetKind::Object(domain) => Some(domain.zones.clone()),
                    _ => None,
                }
            }
            ObjectSet::One(ObjectRef::Query { objects, .. }) => self.known_zones(objects),
            ObjectSet::Union(sets) => {
                let mut zones = Vec::new();
                for set in sets {
                    zones.extend(self.known_zones(set)?);
                }
                Some(zones)
            }
            _ => None,
        }
    }

    pub(super) fn require_zone(&self, objects: &ObjectSet, zone: EventZone) -> Result {
        self.objects(objects)?;
        if self
            .known_zones(objects)
            .is_some_and(|zones| zones.iter().any(|z| *z != zone))
        {
            return Err(format!("operation requires objects in {zone:?}"));
        }
        Ok(())
    }

    pub(super) fn replacement_event(&self, event: &ReplacementEvent) -> Result {
        match event {
            ReplacementEvent::Enter { objects } => self.filter(objects),
            ReplacementEvent::Discard {
                players,
                causes,
                card,
                ..
            } => {
                self.players(players)?;
                causes.iter().chain(card).try_for_each(|f| self.filter(f))
            }
            ReplacementEvent::Damage(damage) => self.event(&EventSpec::Damage(damage.clone())),
            ReplacementEvent::ZoneChange { objects, .. } => self.filter(objects),
            ReplacementEvent::CopySpell { players, spells } => {
                self.players(players)?;
                self.filter(spells)
            }
            ReplacementEvent::Countered { spells, by } => {
                self.filter(spells)?;
                self.filter(by)
            }
            ReplacementEvent::DrawCausedBy { players, abilities } => {
                self.players(players)?;
                self.filter(abilities)
            }
            ReplacementEvent::ProduceColor { sources, .. } => self.filter(sources),
            ReplacementEvent::Primitive { name, operands } => self.primitive(name, operands),
            ReplacementEvent::Player { players, .. } => self.players(players),
            ReplacementEvent::Object { objects, .. } => self.filter(objects),
            ReplacementEvent::Tokens {
                tokens,
                controllers,
                creators,
                ..
            } => {
                self.filter(tokens)?;
                controllers
                    .iter()
                    .chain(creators)
                    .try_for_each(|players| self.players(players))
            }
            ReplacementEvent::Counters {
                objects,
                players,
                by,
                ..
            } => {
                self.filter(objects)?;
                players
                    .iter()
                    .chain(by)
                    .try_for_each(|players| self.players(players))
            }
            ReplacementEvent::ProduceMana { sources, by } => {
                self.filter(sources)?;
                by.iter().try_for_each(|player| self.player(player))
            }
            ReplacementEvent::EachPoint(event) => self.replacement_event(event),
            ReplacementEvent::Measured { event, amount, .. } => {
                self.replacement_event(event)?;
                let mut scope = self.clone();
                scope.event = crate::semantics::with_replacement(&self.event, event);
                scope.expr(amount)
            }
            ReplacementEvent::Caused { cause, event } => {
                self.filter(cause)?;
                self.replacement_event(event)
            }
            ReplacementEvent::PutOntoBattlefield { players, objects } => {
                self.players(players)?;
                self.filter(objects)
            }
            ReplacementEvent::FirstEachTurn(event) => self.replacement_event(event),
            ReplacementEvent::Either(events) => {
                if events.is_empty() {
                    return Err("replacement event alternatives are empty".into());
                }
                events.iter().try_for_each(|e| self.replacement_event(e))
            }
        }
    }

    #[inline]
    pub(super) fn event(&self, event: &EventSpec) -> Result {
        match event {
            EventSpec::Primitive {
                name: _,
                operands: _,
            }
            | EventSpec::ClassLevel {
                objects: _,
                level: _,
            }
            | EventSpec::FirstEachGame(_)
            | EventSpec::FirstEachTurn(_)
            | EventSpec::FirstForObjectEachTurn(_)
            | EventSpec::Caused { cause: _, event: _ }
            | EventSpec::WhileActivating {
                event: _,
                players: _,
                abilities: _,
            }
            | EventSpec::ZoneChangeFromElsewhere { .. }
            | EventSpec::AttackUnblocked { .. }
            | EventSpec::AllAttack(_)
            | EventSpec::OntoStack(_)
            | EventSpec::Resolves(_)
            | EventSpec::CausesTrigger { .. }
            | EventSpec::AttacksWith { .. }
            | EventSpec::AttacksAsGroup { .. }
            | EventSpec::SpellTargeting {
                players: _,
                spells: _,
                targets: _,
                count: _,
            }
            | EventSpec::SpellOrActivation {
                players: _,
                spells: _,
                abilities: _,
            }
            | EventSpec::PaysToActivate { .. }
            | EventSpec::ObjectBecomes(_)
            | EventSpec::ExploresCard {
                objects: _,
                cards: _,
            }
            | EventSpec::TappedForManaOf { .. }
            | EventSpec::ManaAdded { .. }
            | EventSpec::TargetsChosen {
                players: _,
                objects: _,
            }
            | EventSpec::ControlChanges {
                objects: _,
                to: _,
                from: _,
            }
            | EventSpec::CountersOnPlayer { by: _, players: _ }
            | EventSpec::NthCounter { .. } => self.event_case_0(event),
            _ => self.event_dispatch_1(event),
        }
    }

    #[inline]
    pub(super) fn event_dispatch_1(&self, event: &EventSpec) -> Result {
        match event {
            EventSpec::Player { .. }
            | EventSpec::NthEachTurn { event: _, test: _ }
            | EventSpec::Pair { .. }
            | EventSpec::BecomesTarget { .. }
            | EventSpec::Attack {
                attackers: _,
                by: _,
                defenders: _,
            }
            | EventSpec::Batch { .. }
            | EventSpec::Counters { .. }
            | EventSpec::Activated {
                players: _,
                abilities: _,
            }
            | EventSpec::SagaChapter(_)
            | EventSpec::Discard { .. }
            | EventSpec::ZoneChange { .. }
            | EventSpec::Object { .. }
            | EventSpec::Spell { .. }
            | EventSpec::Turn { .. }
            | EventSpec::Damage(_)
            | EventSpec::State { condition: _ }
            | EventSpec::StateWithPlayer {
                condition: _,
                player: _,
            } => self.event_case_1(event),
            _ => self.event_dispatch_2(event),
        }
    }

    #[inline]
    pub(super) fn event_dispatch_2(&self, event: &EventSpec) -> Result {
        match event {
            EventSpec::WithOtherObject {
                event: _,
                object: _,
            }
            | EventSpec::AnyOf(_)
            | EventSpec::Filtered {
                event: _,
                condition: _,
            } => self.event_case_2(event),
            _ => unreachable!("all current variants were dispatched"),
        }
    }

    pub(super) fn event_case_0(&self, event: &EventSpec) -> Result {
        match event {
            EventSpec::Primitive { name, operands } => self.primitive(name, operands),
            EventSpec::ClassLevel { objects, level } => {
                self.filter(objects)?;
                self.expr(level)
            }
            EventSpec::FirstEachGame(event)
            | EventSpec::FirstEachTurn(event)
            | EventSpec::FirstForObjectEachTurn(event) => self.event(event),
            EventSpec::Caused { cause, event } => {
                self.filter(cause)?;
                self.event(event)
            }
            EventSpec::WhileActivating {
                event,
                players,
                abilities,
            } => {
                self.event(event)?;
                self.players(players)?;
                self.filter(abilities)
            }
            EventSpec::ZoneChangeFromElsewhere { objects, .. } => self.filter(objects),
            EventSpec::AttackUnblocked {
                attackers, count, ..
            } => {
                self.filter(attackers)?;
                count.iter().try_for_each(|(_, n)| self.expr(n))
            }
            EventSpec::AllAttack(objects)
            | EventSpec::OntoStack(objects)
            | EventSpec::Resolves(objects) => self.filter(objects),
            EventSpec::CausesTrigger { cause, .. } => self.event(cause),
            EventSpec::AttacksWith {
                object,
                others,
                count,
                by,
                ..
            } => {
                self.object(object)?;
                self.filter(others)?;
                self.expr(&count.1)?;
                by.iter().try_for_each(|p| self.players(p))
            }
            EventSpec::AttacksAsGroup { by, attackers, .. } => {
                self.players(by)?;
                self.filter(attackers)
            }
            EventSpec::SpellTargeting {
                players,
                spells,
                targets,
                count,
            } => {
                self.players(players)?;
                self.filter(spells)?;
                targets.iter().try_for_each(|t| self.objects(t))?;
                count.iter().try_for_each(|(_, n)| self.expr(n))
            }
            EventSpec::SpellOrActivation {
                players,
                spells,
                abilities,
            } => {
                self.players(players)?;
                self.filter(spells)?;
                self.filter(abilities)
            }
            EventSpec::PaysToActivate {
                players, abilities, ..
            } => {
                self.players(players)?;
                self.filter(abilities)
            }
            EventSpec::ObjectBecomes(objects) => self.filter(objects),
            EventSpec::ExploresCard { objects, cards } => {
                self.filter(objects)?;
                self.filter(cards)
            }
            EventSpec::TappedForManaOf { objects, by, .. } => {
                self.filter(objects)?;
                by.iter().try_for_each(|p| self.players(p))
            }
            EventSpec::ManaAdded {
                abilities, players, ..
            } => {
                self.filter(abilities)?;
                self.players(players)
            }
            EventSpec::TargetsChosen { players, objects } => {
                self.players(players)?;
                self.filter(objects)
            }
            EventSpec::ControlChanges { objects, to, from } => {
                self.filter(objects)?;
                self.players(to)?;
                self.players(from)
            }
            EventSpec::CountersOnPlayer { by, players } => {
                self.players(by)?;
                self.players(players)
            }
            EventSpec::NthCounter {
                kind, objects, nth, ..
            } => {
                self.operand(kind)?;
                self.filter(objects)?;
                self.expr(nth)
            }
            _ => unreachable!("validated dispatch"),
        }
    }
}
