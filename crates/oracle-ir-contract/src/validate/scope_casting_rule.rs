//! Scoped validation of portable operands.
use super::*;
impl<'a> Scope<'a> {
    pub(super) fn casting_rule(&self, rule: &CastingRule) -> Result {
        match rule {
            CastingRule::FlashWithRider(rider) => self.operand(rider),
            CastingRule::TaxPaidAsLife(life) => self.expr(life),
            CastingRule::Permission {
                origin,
                player,
                from,
                require,
                cost,
                leave_stack,
            } => {
                self.player(player)?;
                self.zone_scope(from)?;
                self.filter(require)?;
                self.cost(cost)?;
                if let Some(destination) = leave_stack {
                    self.destination(destination)?;
                }
                if matches!(origin, CastingOrigin::Flashback) {
                    let owner = PlayerRef::OwnerOf(Box::new(ObjectRef::Source));
                    if !crate::semantics::same_player(player, &owner)
                        || !matches!(from,ZoneScope::Players {players:PlayerSet::One(p),..} if crate::semantics::same_player(p,&owner))
                    {
                        return Err(
                            "flashback permission belongs to the owner and their graveyard".into(),
                        );
                    }
                    let expected = ObjectFilter::AnyOf(vec![
                        ObjectFilter::CardType(crate::mtg_types::CardType::Instant),
                        ObjectFilter::CardType(crate::mtg_types::CardType::Sorcery),
                    ]);
                    if !matches!(
                        from,
                        ZoneScope::Players {
                            zone: EventZone::Graveyard,
                            ..
                        }
                    ) || !matches!(leave_stack, Some(Destination::Exile))
                        || serde_json::to_value(require).unwrap()
                            != serde_json::to_value(expected).unwrap()
                    {
                        return Err("flashback requires graveyard permission, an instant/sorcery result, and its exile replacement".into());
                    }
                }
                Ok(())
            }
            CastingRule::AdditionalCost { cost, .. }
            | CastingRule::AlternativeCost(cost)
            | CastingRule::AlternativeCostWithFlash(cost) => self.cost(cost),
            CastingRule::ReduceForEachPaid { cost, .. } => self.cost(cost),
            CastingRule::ReduceCost(reductions) => {
                for reduction in reductions {
                    self.count(&Count::Exactly(reduction.amount.clone()))?;
                }
                Ok(())
            }
            CastingRule::Require(condition) => self.condition(condition),
            CastingRule::Flash { additional_cost } => {
                if let Some(cost) = additional_cost {
                    self.cost(cost)?;
                }
                Ok(())
            }
            CastingRule::ProhibitTargets { domain, filter } => {
                self.domain(domain)?;
                self.filter(filter)
            }
            CastingRule::RestrictMana { filter, .. } => match filter {
                ManaPaymentFilter::None => Ok(()),
                ManaPaymentFilter::Colors(colors) if colors.is_empty() => {
                    Err("mana payment colors cannot be empty".into())
                }
                ManaPaymentFilter::Colors(_) => Ok(()),
                ManaPaymentFilter::ProducedBy(filter) => self.filter(filter),
            },
            CastingRule::SpendAsThoughAnyType => Ok(()),
            CastingRule::If { condition, rule } => {
                self.condition(condition)?;
                self.casting_rule(rule)
            }
        }
    }

    pub(super) fn destination(&self, destination: &Destination) -> Result {
        if let Destination::OwnersLibraryFromTop(depth) = destination {
            self.expr(depth)?;
        }
        if let Destination::Battlefield(player) | Destination::Hand(player) = destination {
            self.player(player)?;
        }
        Ok(())
    }

    pub(super) fn primitive(&self, name: &str, operands: &[Operand]) -> Result {
        if name == "Condition::AllCoinsCameUpHeads" {
            return Err("all-coin heads check requires explicit participants".into());
        }
        if crate::semantics::requires_typed_keyword(name) {
            return Err(format!("{name} requires a typed keyword action"));
        }
        if crate::copying::requires_typed_payload(name) {
            return Err(format!("{name} requires typed copy exceptions"));
        }
        if crate::source_values::requires_typed_payload(name) {
            return Err(format!("{name} requires typed face-down characteristics"));
        }
        if crate::primitive_spec::lookup(name).is_none() {
            return Err(format!("{name} is not a reviewed primitive"));
        }
        let mut scoped = self.clone();
        if crate::primitive_spec::lookup(name).is_some_and(|row| !row.binds.is_empty()) {
            scoped.event = crate::semantics::with_row(&self.event, name);
        }
        if let Some(event) = operands.iter().find_map(|o| match o {
            Operand::Event(event) => Some(event),
            _ => None,
        }) {
            scoped.event = crate::semantics::event_bindings(event);
        }
        operands.iter().try_for_each(|o| scoped.operand(o))
    }

    pub(super) fn named(&self, named: &Named) -> Result {
        named.operands.iter().try_for_each(|o| self.operand(o))
    }

    /// A row's form says what its own "each" ranges over.
    pub(super) fn operand(&self, operand: &Operand) -> Result {
        let checked = self.bare_operand(operand);
        if checked.is_err() && !self.affected {
            let mut each = self.clone();
            each.affected = true;
            return each.bare_operand(operand);
        }
        checked
    }

    pub(super) fn bare_operand(&self, operand: &Operand) -> Result {
        match operand {
            Operand::Absent | Operand::Bool(_) | Operand::Integer(_) | Operand::Text(_) => Ok(()),
            Operand::List(items) => items.iter().try_for_each(|o| self.operand(o)),
            Operand::Record(fields) => fields.iter().try_for_each(|f| self.operand(&f.value)),
            Operand::Form(named) => self.named(named),
            Operand::Vocabulary { form, .. } | Operand::Term { form, .. }
                if crate::complete::reads_scope(form) =>
            {
                Err(format!("{form} names something in scope"))
            }
            Operand::Vocabulary { .. } => Ok(()),
            Operand::Term { operands, .. } => operands.iter().try_for_each(|o| self.operand(o)),
            Operand::Object(object) => self.object(object),
            Operand::Objects(objects) => self.objects(objects),
            Operand::Recipients(recipients) => self.recipients(recipients),
            Operand::Player(player) => self.player(player),
            Operand::Players(players) => self.players(players),
            Operand::Number(expression) => self.expr(expression),
            Operand::Condition(condition) => self.condition(condition),
            Operand::Duration(duration) => self.duration(duration),
            Operand::Filter(filter) => self.filter(filter),
            Operand::Comparison(_, expression) => self.expr(expression),
            Operand::NumberTest(test) => self.number_test(test),
            Operand::Stickers(stickers) => {
                for sticker in stickers {
                    if let Sticker::Abilities { abilities, .. } = sticker {
                        for ability in abilities {
                            validate_ability_in(ability, None, &BTreeMap::new())?;
                        }
                    }
                }
                Ok(())
            }
            Operand::Cost(cost) => self.cost(cost),
            // A nested body closes over this scope, targets included.
            Operand::Ability(ability) => match &**ability {
                Ability::Spell { announcement, body } => {
                    let mut child = self.clone();
                    child.replacement = false;
                    child.open_searches.clear();
                    child.join_groups.clear();
                    child.announcement(announcement)?;
                    child.program(body)
                }
                other => validate_ability(other),
            },
            Operand::Event(event) => self.event(event),
            Operand::Result(result) if self.results.contains_key(&result.0) => Ok(()),
            Operand::Result(result) => Err(format!("result {} has not run", result.0)),
            Operand::Program(program) => {
                let mut child = self.clone();
                child.open_searches.clear();
                child.join_groups.clear();
                child.program(program)
            }
        }
    }

    pub(super) fn new_targets(&self, targets: &NewTargets) -> Result {
        match targets {
            NewTargets::To(objects) | NewTargets::Each(objects) => self.objects(objects),
            NewTargets::ToPlayers(players) | NewTargets::EachPlayer(players) => {
                self.players(players)
            }
            NewTargets::Random(except) => except.iter().try_for_each(|o| self.operand(o)),
            NewTargets::Same | NewTargets::May | NewTargets::Must | NewTargets::EachLegal => Ok(()),
        }
    }

    #[inline]
    pub(super) fn operation(&self, operation: &Operation) -> Result {
        match operation {
            Operation::InOrder(_)
            | Operation::WithSelections {
                steps: _,
                action: _,
            }
            | Operation::Meld { .. }
            | Operation::PutStickers { .. }
            | Operation::Piles { .. }
            | Operation::ChoosePile { .. }
            | Operation::Note(_)
            | Operation::Bid {
                players: _,
                start: _,
            }
            | Operation::Guess {
                actor: _,
                object: _,
                filter: _,
                player: _,
            }
            | Operation::Seek {
                actor: _,
                cards: _,
                count: _,
                top: _,
            }
            | Operation::DrawThenDiscard {
                players: _,
                draw: _,
                discard: _,
            }
            | Operation::RedistributeLife(_)
            | Operation::WithReflexive { .. } => self.operation_case_0(operation),
            _ => self.operation_dispatch_1(operation),
        }
    }

    #[inline]
    pub(super) fn operation_dispatch_1(&self, operation: &Operation) -> Result {
        match operation {
            Operation::GiveControl { .. }
            | Operation::AnyPlayerMayPay { .. }
            | Operation::EachPlayer {
                players: _,
                action: _,
            }
            | Operation::EachObject {
                objects: _,
                action: _,
            }
            | Operation::LimitX { test: _ }
            | Operation::RestartGame { keep: _, then: _ }
            | Operation::EachNumber {
                from: _,
                to: _,
                action: _,
            }
            | Operation::Counter {
                objects: _,
                destination_when: _,
                destination: _,
            }
            | Operation::Populate { actor: _, entry: _ }
            | Operation::ReduceSpeed {
                player: _,
                amount: _,
                floor: _,
            }
            | Operation::PutCountersCapped { .. }
            | Operation::MillUntil {
                actor: _,
                cards: _,
                at_most: _,
            }
            | Operation::ChooseDefender {
                actor: _,
                attacker: _,
            }
            | Operation::PlayersPayRepeatedly {
                players: _,
                cost: _,
                at_most: _,
            }
            | Operation::DelayedUnlessPaid { .. }
            | Operation::FightEach {
                fighters: _,
                others: _,
            }
            | Operation::PickFromHand { .. }
            | Operation::DistributeCounters { .. } => self.operation_case_1(operation),
            _ => self.operation_dispatch_2(operation),
        }
    }

    #[inline]
    pub(super) fn operation_dispatch_2(&self, operation: &Operation) -> Result {
        match operation {
            Operation::PutCountersEach { .. }
            | Operation::PutChosenCounters {
                objects: _,
                choice: _,
            }
            | Operation::NoteObjects { objects: _ }
            | Operation::Named { actor: _, form: _ }
            | Operation::Primitive {
                actor: _,
                name: _,
                operands: _,
            }
            | Operation::CreatePerpetualEffect {
                objects: _,
                changes: _,
            }
            | Operation::KeywordEach {
                actor: _,
                objects: _,
                action: _,
            }
            | Operation::MoveTopCard { .. }
            | Operation::PayAnyAmount { .. }
            | Operation::RevealFromHand { .. }
            | Operation::CopyOnStack {
                actor: _,
                object: _,
                times: _,
                targets: _,
                exceptions: _,
                card: _,
            }
            | Operation::Retarget {
                actor: _,
                object: _,
                targets: _,
            }
            | Operation::AbandonScheme { actor: _ }
            | Operation::Together(_)
            | Operation::Repeat { times: _, body: _ } => self.operation_case_2(operation),
            _ => self.operation_dispatch_3(operation),
        }
    }

    #[inline]
    pub(super) fn operation_dispatch_3(&self, operation: &Operation) -> Result {
        match operation {
            Operation::TurnFaceDownAs {
                objects: _,
                as_creature: _,
            }
            | Operation::Exchange { .. }
            | Operation::ChooseValue { .. }
            | Operation::RevealChoices { .. }
            | Operation::FlipCoinSeries { .. }
            | Operation::AttachEach { .. }
            | Operation::CopyEach {
                actor: _,
                objects: _,
                targets: _,
            }
            | Operation::RetargetEach {
                actor: _,
                objects: _,
                targets: _,
            }
            | Operation::TakeSpell {
                actor: _,
                object: _,
                targets: _,
            }
            | Operation::SetX {
                object: _,
                value: _,
            }
            | Operation::ChangeCombat { change: _ } => self.operation_case_3(operation),
            _ => self.operation_dispatch_4(operation),
        }
    }

    #[inline]
    pub(super) fn operation_dispatch_4(&self, operation: &Operation) -> Result {
        match operation {
            Operation::PlayersDo { .. }
            | Operation::CreateBoon {
                actor: _,
                event: _,
                condition: _,
                times: _,
                body: _,
            }
            | Operation::AddPhases { .. }
            | Operation::ChooseAndPay {
                actor: _,
                objects: _,
                cost: _,
            }
            | Operation::DuplicateCounters { .. }
            | Operation::EffectOn {
                subject: _,
                effects: _,
                until: _,
            }
            | Operation::GameEffect {
                until: _,
                effect: _,
            }
            | Operation::AttachChosen {
                actor: _,
                attachments: _,
                count: _,
                hosts: _,
            }
            | Operation::DrawUpTo {
                players: _,
                amount: _,
            }
            | Operation::DiscardAllBut {
                players: _,
                keep: _,
            }
            | Operation::SacrificeAllBut {
                players: _,
                objects: _,
                keep: _,
            } => self.operation_case_4(operation),
            _ => self.operation_dispatch_5(operation),
        }
    }

    #[inline]
    pub(super) fn operation_dispatch_5(&self, operation: &Operation) -> Result {
        match operation {
            Operation::TowardPlayer { .. }
            | Operation::Intensify { cards: _, owner: _ }
            | Operation::FlipCoins { .. }
            | Operation::Wish { .. }
            | Operation::PutSticker { .. }
            | Operation::BeginOnBattlefield { actor: _, flags: _ }
            | Operation::Manifest { .. }
            | Operation::Vote { .. }
            | Operation::Clash {
                actor: _,
                opponents: _,
                won: _,
                lost: _,
            }
            | Operation::DraftFromSpellbook { .. }
            | Operation::CreateEmblem {
                actor: _,
                abilities: _,
            }
            | Operation::MoveCounters { .. }
            | Operation::EachCounterKind { .. }
            | Operation::DoubleCounters { .. } => self.operation_case_5(operation),
            _ => self.operation_dispatch_6(operation),
        }
    }
}
