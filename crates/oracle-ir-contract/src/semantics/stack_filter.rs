//! Extracted portable semantic contracts.
use super::*;
/// What an event says about objects, when a plain filter can say it.
/// A filter on the stack, from whichever of the three families wrote it.
pub(super) fn stack_filter(operand: &Operand) -> Option<ObjectFilter> {
    use StackTrait as T;
    let (family, form, operands): (&str, &str, &[Operand]) = match operand {
        Operand::Term {
            family,
            form,
            operands,
        } => (family, form, operands),
        Operand::Vocabulary { family, form } => (family, form, &[]),
        _ => return None,
    };
    if !matches!(
        family,
        "Abilities" | "ActivatedAbilities" | "SpellsAndAbilities"
    ) {
        return None;
    }
    let is = |value| ObjectFilter::OnStack(value);
    let not = |value| ObjectFilter::Not(Box::new(ObjectFilter::OnStack(value)));
    let keyword = |value| ObjectFilter::OnStack(T::Keyword(value));
    let list = || -> Option<Vec<ObjectFilter>> {
        let mut out = vec![];
        for operand in operands {
            match operand {
                Operand::List(items) => {
                    for item in items {
                        out.push(stack_filter(item)?);
                    }
                }
                _ => out.push(stack_filter(operand)?),
            }
        }
        Some(out)
    };
    let objects = |operand: &Operand| match operand {
        Operand::Objects(objects) => Some(objects_filter(objects)),
        Operand::Filter(filter) => Some(filter.clone()),
        Operand::Object(object) => Some(ObjectFilter::SameObject(object.clone())),
        _ => None,
    };
    let target = |recipients| ObjectFilter::TargetsAny(Box::new(recipients));
    Some(match (form, operands) {
        ("And", _) => ObjectFilter::AllOf(list()?),
        ("Or", _) => ObjectFilter::AnyOf(list()?),
        ("AnyAbility" | "AnySpellOrAbility", []) => ObjectFilter::Any,
        ("FirstAbilityActivatedByPlayerThisTurn", [inner, Operand::Player(player)]) => {
            ObjectFilter::NthCast {
                ordinal: Expr::Constant(1),
                spells: Box::new(stack_filter(inner)?),
                by: Some(PlayerSet::One(player.clone())),
                scope: CastScope::Turn,
            }
        }
        ("HasASingleTarget" | "TargetsOnlyASinglePermanentOrPlayer", []) => is(T::SingleTarget),
        ("IsCardtype", [_]) => ObjectFilter::AbilityOf(Box::new(card_term(operand)?)),
        ("TriggeredAbility", []) => is(T::Triggered),
        ("ActivatedAbility", []) => is(T::Activated),
        ("RoomAbilityOfDungeon", [Operand::Term { operands, .. }]) => match operands.as_slice() {
            [Operand::Players(players)] => ObjectFilter::AllOf(vec![
                is(T::DungeonRoom),
                ObjectFilter::ControlledBy(players.clone()),
            ]),
            _ => return None,
        },
        ("AbilityOfAnEmblem", [Operand::Term { operands, .. }]) => match operands.as_slice() {
            [Operand::Players(players)] => ObjectFilter::AllOf(vec![
                is(T::OfEmblem),
                ObjectFilter::ControlledBy(players.clone()),
            ]),
            _ => return None,
        },
        ("AbilityOfASpell", [Operand::Filter(spells)]) => {
            ObjectFilter::AllOf(vec![is(T::Spell), spells.clone()])
        }
        ("ManaAbility", []) => is(T::Mana),
        ("NonManaAbility", []) => not(T::Mana),
        ("LoyaltyAbility", []) => is(T::Loyalty),
        ("NonLoyaltyAbility", []) => not(T::Loyalty),
        ("HasXInCost", []) => is(T::XInCost),
        ("HasTapSelfInCost", []) => is(T::TapSymbolInCost),
        ("DoesntHaveTapSelfInCost", []) => not(T::TapSymbolInCost),
        ("TargetsOnlySinglePermanent", [Operand::Object(object)]) => ObjectFilter::AllOf(vec![
            is(T::SingleTarget),
            target(Recipients::Objects(ObjectSet::One(object.clone()))),
        ]),
        ("BackupAbility", []) => keyword(AbilityKeyword::Backup),
        ("BoastAbility", []) => keyword(AbilityKeyword::Boast),
        ("EmbalmAbility", []) => keyword(AbilityKeyword::Embalm),
        ("EquipAbility", []) => keyword(AbilityKeyword::Equip),
        ("EternalizeAbility", []) => keyword(AbilityKeyword::Eternalize),
        ("ExhaustAbility", []) => keyword(AbilityKeyword::Exhaust),
        ("ModularAbility", []) => keyword(AbilityKeyword::Modular),
        ("NinjutsuAbility", []) => keyword(AbilityKeyword::Ninjutsu),
        ("OutlastAbility", []) => keyword(AbilityKeyword::Outlast),
        ("PowerUpAbility", []) => keyword(AbilityKeyword::PowerUp),
        ("AbilityOfACardInPlayersGraveyard", [Operand::Filter(cards), Operand::Player(player)]) => {
            ObjectFilter::AbilityOf(Box::new(ObjectFilter::AllOf(vec![
                cards.clone(),
                ObjectFilter::InZone(ZoneScope::Players {
                    zone: EventZone::Graveyard,
                    players: PlayerSet::One(player.clone()),
                }),
            ])))
        }
        ("AbilityOfPermanent" | "AbilityOfAPermanent" | "AbilityOfASource", [source]) => {
            ObjectFilter::AbilityOf(Box::new(objects(source)?))
        }
        ("NotAnAbilityOfAPermanent", [source]) => ObjectFilter::Not(Box::new(
            ObjectFilter::AbilityOf(Box::new(objects(source)?)),
        )),
        ("ControlledByAPlayer", [Operand::Players(players)]) => {
            ObjectFilter::ControlledBy(players.clone())
        }
        ("IsSpell", [Operand::Filter(spells)]) => {
            ObjectFilter::AllOf(vec![is(T::Spell), spells.clone()])
        }
        ("CanTargetOnly", [Operand::Objects(set)]) => {
            ObjectFilter::CanTargetOnly(Box::new(event_filter(set)?))
        }
        ("TargetsAPermanent", [Operand::Objects(set)]) => target(Recipients::Objects(set.clone())),
        ("TargetsPermanent", [Operand::Object(object)]) => {
            target(Recipients::Objects(ObjectSet::One(object.clone())))
        }
        ("TargetsAPlayer", [Operand::Players(players)]) => {
            target(Recipients::Players(players.clone()))
        }
        ("TargetsPlayer", [Operand::Player(player)]) => {
            target(Recipients::Players(PlayerSet::One(player.clone())))
        }
        _ => return None,
    })
}

pub(super) fn event_filter(objects: &ObjectSet) -> Option<ObjectFilter> {
    if let Some(filter) = objects.battlefield_filter() {
        return Some(filter.clone());
    }
    match objects {
        ObjectSet::One(object) => Some(ObjectFilter::SameObject(object.clone())),
        _ => None,
    }
}

/// A counter's kind, read from its operand.
pub fn counter_kind(operand: &Operand) -> Option<crate::counter::CounterKind> {
    use crate::counter::CounterKind;
    match operand {
        Operand::Term {
            family,
            form,
            operands,
        } if family == "CounterType" && form == "PTCounter" => match operands.as_slice() {
            [Operand::Integer(power), Operand::Integer(toughness)] => {
                Some(CounterKind::PowerToughness(
                    (*power).try_into().ok()?,
                    (*toughness).try_into().ok()?,
                ))
            }
            _ => None,
        },
        Operand::Vocabulary { family, form } if family == "CounterType" => {
            let value = serde_json::from_value(serde_json::Value::String(form.clone())).ok()?;
            Some(crate::counter::lower(&value))
        }
        _ => None,
    }
}

pub fn with_row(outer: &EventBindings, name: &str) -> EventBindings {
    with_bindings(outer, &row_bindings(name))
}

/// The enclosing scope plus what `event` binds, for its own filters.
pub fn with_event(outer: &EventBindings, event: &EventSpec) -> EventBindings {
    with_bindings(outer, &event_bindings(event))
}

pub(super) fn with_bindings(outer: &EventBindings, row: &EventBindings) -> EventBindings {
    EventBindings {
        object: outer.object || row.object,
        permanent: outer.permanent || row.permanent,
        spell: outer.spell || row.spell,
        player: outer.player || row.player,
        amount: outer.amount || row.amount,
        block_pair: outer.block_pair || row.block_pair,
        group: outer.group || row.group,
        stack_object: outer.stack_object || row.stack_object,
        other: outer.other || row.other,
        other_player: outer.other_player || row.other_player,
        moved: outer.moved || row.moved,
        // A new distinguished object shadows this particular causal reference.
        enchanted_left: if row.object {
            row.enchanted_left
        } else {
            outer.enchanted_left
        },
        ..*outer
    }
}

/// A limited exile's follow-up matches each returning permanent, independently
/// of the event that created the exile instruction.
pub fn entry_return_event() -> EventSpec {
    EventSpec::ZoneChange {
        objects: ObjectFilter::Any,
        from: Some(EventZone::Exile),
        to: EventZone::Battlefield,
    }
}

#[inline]
pub fn event_bindings(event: &EventSpec) -> EventBindings {
    match event {
        EventSpec::SagaChapter(_)
        | EventSpec::FirstEachTurn(_)
        | EventSpec::FirstForObjectEachTurn(_)
        | EventSpec::FirstEachGame(_)
        | EventSpec::NthEachTurn { .. }
        | EventSpec::Caused { .. }
        | EventSpec::WhileActivating { .. }
        | EventSpec::ZoneChangeFromElsewhere { .. }
        | EventSpec::NthCounter { .. }
        | EventSpec::AttackUnblocked { count: None, .. }
        | EventSpec::AttackUnblocked { .. }
        | EventSpec::AllAttack(_)
        | EventSpec::OntoStack(_)
        | EventSpec::Resolves(_)
        | EventSpec::ExploresCard { .. }
        | EventSpec::TappedForManaOf { by: None, .. }
        | EventSpec::TappedForManaOf { .. }
        | EventSpec::ManaAdded { .. }
        | EventSpec::TargetsChosen { .. }
        | EventSpec::ControlChanges { .. }
        | EventSpec::CountersOnPlayer { .. }
        | EventSpec::AttacksWith { by: Some(_), .. }
        | EventSpec::AttacksWith {
            different_defenders: true,
            ..
        }
        | EventSpec::AttacksWith { .. }
        | EventSpec::AttacksAsGroup { .. }
        | EventSpec::SpellTargeting {
            targets: Some(_), ..
        }
        | EventSpec::SpellTargeting { .. }
        | EventSpec::SpellOrActivation { .. }
        | EventSpec::PaysToActivate {
            payment: ActivationPayment::Life,
            ..
        }
        | EventSpec::PaysToActivate { .. }
        | EventSpec::ObjectBecomes(_)
        | EventSpec::CausesTrigger {
            cause: _,
            own_ability: _,
        }
        | EventSpec::Primitive { .. }
        | EventSpec::Player {
            event: PlayerEvent::RingTempts,
            ..
        } => event_bindings_case_0(event),
        _ => event_bindings_dispatch_1(event),
    }
}

#[inline]
pub(super) fn event_bindings_dispatch_1(event: &EventSpec) -> EventBindings {
    match event {
        EventSpec::Player { .. }
        | EventSpec::Discard { .. }
        | EventSpec::ZoneChange {
            objects: _,
            from: _,
            to: _,
        }
        | EventSpec::Object { .. }
        | EventSpec::Pair {
            event: PairEvent::Blocks | PairEvent::BecomesBlockedBy,
            ..
        }
        | EventSpec::Pair { .. }
        | EventSpec::BecomesTarget { .. }
        | EventSpec::Attack { .. }
        | EventSpec::Counters { .. }
        | EventSpec::Batch {
            event: _,
            per: _,
            count: _,
            including: _,
        }
        | EventSpec::ClassLevel { .. } => event_bindings_case_1(event),
        _ => event_bindings_dispatch_2(event),
    }
}

#[inline]
pub(super) fn event_bindings_dispatch_2(event: &EventSpec) -> EventBindings {
    match event {
        EventSpec::Damage(_)
        | EventSpec::Spell { .. }
        | EventSpec::Activated { .. }
        | EventSpec::Turn { .. }
        | EventSpec::State { .. }
        | EventSpec::StateWithPlayer { .. }
        | EventSpec::WithOtherObject { .. }
        | EventSpec::Filtered { .. }
        | EventSpec::AnyOf(_) => event_bindings_case_2(event),
        _ => unreachable!("all current variants were dispatched"),
    }
}

pub(super) fn event_bindings_case_0(event: &EventSpec) -> EventBindings {
    match event {
        EventSpec::SagaChapter(_) => EventBindings::default(),
        EventSpec::FirstEachTurn(event)
        | EventSpec::FirstForObjectEachTurn(event)
        | EventSpec::FirstEachGame(event) => event_bindings(event),
        EventSpec::NthEachTurn { event, .. } => event_bindings(event),
        EventSpec::Caused { event, .. } => EventBindings {
            stack_object: true,
            ..event_bindings(event)
        },
        EventSpec::WhileActivating { event, .. } => EventBindings {
            player: true,
            ..event_bindings(event)
        },
        EventSpec::ZoneChangeFromElsewhere { to, .. } => EventBindings {
            object: true,
            permanent: true,
            moved: true,
            moved_to_hand: matches!(to, EventZone::Hand),
            moved_owner: matches!(to, EventZone::Graveyard | EventZone::Hand),
            ..EventBindings::default()
        },
        EventSpec::NthCounter { .. } => EventBindings {
            counters_placed: true,
            ..row_bindings("Trigger::WhenTheNthCounterOfTypeIsPutOnAPermanent")
        },
        EventSpec::AttackUnblocked { count: None, .. } => {
            row_bindings("Trigger::WhenACreatureAttacksAPlayerAndIsntBlocked")
        }
        EventSpec::AttackUnblocked { .. } => {
            row_bindings("Trigger::WhenANumberOfCreaturesAttacksAPlayerAndArentBlocked")
        }
        EventSpec::AllAttack(_) => row_bindings("Trigger::WhenAllCreaturesAttack"),
        EventSpec::OntoStack(_) => row_bindings("Trigger::WhenASpellOrAbilityIsPutOntoTheStack"),
        EventSpec::Resolves(_) => row_bindings("Trigger::WhenAManaAbilityOfAPermanentResolves"),
        EventSpec::ExploresCard { .. } => {
            row_bindings("Trigger::WhenAPermanentExploresACardOfType")
        }
        EventSpec::TappedForManaOf { by: None, .. } => {
            row_bindings("Trigger::WhenAPermanentIsTappedForManaOfColor")
        }
        EventSpec::TappedForManaOf { .. } => {
            row_bindings("Trigger::WhenAPlayerTapsAPermanentForManaOfColor")
        }
        EventSpec::ManaAdded { .. } => row_bindings("Trigger::WhenAnAbilityCausesAPlayerToAddMana"),
        EventSpec::TargetsChosen { .. } => {
            row_bindings("Trigger::WhenAPlayerChoosesTargetsForASpellOrAbility")
        }
        EventSpec::ControlChanges { .. } => {
            row_bindings("Trigger::WhenAPlayerGainsControlOfAPermanentFromAPlayer")
        }
        EventSpec::CountersOnPlayer { .. } => EventBindings {
            counters_placed: true,
            ..row_bindings("Trigger::WhenAPlayerPutsCountersOnAPlayer")
        },
        EventSpec::AttacksWith { by: Some(_), .. } => {
            row_bindings("Trigger::WhenAPlayerAttacksWithASingleCreatureAndANumberOfOtherCreatures")
        }
        EventSpec::AttacksWith {
            different_defenders: true,
            ..
        } => row_bindings(
            "Trigger::WhenASingleCreatureAndANumberOfOtherCreaturesAttackDifferentPlayers",
        ),
        EventSpec::AttacksWith { .. } => {
            row_bindings("Trigger::WhenASingleCreatureAttacksWithExactlyOneOtherCreature")
        }
        EventSpec::AttacksAsGroup { .. } => {
            row_bindings("Trigger::WhenAPlayerAttacksWithAnyNumberOfGroupCreatures")
        }
        EventSpec::SpellTargeting {
            targets: Some(_), ..
        } => row_bindings("Trigger::WhenAPlayerCastsASpellThatTargetsAnyNumberOfPermanents"),
        EventSpec::SpellTargeting { .. } => {
            row_bindings("Trigger::WhenAPlayerCastsASpellWithANumberOfTargets")
        }
        EventSpec::SpellOrActivation { .. } => {
            row_bindings("Trigger::WhenAPlayerCastsASpellOrActivatesAnAbility")
        }
        EventSpec::PaysToActivate {
            payment: ActivationPayment::Life,
            ..
        } => row_bindings("Trigger::WhenAPlayerPaysLifeToActivateAnAbility"),
        EventSpec::PaysToActivate { .. } => {
            row_bindings("Trigger::WhenAPlayerSacrificesAnyNumberOfPermanentsToActivateAnAbility")
        }
        EventSpec::ObjectBecomes(_) => EventBindings {
            group: true,
            ..row_bindings("Trigger::WhenAPermanentHasPower")
        },
        EventSpec::CausesTrigger { cause, own_ability } => match (cause.as_ref(), own_ability) {
            (EventSpec::Attack { .. }, _) => {
                row_bindings("Trigger::WhenAPermanentAttackingCausesItsAbilityToTrigger")
            }
            (_, true) => row_bindings(
                "Trigger::WhenAPermanentEnteringTheBattlefieldUnderAPlayersControlCausesItsAbilityToTrigger",
            ),
            _ => row_bindings(
                "Trigger::WhenAPermanentEnteringTheBattlefieldCausesAnAbilityToTrigger",
            ),
        },
        EventSpec::Primitive { name, .. } => row_bindings(name),
        EventSpec::Player {
            event: PlayerEvent::RingTempts,
            ..
        } => EventBindings {
            ring_choice: true,
            ..row_bindings("Trigger::WhenTheRingTemptsAPlayer")
        },
        _ => unreachable!("validated dispatch"),
    }
}

pub(super) fn event_bindings_case_1(event: &EventSpec) -> EventBindings {
    match event {
        EventSpec::Player { event, .. } => PLAYER_EVENTS
            .iter()
            .find(|(_, row)| row == event)
            .map_or_else(EventBindings::default, |(name, _)| row_bindings(name)),
        EventSpec::Discard { grouping, kind, .. } => EventBindings {
            group: matches!(grouping, DiscardGrouping::Player),
            object: matches!(grouping, DiscardGrouping::Card),
            player: true,
            amount: matches!(grouping, DiscardGrouping::Player),
            discard: true,
            discarded: matches!(grouping, DiscardGrouping::Card),
            activation_x: matches!(kind, DiscardKind::Cycling),
            stack_object: matches!(kind, DiscardKind::CausedBy(_)),
            moved: matches!(kind, DiscardKind::CausedBy(_)),
            ..Default::default()
        },
        EventSpec::ZoneChange { objects, from, to } => EventBindings {
            object: true,
            permanent: true,
            moved: true,
            moved_to_hand: matches!(to, EventZone::Hand),
            moved_owner: matches!(to, EventZone::Graveyard | EventZone::Hand),
            enchanted_left: matches!(from, Some(EventZone::Battlefield))
                && matches!(objects,
                ObjectFilter::SameObject(ObjectRef::AttachedTo(aura)) if matches!(aura.as_ref(), ObjectRef::Source)),
            ..EventBindings::default()
        },
        EventSpec::Object { event, .. } => {
            OBJECT_EVENTS.iter().find(|(_, row)| row == event).map_or(
                EventBindings {
                    object: true,
                    permanent: true,
                    ..EventBindings::default()
                },
                |(name, _)| row_bindings(name),
            )
        }
        EventSpec::Pair {
            event: PairEvent::Blocks | PairEvent::BecomesBlockedBy,
            ..
        } => EventBindings {
            object: true,
            block_pair: true,
            ..EventBindings::default()
        },
        EventSpec::Pair { event, .. } => PAIR_EVENTS
            .iter()
            .find(|(_, row)| row == event)
            .map_or_else(EventBindings::default, |(name, _)| row_bindings(name)),
        EventSpec::BecomesTarget { .. } => EventBindings {
            object: true,
            permanent: true,
            stack_object: true,
            ..EventBindings::default()
        },
        EventSpec::Attack { by, defenders, .. } => {
            let attacked = !matches!(defenders, AttackDefenders::Any);
            EventBindings {
                object: true,
                permanent: true,
                player: by.is_some() || attacked,
                other_player: by.is_some() && attacked,
                ..EventBindings::default()
            }
        }
        EventSpec::Counters { change, by, .. } => EventBindings {
            counters_placed: matches!(change, CounterChange::Put),
            object: true,
            permanent: true,
            player: by.is_some(),
            amount: true,
            ..EventBindings::default()
        },
        EventSpec::Batch {
            event,
            per,
            count,
            including,
        } => {
            let mut inner = event_bindings(event);
            // The subject of a batched pair is one object even though its
            // other half is not.
            if matches!(**event, EventSpec::Pair { .. }) {
                inner.object = true;
                inner.permanent = true;
                inner.block_pair = false;
            }
            let one = matches!(per, BatchPer::Subject)
                || count.is_none() && including.is_none() && !matches!(per, BatchPer::Defender);
            EventBindings {
                object: inner.object && one,
                permanent: inner.permanent && one,
                amount: true,
                // "One or more creatures" names the other half of a pair.
                group: !matches!(per, BatchPer::Subject)
                    || matches!(**event, EventSpec::Pair { .. }),
                ..inner
            }
        }
        EventSpec::ClassLevel { .. } => EventBindings {
            object: true,
            permanent: true,
            ..EventBindings::default()
        },
        _ => unreachable!("validated dispatch"),
    }
}
