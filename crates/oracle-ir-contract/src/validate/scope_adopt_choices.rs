//! Scoped validation of portable operands.
use super::*;
impl<'a> Scope<'a> {
    /// A player loop's picks and moves stay readable as a set, never as one
    /// object; anything else it did can still be asked whether it happened.
    pub(super) fn adopt_choices(&mut self, body: &Program, run: &Self) {
        for instruction in &body.instructions {
            match instruction {
                Instruction::Effect {
                    result,
                    operation: Operation::ChooseObjects { .. },
                } => {
                    self.results.insert(result.0, ResultKind::Selection);
                    self.gathered.insert(result.0);
                    if let Some(zones) = run.result_zones.get(&result.0) {
                        self.result_zones.insert(result.0, zones.clone());
                    }
                }
                Instruction::Effect { result, operation } => {
                    let operation = operation.action();
                    if run.ordered_results.contains(&result.0) {
                        self.ordered_results.insert(result.0);
                    }
                    let moved = matches!(
                        operation,
                        Operation::Move { .. }
                            | Operation::Destroy { .. }
                            | Operation::Sacrifice { .. }
                            | Operation::Discard { .. }
                            | Operation::DiscardObjects { .. }
                            | Operation::DiscardAllBut { .. }
                            | Operation::Mill { .. }
                            | Operation::Draw { .. }
                            | Operation::DrawUpTo { .. }
                    );
                    self.results.insert(
                        result.0,
                        if moved {
                            ResultKind::ObjectTransition
                        } else if matches!(
                            operation,
                            Operation::Reveal { .. } | Operation::RevealUntil { .. }
                        ) {
                            ResultKind::Observation
                        } else if matches!(operation, Operation::CreateTokens { .. }) {
                            ResultKind::CreatedObjects
                        } else if crate::semantics::makes_card(operation) {
                            ResultKind::ObjectTransition
                        } else if matches!(operation, Operation::AddMana { .. }) {
                            ResultKind::Mana
                        } else if matches!(
                            operation,
                            Operation::FlipCoin { .. }
                                | Operation::FlipCoins { .. }
                                | Operation::FlipCoinSeries { .. }
                        ) {
                            // Iteration gathers coins into a collection; it cannot
                            // expose one arbitrarily chosen scalar flip.
                            ResultKind::CoinBatch
                        } else {
                            ResultKind::Other
                        },
                    );
                    self.gathered.insert(result.0);
                }
                Instruction::Group(body)
                | Instruction::Optional { body, .. }
                | Instruction::ForEachPlayer { body, .. } => self.adopt_choices(body, run),
                _ => {}
            }
        }
    }

    pub(super) fn branch_receipts(&self, child: &Self) -> BTreeSet<u32> {
        child
            .results
            .keys()
            .copied()
            .filter(|id| !self.results.contains_key(id))
            .chain(
                child
                    .join_groups
                    .iter()
                    .skip(self.join_groups.len())
                    .flatten()
                    .copied(),
            )
            .collect()
    }

    pub(super) fn program(&mut self, program: &Program) -> Result {
        for (i, instruction) in program.instructions.iter().enumerate() {
            let checked: Result = (|| {
                match instruction {
                    Instruction::Effect { result, operation } => {
                        self.effect_instruction(result, operation)?;
                    }
                    Instruction::Group(program) => self.program(program)?,
                    Instruction::If {
                        condition,
                        then,
                        otherwise,
                    } => {
                        self.condition(condition)?;
                        let mut yes = self.clone();
                        yes.program(then)?;
                        let mut no = self.clone();
                        no.program(otherwise)?;
                        let mut joined = self.branch_receipts(&yes);
                        joined.extend(self.branch_receipts(&no));
                        self.join_groups.push(joined);

                        self.open_searches = yes.open_searches;
                        for p in no.open_searches {
                            if !self
                                .open_searches
                                .iter()
                                .any(|q| crate::semantics::same_player(&p, q))
                            {
                                self.open_searches.push(p);
                            }
                        }
                        // If no search occurred there was no open search; if it
                        // did, this exact guard shuffles and closes that search.
                        if let Condition::Searched(result) = condition
                            && let (
                                Some(library),
                                [
                                    Instruction::Effect {
                                        operation:
                                            Operation::Shuffle {
                                                players: PlayerSet::One(shuffled),
                                                ..
                                            },
                                        ..
                                    },
                                ],
                            ) = (
                                self.search_libraries.get(&result.0),
                                then.instructions.as_slice(),
                            )
                            && otherwise.instructions.is_empty()
                            && crate::semantics::same_player(library, shuffled)
                        {
                            self.open_searches
                                .retain(|p| !crate::semantics::same_player(p, library));
                        }
                    }
                    Instruction::ForEachObject {
                        objects,
                        binding,
                        body,
                    } => {
                        self.objects(objects)?;
                        let mut iterated = self.clone();
                        iterated.bound_objects.insert(binding.0);
                        iterated.program(body)?;
                        self.adopt_choices(body, &iterated);
                    }
                    Instruction::RepeatableWithLimit { amount, body } => {
                        self.expr(amount)?;
                        let mut repeated = self.clone();
                        repeated.repeatable = true;
                        repeated.program(body)?;
                    }
                    Instruction::Repeatable(body) => {
                        let mut repeated = self.clone();
                        repeated.repeatable = true;
                        repeated.program(body)?;
                        self.adopt_choices(body, &repeated);
                    }
                    Instruction::ForEachPlayer {
                        players,
                        binding,
                        starting_with,
                        body,
                        ..
                    } => {
                        self.players(players)?;
                        if let Some(player) = starting_with {
                            self.player(player)?;
                        }
                        let mut each = self.clone();
                        if !each.players.insert(binding.0) {
                            return Err(format!("duplicate player binding {}", binding.0));
                        }
                        each.program(body)?;
                        self.adopt_choices(body, &each);
                    }
                    Instruction::ChooseOne {
                        decider,
                        options,
                        offered,
                    } => {
                        self.player(decider)?;
                        if options.len() < 2 {
                            return Err("a choice needs at least two options".into());
                        }
                        if let Some(offered) = offered {
                            self.count(&Count::Exactly(offered.clone()))?;
                        }
                        let mut joined = BTreeSet::new();
                        for option in options {
                            let mut child = self.clone();
                            child.program(option)?;
                            joined.extend(self.branch_receipts(&child));
                        }
                        self.join_groups.push(joined);
                    }
                    Instruction::Optional { decider, body } => {
                        self.player(decider)?;
                        let mut yes = self.clone();
                        yes.program(body)?;
                        self.adopt_moves(body);
                        for p in yes.open_searches {
                            if !self
                                .open_searches
                                .iter()
                                .any(|q| crate::semantics::same_player(&p, q))
                            {
                                self.open_searches.push(p);
                            }
                        }
                    }
                }
                Ok(())
            })();
            checked.map_err(|e| format!("instructions[{i}]: {e}"))?;
        }
        Ok(())
    }
    fn effect_instruction(&mut self, result: &ResultId, operation: &Operation) -> Result {
        self.operation(operation)?;
        if let Operation::PayCost { cost, .. } = operation {
            for part in &cost.parts {
                if let CostPart::PlayerAction { body, .. } = part {
                    self.adopt_moves(body);
                }
            }
        }
        let kind = match operation.action() {
                            Operation::LookAt { .. }
                            | Operation::Reveal { .. }
                            | Operation::RevealUntil { .. }
                            | Operation::RevealUntilGroup { .. } => ResultKind::Observation,
                            Operation::Surveil { .. } => ResultKind::Surveil,
                            Operation::Search { .. } => ResultKind::Search,
                            Operation::PayCost { .. } | Operation::PayRepeatedly { .. } => {
                                ResultKind::Payment
                            }
                            Operation::CreateTokens { .. } => ResultKind::CreatedObjects,
                            Operation::AddMana { .. } => ResultKind::Mana,
                            Operation::FlipCoin { .. } => ResultKind::Coin,
                            Operation::FlipCoins { .. } | Operation::FlipCoinSeries { .. } => {
                                ResultKind::CoinBatch
                            }
                            Operation::ChooseObjects { .. }
                            | Operation::SeparatePiles { .. }
                            | Operation::NoteObjects { .. }
                            // Bolster chooses the creature it counters (701.39a).
                            | Operation::Keyword {
                                action: KeywordAction::Bolster { .. },
                                ..
                            } => ResultKind::Selection,
                            Operation::ReorderLibrary { .. }
                            | Operation::Destroy { .. }
                            | Operation::Move { .. }
                            | Operation::Counter { .. }
                            | Operation::SetTapped { .. }
                            | Operation::Discard { .. }
                            | Operation::DiscardObjects { .. }
                            | Operation::Sacrifice { .. }
                            | Operation::JoinResults { .. }
                            | Operation::Draw { .. }
                            | Operation::Mill { .. } => ResultKind::ObjectTransition,
                            other if crate::semantics::makes_card(other) => {
                                ResultKind::ObjectTransition
                            }
                            other => match crate::semantics::yields(other) {
                                Some("observed") => ResultKind::Observation,
                                Some("extra_turn") => ResultKind::Other,
                                Some("created") => ResultKind::CreatedObjects,
                                Some("paid" | "took") => ResultKind::Other,
                                Some("chosen") => ResultKind::Selection,
                                Some(_) => ResultKind::ObjectTransition,
                                None => ResultKind::Other,
                            },
                        };
        let zones = match operation.action() {
            Operation::LookAt { objects, .. } | Operation::Reveal { objects } => {
                self.known_zones(objects)
            }
            Operation::Search { .. }
            | Operation::ReorderLibrary { .. }
            | Operation::RevealUntil { .. }
            | Operation::RevealUntilGroup { .. } => Some(vec![EventZone::Library]),
            Operation::Draw { .. } => Some(vec![EventZone::Hand]),
            Operation::ChooseObjects { objects, .. } | Operation::SeparatePiles { objects, .. } => {
                self.known_zones(objects)
            }
            _ => None,
        };
        if let Operation::PayCost { cost, .. } = operation {
            self.payment_costs.insert(result.0, cost.clone());
        }
        if crate::semantics::singular_operation(operation, |r| self.singular.contains(&r.0)) {
            self.singular.insert(result.0);
        }
        if self.results.insert(result.0, kind).is_some() {
            return Err(format!("duplicate result {}", result.0));
        }
        if matches!(
            operation,
            Operation::RevealUntil { .. } | Operation::RevealUntilGroup { .. }
        ) {
            self.ordered_results.insert(result.0);
        }
        if let Operation::LookAt { objects, .. }
        | Operation::Reveal { objects }
        | Operation::Move { objects, .. } = operation
            && self.ordered(objects)
        {
            self.ordered_results.insert(result.0);
        }
        if let Some(zones) = zones {
            self.result_zones.insert(result.0, zones);
        }
        match operation {
            Operation::Search {
                library_owner: library,
                ..
            } => {
                self.open_searches.push(library.clone());
                self.search_libraries.insert(result.0, library.clone());
            }
            Operation::Shuffle { players, .. } => self
                .open_searches
                .retain(|p| !crate::semantics::shuffle_closes(p, players)),
            _ => {}
        }

        Ok(())
    }

    pub(super) fn announcement(&mut self, announcement: &'a Announcement) -> Result {
        let mut proposal = self.clone();
        proposal.announcing = true;
        proposal.payment_costs.clear();
        for target in &announcement.targets {
            match target.selector()? {
                crate::targets::TargetSelector::Objects { domain, filter } => {
                    proposal.domain(&domain)?;
                    proposal.filter(&filter)?;
                }
                crate::targets::TargetSelector::Players { filter } => {
                    if let Some(players) = filter {
                        proposal.players(&players)?;
                    }
                }
                crate::targets::TargetSelector::Damageable { objects, players } => {
                    proposal.filter(&objects)?;
                    if let Some(players) = players {
                        proposal.players(&players)?;
                    }
                }
            }
            proposal.count(&target.count)?;
            proposal.selection_constraints(&target.constraints)?;
            for constraint in &target.constraints {
                if let SelectionConstraint::DifferentFromTarget(slot)
                | SelectionConstraint::SharesLimitWith(slot) = constraint
                    && !self.slots.contains_key(&slot.0)
                {
                    return Err(format!("target excludes unannounced slot {}", slot.0));
                }
            }
            if let Some(chooser) = &target.chooser {
                proposal.player(chooser)?;
            }
            if let Some(total) = &target.distribute {
                proposal.expr(total)?;
            }
            if self.slots.insert(target.slot.0, target).is_some() {
                return Err(format!("duplicate target slot {}", target.slot.0));
            }
            proposal.slots.insert(target.slot.0, target);
        }
        if let Some(selection) = &announcement.modes {
            let selection = crate::mode_policies::parts(selection);
            if let Some(ModeHistory::LastCombat(player)) = selection.unchosen {
                proposal.player(player)?;
            }
            proposal.expr(selection.minimum)?;
            proposal.expr(selection.maximum)?;
            if let Some(cost) = &selection.surcharge {
                proposal.cost(cost)?;
            }
            if let Some(players) = &selection.chooser {
                proposal.players(players)?;
            }
            if let Some(budget) = &selection.budget {
                proposal.expr(budget)?;
            }
            bounds::interval(selection.minimum, selection.maximum)
                .map_err(|error| format!("invalid mode bounds: {error}"))?;
            if let (Expr::Constant(min), Expr::Constant(max)) =
                (&selection.minimum, &selection.maximum)
                && (*min < 0
                    || *max < *min
                    || (!selection.repeat && *max > selection.modes.len() as i64))
            {
                return Err("invalid mode bounds".into());
            }
            for (i, mode) in selection.modes.iter().enumerate() {
                if let Some(condition) = &mode.requires {
                    proposal.condition(condition)?;
                }
                let mut scope = self.clone();
                scope.join_groups.clear();
                scope
                    .announcement(&mode.announcement)
                    .map_err(|e| format!("modes[{i}]: {e}"))?;
                scope
                    .program(&mode.body)
                    .map_err(|e| format!("modes[{i}]: {e}"))?;
            }
        }
        Ok(())
    }
}
