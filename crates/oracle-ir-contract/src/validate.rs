//! Validate semantic data after lowering and after loading a normalized export.
//! A successful deserialize alone does not prove reference or instruction scope.

use crate::ir::*;
use crate::movement::legacy::{EntrySpec, EntryState};
use std::collections::{BTreeMap, BTreeSet};

mod arrival;
mod bounds;
mod copying;
mod layout;

/// The slot in `casting_costs` that holds the enclosing cost branch's cost.
const BRANCH_COST: u32 = u32::MAX;

#[derive(Clone, Copy)]
enum ResultKind {
    Observation,
    Surveil,
    Search,
    Payment,
    Selection,
    ObjectTransition,
    CreatedObjects,
    Mana,
    Coin,
    CoinBatch,
    Other,
}

type Result<T = ()> = std::result::Result<T, String>;

#[derive(Clone, Default)]
struct Scope<'a> {
    slots: BTreeMap<u16, &'a TargetSpec>,
    prepared: BTreeMap<u32, Option<Vec<EventZone>>>,
    results: BTreeMap<u32, ResultKind>,
    /// Receipts visible only to an explicit join after mutually exclusive branches.
    join_groups: Vec<BTreeSet<u32>>,
    result_zones: BTreeMap<u32, Vec<EventZone>>,
    activation_cost: Option<Cost>,
    announcing: bool,
    casting_costs: BTreeMap<u32, Cost>,
    payment_costs: BTreeMap<u32, Cost>,
    ordered_results: BTreeSet<u32>,
    bound_objects: BTreeSet<u32>,
    singular: BTreeSet<u32>,
    /// Picks gathered over a player loop: a set, never one object.
    gathered: BTreeSet<u32>,
    repeatable: bool,
    open_searches: Vec<PlayerRef>,
    /// Inside `Operation::Clash`, where `PlayerRef::ClashOpponent` has a value.
    clashing: bool,
    search_libraries: BTreeMap<u32, PlayerRef>,
    event: crate::semantics::EventBindings,
    affected: bool,
    affected_player: bool,
    replacement: bool,
    players: BTreeSet<u32>,
    declaration: bool,
    candidate: bool,
    ward_payer: bool,
    keyword_host: bool,
    definition: bool,
}

pub fn validate_ability(ability: &Ability) -> Result {
    validate_ability_in(ability, None, &BTreeMap::new())
}
// Crew identity carries rules about participants at payment and becoming crewed
// at resolution. Only its reviewed generic definition may carry that identity.
fn valid_crew_definition(
    cost: &Cost,
    limits: &ActivationLimits,
    announcement: &Announcement,
    body: &Program,
) -> bool {
    if limits.once
        || limits.sorcery
        || limits.condition.is_some()
        || limits.activators.is_some()
        || limits.keyword.is_some()
        || !announcement.targets.is_empty()
        || announcement.modes.is_some()
    {
        return false;
    }
    let [CostPart::Constrained { part, constraints }] = cost.parts.as_slice() else {
        return false;
    };
    let [
        SelectionConstraint::TotalProperty {
            property: Property::Power,
            comparison: Comparison::GreaterOrEqual,
            amount: Expr::Constant(threshold),
        },
    ] = constraints.as_slice()
    else {
        return false;
    };
    if *threshold < 0 {
        return false;
    }
    let CostPart::ChangeTapState {
        selections,
        tapped: true,
        symbol: false,
    } = part.as_ref()
    else {
        return false;
    };
    let [
        CostObjectSelection {
            objects: crate::battlefield_query!(ObjectFilter::AllOf(filters)),
            choose: Some(Count::AnyNumber),
        },
    ] = selections.as_slice()
    else {
        return false;
    };
    if filters.len()!=4 || !filters.iter().any(|f|matches!(f,ObjectFilter::CardType(crate::mtg_types::CardType::Creature)))
        || !filters.iter().any(|f|matches!(f,ObjectFilter::Not(inner) if matches!(inner.as_ref(),ObjectFilter::Tapped)))
        || !filters.iter().any(|f|matches!(f,ObjectFilter::ControlledBy(PlayerSet::One(PlayerRef::AbilityController))))
        || !filters.iter().any(|f|matches!(f,ObjectFilter::Not(inner) if matches!(inner.as_ref(),ObjectFilter::SameObject(ObjectRef::Source)))) {return false;}
    matches!(body.instructions.as_slice(),[Instruction::Effect {operation:Operation::CreateContinuousEffect {
        source:ObjectRef::Source,affected:EffectSubjects::Objects(ObjectSet::One(ObjectRef::Source)),changes,
        duration:Duration::EndOfTurn,evaluation:EffectEvaluation::Snapshot},..}]
        if matches!(changes.as_slice(),[Change::AddCardType(crate::mtg_types::CardType::Artifact),Change::AddCardType(crate::mtg_types::CardType::Creature)]))
}

fn valid_cycling_body(origin: &ActivationOrigin, body: &Program) -> bool {
    let ActivationOrigin::Cycling { search } = origin else {
        return false;
    };
    let Some(filter) = search else {
        return matches!(
            body.instructions.as_slice(),
            [Instruction::Effect {
                operation: Operation::Draw {
                    players: PlayerSet::One(PlayerRef::AbilityController),
                    amount: Expr::Constant(1)
                },
                ..
            }]
        );
    };
    let instructions = match body.instructions.as_slice() {
        [Instruction::Group(body)] => body.instructions.as_slice(),
        instructions => instructions,
    };
    let [
        Instruction::Effect {
            result,
            operation:
                Operation::Search {
                    searcher: PlayerRef::AbilityController,
                    library_owner: PlayerRef::AbilityController,
                    top: None,
                    choices,
                    ..
                },
        },
        Instruction::Effect {
            operation:
                Operation::Reveal {
                    objects: ObjectSet::Selected(reveal),
                },
            ..
        },
        Instruction::Effect {
            operation:
                Operation::Move {
                    objects: ObjectSet::Selected(moved),
                    destination: destination @ MovementDestination::OwnersHand(_),
                    qualifiers,
                },
            ..
        },
        Instruction::Effect {
            operation:
                Operation::Shuffle {
                    players: PlayerSet::One(PlayerRef::AbilityController),
                    excluding: None,
                },
            ..
        },
    ] = instructions
    else {
        return false;
    };
    choices.len() == 1
        && matches!(choices[0].count, Count::UpTo(Expr::Constant(1)))
        && choices[0].constraints.is_empty()
        && serde_json::to_value(&choices[0].filter).ok() == serde_json::to_value(filter).ok()
        && reveal.0 == result.0
        && moved.0 == result.0
        && destination.is_plain()
        && qualifiers.is_empty()
}
fn has_definition(ability: &Ability) -> bool {
    match ability {
        Ability::Static {
            kind: StaticKind::CharacteristicDefining { .. },
            ..
        } => true,
        Ability::Scoped { ability, .. } | Ability::WithPresentation { ability, .. } => {
            has_definition(ability)
        }
        Ability::Conditional {
            then, otherwise, ..
        } => then.iter().chain(otherwise).any(has_definition),
        _ => false,
    }
}

#[inline]
fn validate_ability_in(
    ability: &Ability,
    zones: Option<&[EventZone]>,
    casting_costs: &BTreeMap<u32, Cost>,
) -> Result {
    match ability {
        Ability::WithPresentation {
            label: _,
            reminder: _,
            ability: _,
        }
        | Ability::Named(_)
        | Ability::Primitive {
            name: _,
            operands: _,
        }
        | Ability::CounterStages { .. }
        | Ability::Backup { .. }
        | Ability::Mystery {
            condition: _,
            abilities: _,
        }
        | Ability::DraftRule(_)
        | Ability::DamageCantBePrevented { .. }
        | Ability::CantEnter { cards: _, from: _ }
        | Ability::AttackCap {
            defender: _,
            amount: _,
        }
        | Ability::AsEvent {
            object: _,
            event: _,
            actions: _,
        }
        | Ability::BeforeGame { .. }
        | Ability::Modified {
            ability: _,
            modifier: _,
        }
        | Ability::Tiered { modes: _ }
        | Ability::PayToIgnore { .. }
        | Ability::CountsAsNamed { .. }
        | Ability::Unique(_) => validate_ability_in_case_0(ability, zones, casting_costs),
        _ => validate_ability_in_dispatch_1(ability, zones, casting_costs),
    }
}

#[inline]
fn validate_ability_in_dispatch_1(
    ability: &Ability,
    zones: Option<&[EventZone]>,
    casting_costs: &BTreeMap<u32, Cost>,
) -> Result {
    match ability {
        Ability::WhileOnTopOfLibrary(_)
        | Ability::Triggers(_)
        | Ability::KickedHas {
            cost: _,
            ability: _,
        }
        | Ability::StartingHandSize(_)
        | Ability::VillainousChoiceTwice(_)
        | Ability::ExilesSearchedCards {
            searching: _,
            player: _,
        }
        | Ability::Companion(_)
        | Ability::CommanderDeckRule(_)
        | Ability::Offering(_)
        | Ability::FlashIf(_)
        | Ability::Craft {
            materials: _,
            count: _,
            cost: _,
        }
        | Ability::ProtectionExcept { from: _, keeps: _ }
        | Ability::DeckRule(_)
        | Ability::Mechanic { .. }
        | Ability::CostBranch { .. }
        | Ability::CostOptions { .. }
        | Ability::Gift {
            gift: _,
            promised: _,
            unpromised: _,
        }
        | Ability::Spree { modes: _ }
        | Ability::Prototype { .. }
        | Ability::Visit { body: _, prize: _ }
        | Ability::DungeonRoom { .. } => validate_ability_in_case_1(ability, zones, casting_costs),
        _ => validate_ability_in_dispatch_2(ability, zones, casting_costs),
    }
}

#[inline]
fn validate_ability_in_dispatch_2(
    ability: &Ability,
    zones: Option<&[EventZone]>,
    casting_costs: &BTreeMap<u32, Cost>,
) -> Result {
    match ability {
        Ability::TriggerRule { .. }
        | Ability::Class { levels: _ }
        | Ability::Saga { chapters: _ }
        | Ability::Scoped {
            zones: _,
            condition: _,
            ability: _,
        }
        | Ability::Replacement {
            active: _,
            event: _,
            body: _,
        }
        | Ability::Enchant { hosts: _ } => {
            validate_ability_in_case_2(ability, zones, casting_costs)
        }
        _ => validate_ability_in_dispatch_3(ability, zones, casting_costs),
    }
}

#[inline]
fn validate_ability_in_dispatch_3(
    ability: &Ability,
    zones: Option<&[EventZone]>,
    casting_costs: &BTreeMap<u32, Cost>,
) -> Result {
    match ability {
        Ability::Static {
            kind: _,
            affected: _,
            changes: _,
        }
        | Ability::Conditional {
            condition: _,
            then: _,
            otherwise: _,
        }
        | Ability::Keyword(_)
        | Ability::Casting(_)
        | Ability::Spell {
            announcement: _,
            body: _,
        }
        | Ability::OpeningHand {
            announcement: _,
            body: _,
        } => validate_ability_in_case_3(ability, zones, casting_costs),
        _ => validate_ability_in_dispatch_4(ability, zones, casting_costs),
    }
}

#[inline]
fn validate_ability_in_dispatch_4(
    ability: &Ability,
    zones: Option<&[EventZone]>,
    casting_costs: &BTreeMap<u32, Cost>,
) -> Result {
    match ability {
        Ability::Activated {
            origin: _,
            cost: _,
            limits: _,
            announcement: _,
            body: _,
        }
        | Ability::Triggered {
            event: _,
            intervening_if: _,
            limits: _,
            announcement: _,
            body: _,
        } => validate_ability_in_case_4(ability, zones, casting_costs),
        _ => unreachable!("all current variants were dispatched"),
    }
}

fn validate_ability_in_case_0(
    ability: &Ability,
    zones: Option<&[EventZone]>,
    casting_costs: &BTreeMap<u32, Cost>,
) -> Result {
    let scope = Scope::default();
    match ability {
        Ability::WithPresentation {
            label,
            reminder,
            ability,
        } => {
            crate::presentation::validate_label(label.as_deref())?;
            if matches!(ability.as_ref(), Ability::WithPresentation { .. }) {
                return Err("ability presentation must not be nested".into());
            }
            if *reminder
                && matches!(
                    ability.as_ref(),
                    Ability::Keyword(_)
                        | Ability::Static {
                            kind: StaticKind::CharacteristicDefining {
                                keyword: Some(_),
                                ..
                            },
                            ..
                        }
                )
            {
                return Err("keyword reminder belongs to its keyword instance".into());
            }
            validate_ability_in(ability, zones, casting_costs)
        }
        Ability::Named(named) => scope.named(named),
        Ability::Primitive { name, operands } => scope.primitive(name, operands),
        Ability::CounterStages {
            level_up, stages, ..
        } => {
            level_up.iter().try_for_each(|cost| scope.cost(cost))?;
            for ability in stages.iter().flat_map(|stage| &stage.abilities) {
                validate_ability_in(ability, zones, casting_costs)?;
            }
            Ok(())
        }
        Ability::Backup { abilities, .. } => {
            for ability in abilities {
                validate_ability_in(ability, zones, casting_costs)?;
            }
            Ok(())
        }
        Ability::Mystery {
            condition,
            abilities,
        } => {
            scope.condition(condition)?;
            for ability in abilities {
                validate_ability_in(ability, zones, casting_costs)?;
            }
            Ok(())
        }
        Ability::DraftRule(effects) => effects.iter().try_for_each(|effect| scope.operand(effect)),
        Ability::DamageCantBePrevented { sources, .. } => {
            sources.iter().try_for_each(|set| scope.objects(set))
        }
        Ability::CantEnter { cards, from } => {
            from.iter().try_for_each(|zone| scope.zone_scope(zone))?;
            scope.filter(cards)
        }
        Ability::AttackCap { defender, amount } => {
            scope.recipients(defender)?;
            scope.expr(amount)
        }
        Ability::AsEvent {
            object,
            event,
            actions,
        } => {
            scope.object(object)?;
            if let AsEvent::BecomesAttachedTo(hosts) = event {
                scope.filter(hosts)?;
            }
            scope.operand(actions)
        }
        Ability::BeforeGame { body, .. } => scope.operand(body),
        Ability::Modified { ability, modifier } => {
            validate_ability_in(ability, zones, casting_costs)?;
            scope.operand(modifier)
        }
        Ability::Tiered { modes } => {
            for mode in modes {
                scope.cost(&mode.cost)?;
                validate_ability_in(&mode.ability, zones, casting_costs)?;
            }
            Ok(())
        }
        Ability::PayToIgnore {
            subjects,
            rules,
            payer,
            cost,
            ..
        } => {
            match subjects {
                EffectSubjects::Objects(objects) => scope.objects(objects)?,
                EffectSubjects::Players(players) => scope.players(players)?,
            }
            payer.iter().try_for_each(|p| scope.player(p))?;
            scope.cost(cost)?;
            scope.operand(rules)
        }
        Ability::CountsAsNamed { .. } | Ability::Unique(_) => Ok(()),
        _ => unreachable!("validated dispatch"),
    }
}

fn validate_ability_in_case_1(
    ability: &Ability,
    zones: Option<&[EventZone]>,
    casting_costs: &BTreeMap<u32, Cost>,
) -> Result {
    let scope = Scope::default();
    match ability {
        Ability::WhileOnTopOfLibrary(abilities) | Ability::Triggers(abilities) => {
            for ability in abilities {
                validate_ability_in(ability, zones, casting_costs)?;
            }
            Ok(())
        }
        Ability::KickedHas { cost, ability } => {
            scope.cost(cost)?;
            validate_ability_in(ability, zones, casting_costs)
        }
        Ability::StartingHandSize(size) => scope.expr(size),
        Ability::VillainousChoiceTwice(players) => scope.players(players),
        Ability::ExilesSearchedCards { searching, player } => {
            scope.players(searching)?;
            scope.player(player)
        }
        Ability::Companion(rule) | Ability::CommanderDeckRule(rule) => scope.operand(rule),
        Ability::Offering(filter) => scope.filter(filter),
        Ability::FlashIf(condition) => scope.condition(condition),
        Ability::Craft {
            materials,
            count,
            cost,
        } => {
            scope.operand(materials)?;
            if let Some((_, value)) = count {
                scope.expr(value)?;
            }
            scope.operand(cost)
        }
        Ability::ProtectionExcept { from, keeps } => {
            scope.operand(from)?;
            scope.objects(keeps)
        }
        Ability::DeckRule(rule) => match rule {
            DeckRule::UpToCopies(count) => scope.expr(count),
            DeckRule::AnyCards(cards) | DeckRule::AnyCardsWithOneAdditionalColor(cards) => {
                scope.filter(cards)
            }
            _ => Ok(()),
        },
        Ability::Mechanic {
            cost,
            amount,
            filter,
            ..
        } => {
            cost.iter().try_for_each(|cost| scope.cost(cost))?;
            amount.iter().try_for_each(|amount| scope.expr(amount))?;
            filter.iter().try_for_each(|filter| scope.filter(filter))
        }
        Ability::CostBranch {
            cost, paid, unpaid, ..
        } => {
            scope.cost(cost)?;
            let mut paid_costs = casting_costs.clone();
            paid_costs.insert(BRANCH_COST, cost.clone());
            validate_ability_in(paid, zones, &paid_costs)?;
            validate_ability_in(unpaid, zones, casting_costs)
        }
        Ability::CostOptions { options, .. } => {
            for (cost, ability) in options {
                scope.cost(cost)?;
                let mut paid_costs = casting_costs.clone();
                paid_costs.insert(BRANCH_COST, cost.clone());
                validate_ability_in(ability, zones, &paid_costs)?;
            }
            Ok(())
        }
        Ability::Gift {
            gift,
            promised,
            unpromised,
        } => {
            for program in gift {
                scope.clone().program(program)?;
            }
            for outcome in promised.iter().chain(unpromised) {
                validate_ability_in(outcome, zones, casting_costs)?;
            }
            Ok(())
        }
        Ability::Spree { modes } => {
            if modes.is_empty() {
                return Err("spree has no modes".into());
            }
            for mode in modes {
                scope.cost(&mode.cost)?;
                validate_ability_in(&mode.ability, zones, casting_costs)?;
            }
            Ok(())
        }
        Ability::Prototype { .. } => Ok(()),
        Ability::Visit { body, prize } => {
            validate_ability_in(body, zones, casting_costs)?;
            if let Some(prize) = prize {
                validate_ability_in(prize, zones, casting_costs)?;
            }
            Ok(())
        }
        Ability::DungeonRoom { body, .. } => validate_ability_in(body, zones, casting_costs),
        _ => unreachable!("validated dispatch"),
    }
}

fn validate_ability_in_case_2(
    ability: &Ability,
    zones: Option<&[EventZone]>,
    casting_costs: &BTreeMap<u32, Cost>,
) -> Result {
    let mut scope = Scope::default();
    match ability {
        Ability::TriggerRule {
            cause, abilities, ..
        } => {
            if let Some(event) = cause {
                scope.event(event)?;
            }
            if let Some(filter) = abilities {
                scope.filter(filter)?;
            }
            Ok(())
        }
        Ability::Class { levels } => {
            if levels.is_empty() {
                return Err("class has no levels".into());
            }
            for level in levels {
                scope.cost(&level.cost)?;
                for ability in &level.abilities {
                    validate_ability_in(ability, zones, casting_costs)?;
                }
            }
            Ok(())
        }
        Ability::Saga { chapters } => {
            if chapters.is_empty() {
                return Err("saga has no chapters".into());
            }
            for chapter in chapters {
                if !matches!(
                    chapter,
                    Ability::Triggered {
                        event: EventSpec::SagaChapter(1..),
                        ..
                    }
                ) {
                    return Err("saga chapter must trigger on a positive chapter number".into());
                }
                validate_ability_in(chapter, zones, casting_costs)?;
            }
            Ok(())
        }
        Ability::Scoped {
            zones: active,
            condition,
            ability,
        } => {
            if active.is_empty()
                || active
                    .iter()
                    .enumerate()
                    .any(|(i, z)| active[..i].contains(z))
            {
                return Err("ability scope has empty or repeated zones".into());
            }
            let effective = active
                .iter()
                .filter(|z| zones.is_none_or(|outer| outer.contains(z)))
                .cloned()
                .collect::<Vec<_>>();
            if effective.is_empty() {
                return Err("nested source scopes have no common zone".into());
            }
            if let Some(condition) = condition {
                scope.condition(condition)?;
            }
            validate_ability_in(ability, Some(&effective), casting_costs)
        }
        Ability::Replacement {
            active,
            event,
            body,
        } => {
            scope.replacement_event(event)?;
            fn only_source(filter: &ObjectFilter) -> bool {
                match filter {
                    ObjectFilter::SameObject(ObjectRef::Source) => true,
                    ObjectFilter::AllOf(filters) => filters.iter().any(only_source),
                    _ => false,
                }
            }
            if matches!(active, ReplacementActive::SourceEntering)
                && !matches!(event, ReplacementEvent::Enter { objects } if only_source(objects))
            {
                return Err("source-entry replacement must match only its source".into());
            }
            scope.replacement = true;
            scope.event = crate::semantics::replacement_bindings(event);
            scope.program(body)
        }
        Ability::Enchant { hosts } => match hosts {
            EnchantHosts::Permanents(filter) => scope.filter(filter),
            EnchantHosts::Players(players) => scope.players(players),
            EnchantHosts::GraveyardCards { filter, owners } => {
                scope.filter(filter)?;
                scope.players(owners)
            }
        },
        _ => unreachable!("validated dispatch"),
    }
}

fn validate_ability_in_case_3(
    ability: &Ability,
    zones: Option<&[EventZone]>,
    casting_costs: &BTreeMap<u32, Cost>,
) -> Result {
    let mut scope = Scope::default();
    let (announcement, body) = match ability {
        Ability::Static {
            kind,
            affected,
            changes,
        } => {
            if let StaticKind::CharacteristicDefining { keyword, .. } = kind {
                scope.definition = true;
                if zones.is_some()
                    || !matches!(
                        affected,
                        EffectSubjects::Objects(ObjectSet::One(ObjectRef::Source))
                    )
                {
                    return Err("characteristic definition must intrinsically affect only its source everywhere".into());
                }
                if changes.iter().any(|c| {
                    !matches!(
                        c,
                        Change::SetPower(_)
                            | Change::SetToughness(_)
                            | Change::SetBasePowerToughness { .. }
                            | Change::Colors { .. }
                            | Change::ChosenColor { .. }
                            | Change::ChosenType { .. }
                            | Change::CreatureTypes { .. }
                            | Change::LandTypes { .. }
                            | Change::Primitive { .. }
                    )
                }) {
                    return Err("characteristic definition may define only color, subtypes, power, or toughness".into());
                }
                if let Some(keyword) = keyword {
                    scope.keyword(keyword)?;
                    let valid = match crate::presentation::keyword(keyword) {
                        Keyword::Devoid => {
                            matches!(changes.as_slice(),[Change::Colors {mode:ChangeMode::Set,colors}] if colors.is_empty())
                        }
                        Keyword::Changeling => matches!(
                            changes.as_slice(),
                            [Change::CreatureTypes {
                                mode: ChangeMode::Set,
                                types: CreatureTypeSet::All
                            }]
                        ),
                        _ => false,
                    };
                    if !valid {
                        return Err(
                            "defining keyword does not match its characteristic definition".into(),
                        );
                    }
                }
            }
            return scope.subject_changes(affected, changes);
        }
        Ability::Conditional {
            condition,
            then,
            otherwise,
        } => {
            scope.condition(condition)?;
            for ability in then.iter().chain(otherwise) {
                if has_definition(ability) {
                    return Err("characteristic definition cannot be a conditional ability".into());
                }
                validate_ability_in(ability, zones, casting_costs)?;
            }
            return Ok(());
        }
        Ability::Keyword(keyword) => return scope.keyword(keyword),
        Ability::Casting(rule) => return scope.casting_rule(rule),
        Ability::Spell { announcement, body } => {
            scope.casting_costs = casting_costs.clone();
            (announcement, body)
        }
        Ability::OpeningHand { announcement, body } => (announcement, body),
        _ => unreachable!("validated dispatch"),
    };
    scope.announcement(announcement)?;
    if announcement.modes.is_some() && !body.instructions.is_empty() {
        return Err("modal ability has a separate resolution body".into());
    }
    scope.program(body)
}

fn validate_ability_in_case_4(
    ability: &Ability,
    zones: Option<&[EventZone]>,
    _casting_costs: &BTreeMap<u32, Cost>,
) -> Result {
    let mut scope = Scope::default();
    let (announcement, body) = match ability {
        Ability::Activated {
            origin,
            cost,
            limits,
            announcement,
            body,
        } => {
            if matches!(origin, ActivationOrigin::Crew)
                && !valid_crew_definition(cost, limits, announcement, body)
            {
                return Err("invalid crew payment, participants, or resolution program".into());
            }
            if matches!(origin, ActivationOrigin::Cycling { .. })
                && (zones != Some(&[EventZone::Hand][..])
                    || announcement.modes.is_some()
                    || !announcement.targets.is_empty()
                    || limits.sorcery
                    || limits.once
                    || limits.once_each_turn
                    || limits.condition.is_some()
                    || !cost.parts.iter().any(|part| {
                        matches!(
                            part,
                            CostPart::Discard {
                                count: Expr::Constant(1),
                                filter: ObjectFilter::SameObject(ObjectRef::Source),
                                random: false
                            }
                        )
                    })
                    || !valid_cycling_body(origin, body))
            {
                return Err("invalid cycling scope, source discard, or draw program".into());
            }
            scope.cost(cost)?;
            scope.activation_cost = Some(cost.clone());
            if let Some(condition) = &limits.condition {
                let mut proposal = scope.clone();
                proposal.announcing = true;
                proposal.condition(condition)?;
            }
            // A cost change can read what the ability targets.
            let mut announced = scope.clone();
            announced.announcement(announcement)?;
            for modifier in &limits.modifiers {
                match modifier {
                    Change::Primitive { name, operands } => announced.primitive(name, operands)?,
                    Change::ActivationRule(rule) => {
                        let mut proposal = announced.clone();
                        proposal.announcing = true;
                        proposal.activation_rule(rule)?;
                    }
                    _ => return Err("an activation modifier must be a reviewed form".into()),
                }
            }
            if matches!(origin, ActivationOrigin::Equip) {
                let [target] = announcement.targets.as_slice() else {
                    return Err("equip must announce exactly one target slot".into());
                };
                if !limits.sorcery
                    || announcement.modes.is_some()
                    || !matches!(&target.kind, TargetKind::Object(domain) if *domain == ObjectDomain::permanent())
                    || !matches!(target.count, Count::Exactly(Expr::Constant(1)))
                {
                    return Err("invalid equip activation structure".into());
                }
                let [
                    Instruction::Effect {
                        operation:
                            Operation::Attach {
                                attachments: ObjectSet::One(ObjectRef::Source),
                                host: ObjectRef::Target { slot, index: 0 },
                            },
                        ..
                    },
                ] = body.instructions.as_slice()
                else {
                    return Err("equip must attach its source to its announced target".into());
                };
                if slot.0 != target.slot.0 {
                    return Err("equip attachment uses a different target slot".into());
                }
            }
            (announcement, body)
        }
        Ability::Triggered {
            event,
            intervening_if,
            limits,
            announcement,
            body,
        } => {
            scope.event(event)?;
            scope.event = crate::semantics::event_bindings(event);
            if matches!(limits.per_turn, Some(0)) || matches!(limits.total, Some(0)) {
                return Err("trigger limit must be positive".into());
            }
            if let Some(condition) = intervening_if {
                scope.condition(condition)?;
            }
            (announcement, body)
        }
        _ => unreachable!("validated dispatch"),
    };
    scope.announcement(announcement)?;
    if announcement.modes.is_some() && !body.instructions.is_empty() {
        return Err("modal ability has a separate resolution body".into());
    }
    scope.program(body)
}

/// Accepts `Named` forms. Complete cards go through `card`, which rejects them.
pub fn first_invalid(card: &Card) -> Option<(usize, String)> {
    first_invalid_abilities(&card.abilities)
}

fn first_invalid_abilities(abilities: &[Ability]) -> Option<(usize, String)> {
    let casting_costs = abilities
        .iter()
        .enumerate()
        .filter_map(|(i, ability)| {
            crate::cost_history::casting_cost(ability)
                .cloned()
                .map(|cost| (i as u32, cost))
        })
        .collect();
    abilities.iter().enumerate().find_map(|(i, ability)| {
        validate_ability_in(ability, None, &casting_costs)
            .err()
            .map(|e| (i, e))
    })
}

/// Validate complete abilities without inventing printed card characteristics.
/// Casting-cost addresses use the list's actual ability indices. Named/opaque
/// unreduced forms never count as a successful semantic reading.
pub fn abilities(abilities: &[Ability]) -> Result {
    if let Some((index, why)) = first_invalid_abilities(abilities) {
        return Err(format!("abilities[{index}]: {why}"));
    }
    match crate::complete::named_forms(&abilities).keys().next() {
        Some(form) => Err(format!("unreduced form {form}")),
        None => Ok(()),
    }
}

fn face(card: &Card) -> Result {
    if card.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported AST schema version {}",
            card.schema_version
        ));
    }
    if card.name.is_empty() {
        return Err("card name is empty".into());
    }
    match first_invalid(card) {
        Some((i, e)) => Err(format!("{}: abilities[{i}]: {e}", card.name)),
        None => Ok(()),
    }
}

pub fn card(card: &Card) -> Result {
    face(card)?;
    match crate::complete::named_forms(card).keys().next() {
        Some(form) => Err(format!("{}: unreduced form {form}", card.name)),
        None => Ok(()),
    }
}

/// Validate a record that may hold `Named` forms.
pub fn unified(card: &UnifiedCard) -> Result {
    if card.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported AST schema version {}",
            card.schema_version
        ));
    }
    if card.faces.is_empty() && card.data.is_empty() {
        return Err(format!("{} record is empty", card.layout));
    }
    layout::record(card)?;
    card.faces.iter().try_for_each(face)?;
    card.data.iter().try_for_each(|f| operand(&f.value))
}

/// A `Named` operand must stand alone, so it is checked with no surrounding scope.
pub fn operand(value: &Operand) -> Result {
    Scope::default().operand(value)
}

mod scope_adopt_choices;
mod scope_casting_rule;
mod scope_event_case_1;
mod scope_filter_inner;
mod scope_objects;
mod scope_operation_case_3;
mod scope_operation_case_9;
mod scope_operation_dispatch_6;
mod scope_paid_component;
mod scope_subject_changes;
