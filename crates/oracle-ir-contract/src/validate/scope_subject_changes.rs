//! Scoped validation of portable operands.
use super::*;
impl<'a> Scope<'a> {
    pub(super) fn subject_changes(&self, subjects: &EffectSubjects, changes: &[Change]) -> Result {
        if changes.is_empty() {
            return Err("empty continuous effect".into());
        }
        let mut scope = self.clone();
        scope.affected = false;
        scope.affected_player = false;
        match subjects {
            EffectSubjects::Objects(objects) => {
                scope.objects(objects)?;
                scope.affected = true;
            }
            EffectSubjects::Players(players) => {
                scope.players(players)?;
                scope.affected_player = true;
            }
        }
        for change in changes {
            if matches!(change, Change::PlayerRule(_)) != scope.affected_player {
                return Err(
                    "continuous effect change does not match its object/player subjects".into(),
                );
            }
            scope.change(change)?;
        }
        Ok(())
    }

    pub(super) fn mana_payment(&self, payment: &ManaPayment) -> Result {
        match payment {
            ManaPayment::Anything => Ok(()),
            ManaPayment::Spells(filter) | ManaPayment::Abilities(filter) => self.filter(filter),
        }
    }

    pub(super) fn player_action(&self, action: &PlayerAction) -> Result {
        match action {
            PlayerAction::CastSpell { filter, from } | PlayerAction::PlayLand { filter, from } => {
                if matches!(
                    from,
                    Some(ZoneScope::Shared(EventZone::Battlefield | EventZone::Stack))
                ) {
                    return Err("cast/play origin must be a card zone".into());
                }
                if let Some(zone) = from {
                    self.zone_scope(zone)?;
                }
                self.filter(filter)
            }
            PlayerAction::AttackWith(filter) | PlayerAction::BlockWith(filter) => {
                self.filter(filter)
            }
            PlayerAction::AttackInto(recipients) => self.recipients(recipients),
            PlayerAction::PayLife(payment) => self.mana_payment(payment),
            PlayerAction::Sacrifice { objects, paying } => {
                self.filter(objects)?;
                paying.iter().try_for_each(|p| self.mana_payment(p))
            }
            PlayerAction::Activate(filter) => self.activation_filter(filter),
            PlayerAction::PlayCard(filter)
            | PlayerAction::ExileObjects(filter)
            | PlayerAction::BeTargeted(filter) => self.filter(filter),
            PlayerAction::AttackDefender(defenders) => self.defenders(defenders),
            _ => Ok(()),
        }
    }

    pub(super) fn player_rule(&self, rule: &PlayerRule) -> Result {
        match rule {
            PlayerRule::SpecialAction {
                body, cost, during, ..
            } => {
                self.operand(body)?;
                cost.iter().try_for_each(|c| self.operand(c))?;
                during.iter().try_for_each(|p| self.players(p))
            }
            PlayerRule::ActivationGrant { abilities, grant } => {
                self.filter(abilities)?;
                match grant {
                    ActivationGrant::InstantSpeedDuring(players) => self.players(players),
                    ActivationGrant::AdditionalTimes(times) => self.expr(times),
                    _ => Ok(()),
                }
            }
            PlayerRule::KeywordCost {
                cards, change, per, ..
            } => {
                self.filter(cards)?;
                per.iter().try_for_each(|n| self.expr(n))?;
                self.keyword_cost_change(change)
            }
            PlayerRule::KeywordAccess { cards, during, .. } => {
                self.filter(cards)?;
                during.iter().try_for_each(|p| self.players(p))
            }
            PlayerRule::Decides(GameDecision::HowCreaturesBlock(filter))
            | PlayerRule::Decides(GameDecision::CombatDamageOf(filter))
            | PlayerRule::AttackedOnlyBy(filter)
            | PlayerRule::TargetsMustInclude(filter)
            | PlayerRule::TargetableDespite {
                sources: filter, ..
            } => self.filter(filter),
            PlayerRule::Decides(GameDecision::SearchesOf(players))
            | PlayerRule::DrawsDuringDrawStepOf(players) => self.players(players),
            PlayerRule::MustAttack {
                attackers,
                defenders,
                count,
            } => {
                self.filter(attackers)?;
                self.defenders(defenders)?;
                count.iter().try_for_each(|(_, n)| self.expr(n))
            }
            PlayerRule::AttackOnly(defenders) => self.defenders(defenders),
            PlayerRule::AttackWithoutTapping {
                attackers,
                condition,
            } => {
                self.filter(attackers)?;
                self.condition(condition)
            }
            PlayerRule::BlockerLimit(n)
            | PlayerRule::SurveilBonus(n)
            | PlayerRule::DevotionBonus(n) => self.expr(n),
            PlayerRule::SpendColorAs { on, .. } => self.mana_payment(on),
            PlayerRule::RevealDraws {
                first_during: Some(players),
                ..
            } => self.players(players),
            PlayerRule::CantBeCaused { action, by } => {
                self.player_action(action)?;
                self.filter(by)
            }
            PlayerRule::UntapsDuringUntapStepOf {
                permanents,
                players,
            } => {
                self.filter(permanents)?;
                self.players(players)
            }
            PlayerRule::UntapOnlyOneOfTypes(types) => {
                types.iter().try_for_each(|o| self.operand(o))
            }
            PlayerRule::LifeForMana { life, symbol: mana }
            | PlayerRule::LifeForManaAbility { life, mana } => {
                self.expr(life)?;
                self.operand(mana)
            }
            PlayerRule::TapOthersForMana {
                permanents,
                modifier,
            } => {
                self.filter(permanents)?;
                self.operand(modifier)
            }
            PlayerRule::DiceChange { cost, .. } => self.operand(cost),
            PlayerRule::CascadePut { filter, flags } => {
                self.filter(filter)?;
                self.operand(flags)
            }
            PlayerRule::AsManaEmpties { mana, then } => {
                self.operand(mana)?;
                self.operand(then)
            }
            PlayerRule::GrantKeyword(
                PlayerKeyword::Protection(quality) | PlayerKeyword::HexproofFrom(quality),
            ) => self.protection(quality),
            PlayerRule::PlayPermission {
                cards,
                from,
                filter,
                ..
            } => {
                if matches!(from, EventZone::Stack | EventZone::Battlefield) {
                    return Err("play permission origin must be a card zone".into());
                }
                if self.known_zones(cards).is_some_and(|zones| {
                    zones
                        .iter()
                        .any(|z| matches!(z, EventZone::Stack | EventZone::Battlefield))
                }) {
                    return Err(
                        "play permission must select cards outside the battlefield and stack"
                            .into(),
                    );
                }
                self.require_zone(cards, from.clone())?;
                self.filter(filter)
            }
            PlayerRule::PlayPermissionWith { permission, terms } => {
                self.player_rule(permission)?;
                if let Some(players) = &terms.during {
                    self.players(players)?;
                }
                if let PlayCost::Alternate(cost) | PlayCost::Additional(cost) = &terms.cost {
                    self.operand(cost)?;
                }
                if let Some(condition) = &terms.condition {
                    self.condition(condition)?;
                }
                // A program here runs as the card is cast, and reads that spell.
                let mut cast = self.clone();
                cast.event.object = true;
                cast.event.spell = true;
                terms.effects.iter().try_for_each(|o| match o {
                    Operand::Program(_) => cast.operand(o),
                    _ => self.operand(o),
                })
            }
            PlayerRule::Primitive { name, operands } => self.primitive(name, operands),
            PlayerRule::SpellsHaveAbilities {
                spells,
                nth_each_turn,
                abilities,
                ..
            } => {
                self.filter(spells)?;
                if let Some(nth) = nth_each_turn {
                    self.expr(nth)?;
                }
                self.operand(abilities)
            }
            PlayerRule::SpendManaAs { on, .. } => match on {
                ManaPayment::Anything => Ok(()),
                ManaPayment::Spells(filter) | ManaPayment::Abilities(filter) => self.filter(filter),
            },
            PlayerRule::ActivationCost {
                abilities, change, ..
            } => {
                self.filter(abilities)?;
                self.activation_cost_change(change)
            }
            PlayerRule::Prohibit(action) => self.player_action(action),
            PlayerRule::LimitPerTurn { action, amount } => {
                self.player_action(action)?;
                self.count(&Count::Exactly(amount.clone()))
            }
            PlayerRule::HandSize(HandSizeRule::Set(n) | HandSizeRule::Adjust(n)) => self.expr(n),
            PlayerRule::AdditionalLandPlays(count) => {
                if !matches!(count, Count::Exactly(_) | Count::AnyNumber) {
                    return Err("additional land allowance must be exact or unlimited".into());
                }
                self.count(count)
            }
            PlayerRule::SpellCost { spells, modifier } => {
                self.filter(spells)?;
                let mut each = self.clone();
                each.affected = true;
                each.spell_cost_modifier(modifier)
            }
            PlayerRule::SpellTiming { spells, .. } => self.filter(spells),
            PlayerRule::Visibility { objects, .. } => self.objects(objects),
            PlayerRule::UntapLimit { permanents, amount } => {
                self.filter(permanents)?;
                self.count(&Count::Exactly(amount.clone()))
            }
            PlayerRule::RetainMana {
                colors: Some(colors),
            } if colors.is_empty() => Err("empty mana retention colors".into()),
            _ => Ok(()),
        }
    }

    pub(super) fn protection(&self, protection: &Protection) -> Result {
        match protection {
            Protection::Objects { filter, .. } => self.filter(filter),
            Protection::Players(players) => self.players(players),
            Protection::Each(qualities) => {
                if qualities.len() < 2 {
                    return Err(
                        "multiple protection qualities require at least two instances".into(),
                    );
                }
                for quality in qualities {
                    self.protection(quality)?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn keyword(&self, keyword: &Keyword) -> Result {
        match keyword {
            Keyword::WithReminder { keyword, .. } => {
                if matches!(keyword.as_ref(), Keyword::WithReminder { .. }) {
                    return Err("keyword presentation must not be nested".into());
                }
                self.keyword(keyword)
            }
            Keyword::Protection(quality) | Keyword::HexproofFrom(quality) => {
                self.protection(quality)
            }
            Keyword::Landwalk(lands) => self.filter(lands),
            Keyword::Ward(cost) => {
                let scope = Scope {
                    ward_payer: true,
                    keyword_host: true,
                    ..Scope::default()
                };
                scope.cost(cost)
            }
            _ => Ok(()),
        }
    }

    pub(super) fn removals(&self, removals: &[CounterRemoval]) -> Result {
        if removals.is_empty() {
            return Err("empty counter removal batch".into());
        }
        for removal in removals {
            self.player(&removal.player)?;
            self.recipients(&removal.recipients)?;
            if let CounterAmount::Count(count) | CounterAmount::AmongThem(count) = &removal.amount {
                self.count(count)?;
            }
            if let Some(selection) = &removal.selection {
                if !matches!(removal.recipients, Recipients::Objects(_)) {
                    return Err("counter removal selection requires objects".into());
                }
                self.player(&selection.player)?;
                self.count(&selection.count)?;
            }
        }
        Ok(())
    }

    pub(super) fn cost_object_selection(&self, selection: &CostObjectSelection) -> Result {
        self.objects(&selection.objects)?;
        if let Some(count) = &selection.choose {
            if matches!(count, Count::AsManyAsCan(_)) {
                return Err("cost selection must be payable in full".into());
            }
            self.count(count)?;
        }
        Ok(())
    }

    pub(super) fn play_cost(&self, cost: &PlayCost) -> Result {
        match cost {
            PlayCost::Alternate(paid) | PlayCost::Additional(paid) | PlayCost::Reduced(paid) => {
                self.operand(paid)
            }
            PlayCost::Normal | PlayCost::Free | PlayCost::Using(_) => Ok(()),
        }
    }

    pub(super) fn activation_cost_change(&self, change: &ActivationCostChange) -> Result {
        match change {
            ActivationCostChange::Additional(cost) => self.cost(cost),
            ActivationCostChange::Reduce(amount) | ActivationCostChange::Increase(amount) => {
                self.operand(amount)
            }
            ActivationCostChange::ReduceX { amount, times } => {
                self.operand(amount)?;
                self.expr(times)
            }
        }
    }

    pub(super) fn keyword_cost_change(&self, change: &KeywordCostChange) -> Result {
        match change {
            KeywordCostChange::Reduce(amount)
            | KeywordCostChange::Increase(amount)
            | KeywordCostChange::Alternate(amount) => self.operand(amount),
        }
    }

    pub(super) fn spell_cost_modifier(&self, modifier: &SpellCostModifier) -> Result {
        match modifier {
            SpellCostModifier::Increase(cost) => self.cost(cost),
            SpellCostModifier::Alternate { cost, .. } => self.play_cost(cost),
            SpellCostModifier::AlternateDuring { cost, during, .. } => {
                self.players(during)?;
                self.play_cost(cost)
            }
            SpellCostModifier::PayToReduce { cost, mana } => {
                self.operand(cost)?;
                self.operand(mana)
            }
            SpellCostModifier::OptionalAdditional { cost, effects } => {
                self.operand(cost)?;
                self.operand(effects)
            }
            SpellCostModifier::Scaled { change, per } => {
                if let CostScale::Times(times) = per {
                    self.expr(times)?;
                }
                self.activation_cost_change(change)
            }
            SpellCostModifier::Minimum(n) => self.count(&Count::Exactly(n.clone())),
            SpellCostModifier::Reduce {
                reductions,
                spill_into_generic,
            } => {
                if reductions.is_empty() {
                    return Err("empty spell cost reduction".into());
                }
                for r in reductions {
                    if !spill_into_generic && r.color.is_none() {
                        return Err("non-spilling colored reduction requires a color".into());
                    }
                    self.count(&Count::Exactly(r.amount.clone()))?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn activation_rule(&self, rule: &ActivationRule) -> Result {
        match rule {
            ActivationRule::RestrictMana { .. } => Ok(()),
            ActivationRule::ManaInstead(cost) => self.cost(cost),
            ActivationRule::Cost {
                modifier,
                condition,
            } => {
                self.spell_cost_modifier(modifier)?;
                condition.iter().try_for_each(|c| self.condition(c))
            }
            ActivationRule::OnlyBy(players) => self.players(players),
            ActivationRule::XIs(test) => self.number_test(test),
            ActivationRule::MaxPerTurn(n) | ActivationRule::XAtMost(n) => self.expr(n),
            ActivationRule::XNotZero
            | ActivationRule::CantBeCopied
            | ActivationRule::AnyTimeOfInstant => Ok(()),
        }
    }

    pub(super) fn cost(&self, cost: &Cost) -> Result {
        for part in &cost.parts {
            match part {
                CostPart::Primitive { name, operands } => self.primitive(name, operands)?,
                CostPart::LifeEqualToOwnManaValue
                | CostPart::ManaAnyNumberOfTimes(_)
                | CostPart::RevealChoice(_)
                | CostPart::EnergyOneOrMore
                | CostPart::ManaFromTreasure(_)
                | CostPart::ShuffleFromHand => {}
                CostPart::ManaCostReduced { object, reduction } => {
                    object.iter().try_for_each(|o| self.object(o))?;
                    self.operand(reduction)?;
                }
                CostPart::ManaXBounded { value, .. } => self.expr(value)?,
                CostPart::UnattachFrom { attachments, hosts } => {
                    self.objects(attachments)?;
                    self.objects(hosts)?;
                }
                CostPart::EachPlayerAction { players, action } => {
                    self.players(players)?;
                    self.operand(action)?;
                }
                CostPart::ManaReducedForEach { reduction, per, .. } => {
                    self.operand(reduction)?;
                    self.expr(per)?;
                }
                CostPart::PlayFromTop(filter) => self.filter(filter)?,
                CostPart::ObjectAction { objects, action } => {
                    self.objects(objects)?;
                    if let CostObjectAction::AttachToPlayer(players) = action {
                        self.players(players)?;
                    }
                }
                CostPart::SinglePlayersCards(part) => self.cost(&Cost {
                    parts: vec![part.as_ref().clone()],
                })?,
                CostPart::Constrained { part, constraints } => {
                    let selectable = match part.as_ref() {
                        CostPart::Sacrifice { .. }
                        | CostPart::Discard { .. }
                        | CostPart::Reveal { .. } => true,
                        CostPart::ChangeTapState {
                            selections,
                            symbol: false,
                            ..
                        }
                        | CostPart::Move { selections, .. } => selections.len() == 1,
                        _ => false,
                    };
                    if !selectable
                        || constraints.is_empty()
                        || constraints
                            .iter()
                            .any(|c| matches!(c, SelectionConstraint::CanMoveTo { .. }))
                    {
                        return Err("group payment requires a single object selection and nonempty payment constraints".into());
                    }
                    self.cost(&Cost {
                        parts: vec![part.as_ref().clone()],
                    })?;
                    self.selection_constraints(constraints)?;
                }
                CostPart::ManaCostOf(object) => self.object(object)?,
                CostPart::Move {
                    selections,
                    destination,
                } => {
                    self.operation(&Operation::Move {
                        objects: ObjectSet::Union(
                            selections.iter().map(|s| s.objects.clone()).collect(),
                        ),
                        destination: destination.clone(),
                        qualifiers: vec![],
                    })?;
                    for selection in selections {
                        self.cost_object_selection(selection)?;
                    }
                }
                CostPart::ChangeTapState {
                    selections, symbol, ..
                } => {
                    if selections.is_empty() {
                        return Err("tap-state payment has no object selections".into());
                    }
                    if *symbol
                        && !matches!(
                            selections.as_slice(),
                            [CostObjectSelection {
                                objects: ObjectSet::One(
                                    ObjectRef::Source | ObjectRef::AbilityHolder
                                ),
                                choose: None
                            }]
                        )
                    {
                        return Err("tap/untap symbol requires the implicit source or keyword host without a choice".into());
                    }
                    for selection in selections {
                        self.cost_object_selection(selection)?;
                        self.require_zone(&selection.objects, EventZone::Battlefield)?;
                    }
                }
                CostPart::Choice(options) => {
                    if options.is_empty() {
                        return Err("cost choice has no alternatives".into());
                    }
                    for cost in options {
                        self.cost(cost)?;
                    }
                }
                CostPart::Discard { count, filter, .. } => {
                    self.count(&Count::Exactly(count.clone()))?;
                    self.filter(filter)?;
                }
                CostPart::RemoveCounters { removals } => self.removals(removals)?,
                CostPart::Mana(_) | CostPart::TapSymbol => {}
                CostPart::Life(amount) => self.count(&Count::Exactly(amount.clone()))?,
                CostPart::Loyalty(delta) | CostPart::Energy(delta) => self.expr(delta)?,
                CostPart::Draw { count } | CostPart::Mill { count } => {
                    self.count(&Count::Exactly(count.clone()))?
                }
                CostPart::PlayerAction { player, body } => {
                    self.player(player)?;
                    self.clone().program(body)?;
                }
                CostPart::Reveal { count, filter, .. } => {
                    self.count(count)?;
                    self.filter(filter)?;
                }
                CostPart::ManaWithX { x, .. } => x.iter().try_for_each(|x| self.expr(x))?,
                CostPart::Exert(object)
                | CostPart::Unattach(object)
                | CostPart::Unspecialize(object) => self.object(object)?,
                CostPart::Keyword { amount, .. } => {
                    if let Some(amount) = amount {
                        self.expr(amount)?;
                    }
                }
                CostPart::DiscardHand => {}
                CostPart::BeholdAndExile(what) => self.operand(what)?,
                CostPart::Behold { count, what } => {
                    self.expr(count)?;
                    self.operand(what)?;
                }
                CostPart::ExiledToGraveyard(cards) => self.objects(cards)?,
                CostPart::RevealTop(filter) => self.filter(filter)?,
                CostPart::Waterbend(_) | CostPart::Teamwork(_) => {}
                CostPart::MoveCounters {
                    from, to, amount, ..
                } => {
                    self.objects(from)?;
                    self.objects(to)?;
                    if let MovedCounters::Exactly(amount) | MovedCounters::AtLeast(amount) = amount
                    {
                        self.expr(amount)?;
                    }
                }
                CostPart::WaterbendX { x, .. } => self.expr(x)?,
                CostPart::PutCounters(placements) => {
                    for placement in placements {
                        self.recipients(&placement.recipients)?;
                        self.count(&Count::Exactly(placement.amount.clone()))?;
                    }
                }
                CostPart::Repeated { part, times } => {
                    self.count(&Count::Exactly(times.clone()))?;
                    self.cost(&Cost {
                        parts: vec![part.as_ref().clone()],
                    })?;
                }
                CostPart::Sacrifice { objects, choose } => {
                    self.cost_object_selection(&CostObjectSelection {
                        objects: objects.clone(),
                        choose: choose.clone(),
                    })?;
                }
            }
        }
        Ok(())
    }
}
