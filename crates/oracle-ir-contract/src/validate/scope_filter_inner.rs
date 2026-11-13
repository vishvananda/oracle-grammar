//! Scoped validation of portable operands.
use super::*;
impl<'a> Scope<'a> {
    pub(super) fn filter_inner(&self, filter: &ObjectFilter) -> Result {
        match filter {
            ObjectFilter::Noted(_) | ObjectFilter::NthChosen(_) => Ok(()),
            ObjectFilter::MostCommonCreatureTypeIn(player) => self.player(player),
            ObjectFilter::TargetCount(_, value) => self.expr(value),
            ObjectFilter::ChosenBy { player, .. } => self.player(player),
            ObjectFilter::ChosenOn { object, .. }
            | ObjectFilter::AttackingSame(object)
            | ObjectFilter::SharesTypeFrom { with: object, .. } => self.object(object),
            ObjectFilter::DraftNoted(note) => self.player(&note.player),
            ObjectFilter::InPile(object) | ObjectFilter::InSector(object) => {
                object.iter().try_for_each(|object| self.object(object))
            }
            ObjectFilter::CastFromAmong(objects) | ObjectFilter::SharesMostCommonColor(objects) => {
                self.objects(objects)
            }
            ObjectFilter::ArrivedThisTurn { .. } | ObjectFilter::ChosenFilter => Ok(()),
            ObjectFilter::SharesColorWith(object)
            | ObjectFilter::ExiledWith(object)
            | ObjectFilter::MadeBy(object)
            | ObjectFilter::WasAttachedTo(object) => self.object(object),
            ObjectFilter::DealtExcessBy(result) if self.results.contains_key(&result.0) => Ok(()),
            ObjectFilter::DealtExcessBy(result) => {
                Err(format!("result {} is unavailable", result.0))
            }
            ObjectFilter::DamagedBy(result) if self.results.contains_key(&result.0) => Ok(()),
            ObjectFilter::DamagedBy(result) => Err(format!("result {} is unavailable", result.0)),
            ObjectFilter::Primitive { name, operands } => self.primitive(name, operands),
            ObjectFilter::History { event, window } => {
                match event {
                    ObjectHistory::AttackedDefender(filter) | ObjectHistory::Enlisted(filter) => {
                        self.filter(filter)?
                    }
                    ObjectHistory::Drawn(Some(players))
                    | ObjectHistory::ExiledFromGraveyard(players) => self.players(players)?,
                    ObjectHistory::DamagedBy(object)
                    | ObjectHistory::Convoked(object)
                    | ObjectHistory::Exploited(object)
                    | ObjectHistory::TappedToPayFor(object)
                    | ObjectHistory::PhasedOutWith(object)
                    | ObjectHistory::CountersPutWith { with: object, .. }
                    | ObjectHistory::CountersRemovedWith { with: object, .. } => {
                        self.object(object)?
                    }
                    ObjectHistory::CountersPutBy { by, .. } => self.players(by)?,
                    _ => {}
                }
                match window {
                    ObjectWindow::LastTurnOf(player)
                    | ObjectWindow::SinceLastUpkeepOf(player)
                    | ObjectWindow::SinceLastTurnOf(player)
                    | ObjectWindow::LastCombatOf(player) => self.player(player),
                    _ => Ok(()),
                }
            }
            ObjectFilter::ManaSpent {
                sources,
                payers,
                value,
                ..
            } => {
                if let Some(sources) = sources {
                    self.filter(sources)?;
                }
                if let Some(payers) = payers {
                    self.players(payers)?;
                }
                self.expr(value)
            }
            ObjectFilter::AlternateCostPaid(operands) | ObjectFilter::ManaCostIs(operands) => {
                operands
                    .iter()
                    .try_for_each(|operand| self.operand(operand))
            }
            ObjectFilter::ManaSymbolInCost(_) | ObjectFilter::NameFirstPrintedIn(_) => Ok(()),
            ObjectFilter::BlockingAny(filter)
            | ObjectFilter::BlockedByAny(filter)
            | ObjectFilter::PairedWith(filter)
            | ObjectFilter::CanTargetOnly(filter)
            | ObjectFilter::WouldDestroy(filter) => self.filter(filter),
            ObjectFilter::BandedWith(object)
            | ObjectFilter::TargetableBy(object)
            | ObjectFilter::CouldProduceManaOf(object) => self.object(object),
            ObjectFilter::AttachedCount {
                attachments, value, ..
            } => {
                self.filter(attachments)?;
                self.expr(value)
            }
            ObjectFilter::ProtectedBy(players) | ObjectFilter::InStartingDeck(players) => {
                self.players(players)
            }
            ObjectFilter::CouldEnchant(objects) => self.objects(objects),
            ObjectFilter::CouldBeEnchantedBy(aura) => self.object(aura),
            ObjectFilter::CouldProduce(operand) => self.operand(operand),
            ObjectFilter::ExiledBy { player, with, .. } => {
                self.player(player)?;
                match with {
                    Some(object) => self.object(object),
                    None => Ok(()),
                }
            }
            ObjectFilter::Condition(condition) => self.condition(condition),
            ObjectFilter::HasAttached(filter) => self.filter(filter),
            ObjectFilter::AttachedToPlayer(player) => self.player(player),
            ObjectFilter::Extreme { among, .. } => self.objects(among),
            ObjectFilter::ManaSpentFrom(sources) => self.filter(sources),
            ObjectFilter::TargetedBy(filter) => self.filter(filter),
            ObjectFilter::ColorOutsideCommanderIdentity(player) => self.player(player),
            ObjectFilter::SharesColorWithMana(_) | ObjectFilter::ManaCostPayableWith(_) => Ok(()),
            ObjectFilter::KickedWith(kicker) => {
                kicker.iter().try_for_each(|operand| self.operand(operand))
            }
            ObjectFilter::NthCast {
                ordinal,
                spells,
                by,
                ..
            } => {
                self.expr(ordinal)?;
                self.filter(spells)?;
                by.iter().try_for_each(|players| self.players(players))
            }
            ObjectFilter::Cast { by, from } => {
                if let Some(players) = by {
                    self.players(players)?;
                }
                if let Some(scope) = from {
                    self.zone_scope(scope)?;
                }
                Ok(())
            }
            ObjectFilter::EnteredFrom(scope) => self.zone_scope(scope),
            ObjectFilter::HasKeywordFamily(_) => Ok(()),
            ObjectFilter::HasKeyword(keyword) => self.keyword(keyword),
            ObjectFilter::TargetsAny(targets) => self.recipients(targets),
            ObjectFilter::OnStack(_) => Ok(()),
            ObjectFilter::AbilityOf(sources) => self.filter(sources),
            ObjectFilter::HasActivatedAbility(filter) => self.activation_filter(filter),
            ObjectFilter::InZone(scope) => self.zone_scope(scope),
            ObjectFilter::InSet(objects) | ObjectFilter::SharesColorWithAny(objects) => {
                self.objects(objects)
            }
            ObjectFilter::Any
            | ObjectFilter::CardType(_)
            | ObjectFilter::Supertype(_)
            | ObjectFilter::Subtype(_)
            | ObjectFilter::Color(_)
            | ObjectFilter::Colorless
            | ObjectFilter::IsCard
            | ObjectFilter::Name(_)
            | ObjectFilter::Token
            | ObjectFilter::Modified
            | ObjectFilter::Tapped
            | ObjectFilter::Attacking
            | ObjectFilter::Blocking => Ok(()),
            ObjectFilter::State(_) | ObjectFilter::Chosen(_) => Ok(()),
            ObjectFilter::BlockedByObject(object) => self.object(object),
            ObjectFilter::AttackingDefender(defenders) => self.defenders(defenders),
            ObjectFilter::AttachedTo(filter) => self.filter(filter),
            ObjectFilter::BlockingObject(object) => self.object(object),
            ObjectFilter::AllOf(items) | ObjectFilter::AnyOf(items) => {
                for item in items {
                    self.filter(item)?;
                }
                Ok(())
            }
            ObjectFilter::Not(filter) => self.filter(filter),
            ObjectFilter::ControlledBy(players) | ObjectFilter::OwnedBy(players) => {
                self.players(players)
            }
            ObjectFilter::SameObject(object)
            | ObjectFilter::SharesNameWith(object)
            | ObjectFilter::Shares { with: object, .. } => self.object(object),
            ObjectFilter::SharesNameWithAny(objects)
            | ObjectFilter::SharesNameWithAnother(objects)
            | ObjectFilter::SharesWithAny { with: objects, .. } => self.objects(objects),
            ObjectFilter::Property { test, .. } => self.number_test(test),
            ObjectFilter::Counters { value, .. } => self.expr(value),
        }
    }

    pub(super) fn number_test(&self, test: &NumberTest) -> Result {
        match test {
            NumberTest::Compare { value, .. } => self.expr(value),
            NumberTest::AllOf(tests) | NumberTest::AnyOf(tests) => {
                tests.iter().try_for_each(|test| self.number_test(test))
            }
            NumberTest::AnyProperty { objects, .. } => self.objects(objects),
        }
    }

    pub(super) fn condition(&self, condition: &Condition) -> Result {
        match condition {
            Condition::ChoseNth { player, .. } => self.player(player),
            Condition::IsPrime(value) => self.expr(value),
            Condition::WonSubgame { player, .. } | Condition::CompletedDungeon { player, .. } => {
                self.player(player)
            }
            Condition::NotedForDraft { player, note } => {
                self.player(player)?;
                self.player(&note.player)
            }
            Condition::TargetCount {
                object,
                filter,
                value,
                ..
            } => {
                self.object(object)?;
                self.filter(filter)?;
                self.expr(value)
            }
            Condition::AsCast { player, action } => {
                self.player(player)?;
                match action {
                    AsCastAction::Revealed(filter) => self.filter(filter),
                    AsCastAction::Controlled(objects) | AsCastAction::Chose(objects) => {
                        self.objects(objects)
                    }
                }
            }
            Condition::PlayerIn { player, players } => {
                self.player(player)?;
                self.players(players)
            }
            Condition::ValueChosenIs(player) | Condition::ChoseExtreme { player, .. } => {
                self.player(player)
            }
            Condition::HadExtreme { player, filter, .. } => {
                self.player(player)?;
                filter.iter().try_for_each(|filter| self.filter(filter))
            }
            Condition::VotedAs { player, other, .. } => {
                self.player(player)?;
                self.player(other)
            }
            Condition::CoinCameUp { player, .. } => {
                player.iter().try_for_each(|player| self.player(player))
            }
            Condition::Outcome(_) | Condition::AllVotedFor(_) => Ok(()),
            Condition::MostCommonColor { color, objects, .. } => {
                self.operand(color)?;
                self.objects(objects)
            }
            Condition::GraveyardPosition {
                card,
                player,
                above,
                amount,
                ..
            } => {
                self.object(card)?;
                self.player(player)?;
                self.filter(above)?;
                self.expr(amount)
            }
            Condition::PlayerDid {
                player,
                event,
                window,
            } => {
                self.player(player)?;
                match event {
                    PlayerHistory::AttackedWithTotalPower(_, value)
                    | PlayerHistory::PaidOrLostEnergy(_, value)
                    | PlayerHistory::RolledDie(_, value) => self.expr(value)?,
                    PlayerHistory::AttackedBy(filter)
                    | PlayerHistory::AttackedWith(filter)
                    | PlayerHistory::AttackedDefender(filter)
                    | PlayerHistory::CastSpell(filter)
                    | PlayerHistory::PlayedFromExile(filter)
                    | PlayerHistory::PermanentEntered(filter) => self.filter(filter)?,
                    PlayerHistory::AttackedPlayer(other) => self.player(other)?,
                    PlayerHistory::Cycled(_, value, filter) => {
                        self.expr(value)?;
                        self.filter(filter)?
                    }
                    _ => {}
                }
                match window {
                    ObjectWindow::LastTurnOf(player)
                    | ObjectWindow::SinceLastUpkeepOf(player)
                    | ObjectWindow::SinceLastTurnOf(player)
                    | ObjectWindow::LastCombatOf(player) => self.player(player),
                    _ => Ok(()),
                }
            }
            Condition::ValueChosen(_) => Ok(()),
            Condition::Primitive { name, operands } => self.primitive(name, operands),
            Condition::SubsetMeets {
                objects,
                amount,
                constraints,
                ..
            } => {
                self.objects(objects)?;
                self.expr(amount)?;
                self.selection_constraints(constraints)
            }
            Condition::Window(_) => Ok(()),
            Condition::FirstDrawInOwnDrawStep(player) => self.player(player),
            Condition::PlayerState { player, .. } => self.player(player),
            Condition::ItsNeitherDayNorNight | Condition::ItsNight => Ok(()),
            Condition::MostVotes { option, .. } | Condition::Voted(option) => {
                self.vote_option(option)
            }
            Condition::PlayerAttacks { attacker, defender } => {
                self.player(attacker)?;
                self.player(defender)
            }
            Condition::SameTeam { player, other } => {
                self.player(player)?;
                self.player(other)
            }
            Condition::VotedFor { player, option } => {
                self.player(player)?;
                self.vote_option(option)
            }
            Condition::ChosenWord(_) | Condition::GiftPromised(_) => Ok(()),
            Condition::PlayerTook { player, result } => {
                self.player(player)?;
                if self.results.contains_key(&result.0) {
                    Ok(())
                } else {
                    Err(format!("result {} is unavailable", result.0))
                }
            }
            Condition::Searched(result) => match self.results.get(&result.0) {
                Some(ResultKind::Search) => Ok(()),
                _ => Err("searched condition requires an available search result".into()),
            },
            Condition::TargetsMatch { object, filter, .. } => {
                self.object(object)?;
                self.filter(filter)
            }
            Condition::WasCastFrom { object, .. } => self.object(object),
            Condition::ExistsPlayer {
                players,
                binding,
                condition,
            } => {
                self.players(players)?;
                let mut child = self.clone();
                if !child.players.insert(binding.0) {
                    return Err("player predicate redeclares a binding".into());
                }
                child.condition(condition)
            }
            Condition::PaymentChosen(result) => match self.results.get(&result.0) {
                Some(ResultKind::Payment) => Ok(()),
                _ => Err(format!(
                    "result {} is not an available payment decision",
                    result.0
                )),
            },
            Condition::PlayerIsActive(player) => self.player(player),
            Condition::Compare { left, test } => {
                self.expr(left)?;
                self.number_test(test)
            }
            Condition::CoinFlip { result, .. } => match self.results.get(&result.0) {
                Some(ResultKind::Coin) => Ok(()),
                _ => Err(format!(
                    "result {} is not a single coin flip in this scope",
                    result.0
                )),
            },
            Condition::AllOf(items) | Condition::AnyOf(items) => {
                for item in items {
                    self.condition(item)?;
                }
                Ok(())
            }
            Condition::Not(condition) => self.condition(condition),
            Condition::RingBearerChosen(filter) => {
                if !self.event.ring_choice {
                    return Err("Ring-bearer choice requires a matched Ring temptation".into());
                }
                self.filter(filter)
            }
            Condition::ObjectMatches { object, filter } => {
                self.object(object)?;
                self.filter(filter)
            }
            Condition::Controls { player, filter, .. } => {
                self.player(player)?;
                self.filter(filter)
            }
        }
    }

    pub(super) fn activation_filter(&self, filter: &ActivationFilter) -> Result {
        match filter {
            ActivationFilter::Is(_)
            | ActivationFilter::Any
            | ActivationFilter::Mana
            | ActivationFilter::Cycling => Ok(()),
            ActivationFilter::Of(sources) => self.filter(sources),
            ActivationFilter::Not(f) => self.activation_filter(f),
            ActivationFilter::All(v) | ActivationFilter::Either(v) => {
                for f in v {
                    self.activation_filter(f)?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn defenders(&self, defenders: &AttackDefenders) -> Result {
        match defenders {
            AttackDefenders::Any => Ok(()),
            AttackDefenders::Objects(f) => self.filter(f),
            AttackDefenders::Players(p)
            | AttackDefenders::PlayersAndPlaneswalkers(p)
            | AttackDefenders::DefendingPlayers(p) => self.players(p),
        }
    }

    pub(super) fn object_action(&self, action: &ObjectAction) -> Result {
        match action {
            ObjectAction::Attack(d) => self.defenders(d),
            ObjectAction::Block(f) | ObjectAction::BeBlocked(f) => self.filter(f),
            ObjectAction::Activate(f) => self.activation_filter(f),
            ObjectAction::BeTargeted(f) => self.filter(f),
            ObjectAction::BeAttachedBy { attachments, .. } => self.filter(attachments),
            ObjectAction::PhaseIn
            | ObjectAction::BePlayed
            | ObjectAction::TurnFaceUp
            | ObjectAction::BecomeSuspected
            | ObjectAction::BeBeamedUp
            | ObjectAction::ChangeControl
            | ObjectAction::LoseCounters(_)
            | ObjectAction::BeAttacked
            | ObjectAction::Untap
            | ObjectAction::Cast
            | ObjectAction::Regenerate
            | ObjectAction::Sacrifice
            | ObjectAction::Counter
            | ObjectAction::ReceiveCounters(_)
            | ObjectAction::Transform
            | ObjectAction::PhaseOut
            | ObjectAction::Crew
            | ObjectAction::Copy => Ok(()),
        }
    }

    pub(super) fn combat_action(&self, action: &ObjectAction) -> Result {
        if !matches!(
            action,
            ObjectAction::Attack(_) | ObjectAction::Block(_) | ObjectAction::BeBlocked(_)
        ) {
            return Err("combat requirement/cost must describe attacking or blocking".into());
        }
        self.object_action(action)
    }

    pub(super) fn rule(&self, rule: &RuleChange) -> Result {
        match rule {
            RuleChange::Primitive { name, operands } => self.primitive(name, operands),
            RuleChange::Prohibit(action) => self.object_action(action),
            RuleChange::GoadedBy(player) => self.player(player),
            RuleChange::CantAttackDefenderIf(condition) => self.condition(condition),
            RuleChange::TapPowerBonus { bonus, .. } => self.expr(bonus),
            RuleChange::MayExert(then) => then.iter().try_for_each(|o| self.operand(o)),
            RuleChange::BlockedAsThoughLacking(ability) | RuleChange::CantGainAbility(ability) => {
                self.operand(ability)
            }
            RuleChange::DamageStaysMarked | RuleChange::LegendRuleExempt => Ok(()),
            RuleChange::TargetableDespite { sources: f, .. }
            | RuleChange::AttachOnlyTo(f)
            | RuleChange::AttackDrags(f)
            | RuleChange::AttacksWhenAttacking(f)
            | RuleChange::BlocksDespite { attackers: f, .. } => self.filter(f),
            RuleChange::BlocksOnlyIf(condition) => self.condition(condition),
            RuleChange::BlockedByDefender { players, .. } => self.players(players),
            RuleChange::TapCostUsesToughness(_) | RuleChange::Unique(_) => Ok(()),
            RuleChange::CounterLimit { max, .. } => self.expr(max),
            RuleChange::ActivationsEachTurn { abilities, times } => {
                self.filter(abilities)?;
                self.expr(times)
            }
            RuleChange::OwnActivationCost { abilities, change } => {
                self.filter(abilities)?;
                self.activation_cost_change(change)
            }
            RuleChange::AssignDamageTo {
                players,
                permanents,
            } => {
                players.iter().try_for_each(|p| self.player(p))?;
                self.filter(permanents)
            }
            RuleChange::RequireCombat { action, amount } => {
                self.combat_action(action)?;
                if let RequirementAmount::AtLeast(n) = amount {
                    self.count(&Count::AtLeast(n.clone()))?;
                }
                Ok(())
            }
            RuleChange::CombatCost {
                action,
                payer,
                cost,
            } => {
                self.combat_action(action)?;
                let mut scope = self.clone();
                scope.declaration = true;
                scope.player(payer)?;
                scope.cost(cost)
            }
            RuleChange::CombatGroup {
                participants,
                amount,
                ..
            } => {
                self.filter(participants)?;
                self.expr(amount)
            }
            RuleChange::AttackException { defenders, .. } => self.defenders(defenders),
            RuleChange::BlockWhileTapped | RuleChange::CombatDamage(_) => Ok(()),
            RuleChange::BlockCapacity(BlockCapacity::Unlimited) => Ok(()),
            RuleChange::BlockCapacity(BlockCapacity::Additional(n)) => {
                self.count(&Count::Exactly(n.clone()))
            }
            RuleChange::UntapStep { players, .. } => self.players(players),
        }
    }

    pub(super) fn change(&self, change: &Change) -> Result {
        match change {
            Change::Continuing { changes, until } => {
                self.duration(until)?;
                changes.iter().try_for_each(|c| self.change(c))
            }
            // Copiable text is the object's own, so it may define characteristics.
            Change::Copiable(changes) => changes.iter().try_for_each(|c| match c {
                Change::GrantAbility(ability) => validate_ability(ability),
                _ => self.change(c),
            }),
            Change::Primitive { name, operands } => self.primitive(name, operands),
            Change::GrantKeyword(keyword) => self.keyword(keyword),
            Change::PlayerRule(rule) => self.player_rule(rule),
            Change::Rule(rule) => self.rule(rule),
            Change::SetController(player) => self.player(player),
            Change::SetPower(n) | Change::SetToughness(n) => self.expr(n),
            Change::Pump { power, toughness }
            | Change::SetBasePowerToughness { power, toughness } => {
                self.expr(power)?;
                self.expr(toughness)
            }
            Change::ColorOfMana { mana, .. }
                if matches!(self.results.get(&mana.0), Some(ResultKind::Mana)) =>
            {
                Ok(())
            }
            Change::ColorOfMana { mana, .. } => Err(format!(
                "result {} does not provide produced mana in this scope",
                mana.0
            )),
            Change::GrantAbility(ability) => {
                if has_definition(ability) {
                    return Err(
                        "a granted ability is not an intrinsic characteristic definition".into(),
                    );
                }
                validate_ability(ability)
            }
            Change::Colors { colors, .. }
                if colors.iter().any(|c| matches!(c, ManaColor::Colorless)) =>
            {
                Err("colorless is an empty color set, not a color".into())
            }
            Change::Colors { .. }
            | Change::ChosenColor { .. }
            | Change::ChosenAbility
            | Change::CreatureTypes { .. }
            | Change::AddCardType(_)
            | Change::RemoveCardType(_)
            | Change::AddSupertype(_)
            | Change::RemoveSupertype(_)
            | Change::AddLandType(_)
            | Change::AddArtifactType(_)
            | Change::SetArtifactType(_)
            | Change::SetCardTypes(_)
            | Change::RemoveAllAbilities
            | Change::ChosenType { .. }
            | Change::SwitchPowerToughness => Ok(()),
            Change::EntersWithCounters { amount, .. } => self.expr(amount),
            Change::SpellCost(modifier) => self.spell_cost_modifier(modifier),
            Change::CantBeCountered => Ok(()),
            Change::ActivationRule(_) => {
                Err("an activation rule belongs to an activated ability".into())
            }
            Change::SetName(_)
            | Change::LandTypes { .. }
            | Change::RemoveAllLandTypes
            | Change::RemoveSubtype(_)
            | Change::ColorlessDamageSource
            | Change::Incorporate(_) => Ok(()),
            Change::AddNames(filter) => self.filter(filter),
            Change::CreatureTypesOf { object, .. } => self.object(object),
            Change::TextOf { object, plus } => {
                self.object(object)?;
                self.operand(plus)
            }
            Change::GrantAbilityIfAbsent(ability) => validate_ability(ability),
            Change::GainAbilitiesOf { objects, kinds, .. } => {
                self.objects(objects)?;
                self.operand(kinds)
            }
            Change::ReplaceWord { from, to } => {
                self.operand(from)?;
                self.operand(to)
            }
            Change::AddEnchantmentType(operand)
            | Change::LosesAbility(operand)
            | Change::SetLandType(operand) => self.operand(operand),
            Change::CopyOf { object, exceptions } => {
                self.object(object)?;
                self.copy_exceptions(exceptions)
            }
        }
    }
}
