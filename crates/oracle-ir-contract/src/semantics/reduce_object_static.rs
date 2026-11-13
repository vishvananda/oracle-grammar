//! Extracted portable semantic contracts.
use super::*;
/// Rules on a permanent, past the prohibitions `reduce_rule` already reads.
pub(super) fn reduce_object_static(bare: &str, operands: &[Operand]) -> Option<RuleChange> {
    use ObjectAction as A;
    use Operand::{Condition as Cond, Number, Objects, Players};
    use RuleChange as R;
    let prohibit = |action| Some(R::Prohibit(action));
    let attached = |attachments, keep_existing| {
        Some(R::Prohibit(A::BeAttachedBy {
            attachments,
            keep_existing,
        }))
    };
    let despite = |attackers, evasion| Some(R::BlocksDespite { attackers, evasion });
    let unique = |rule| Some(R::Unique(rule));
    let with_keyword = |form| ObjectFilter::Subtype(form);
    match (bare, operands) {
        ("CanBeTheTargetOfSpellsAndAbilitiesAsThoughTheyDidntHaveHexproof", [sources]) => {
            Some(R::TargetableDespite {
                sources: stack_filter(sources)?,
                keyword: TargetingKeyword::Hexproof,
            })
        }
        ("CanBeTheTargetOfSpellsOrAbilitiesAsThoughItDidntHaveShroud", [sources]) => {
            Some(R::TargetableDespite {
                sources: stack_filter(sources)?,
                keyword: TargetingKeyword::Shroud,
            })
        }
        ("CanBeAttachedOnlyToAPermanent", [Objects(set)]) => {
            Some(R::AttachOnlyTo(event_filter(set)?))
        }
        ("CantBlockUnlessAttackingPlayer", [Cond(condition)]) => {
            Some(R::BlocksOnlyIf(condition.clone()))
        }
        ("CantBeBlockedUnlessDefendingPlayer", [Players(players)]) => Some(R::BlockedByDefender {
            players: players.clone(),
            unless: true,
        }),
        ("CantBeBlockedIfDefendingPlayer", [Players(players)]) => Some(R::BlockedByDefender {
            players: players.clone(),
            unless: false,
        }),
        ("AttackingCausesOthersToAttack", [Objects(set)]) => {
            Some(R::AttackDrags(event_filter(set)?))
        }
        ("MustAttackIfAnotherCreatureAttacks", [Objects(set)]) => {
            Some(R::AttacksWhenAttacking(event_filter(set)?))
        }
        ("CanBlockCreaturesWithShadowAsThoughTheyDidntHaveShadow", [])
        | ("CanBlockCreaturesWithShadowAsThoughItHadShadow", []) => {
            despite(ObjectFilter::Any, BlockEvasion::Shadow)
        }
        ("CanBlockCreaturesWithFlyingAsThoughItHadReach", [Objects(set)]) => {
            despite(event_filter(set)?, BlockEvasion::Flying)
        }
        ("CanBlockCreaturesWithLandwalkAbilitiesAsThoughTheyDidntHaveThem", []) => {
            despite(ObjectFilter::Any, BlockEvasion::Landwalk)
        }
        ("CantBeEnchantedByAnEnchantment", [Objects(set)]) => attached(event_filter(set)?, false),
        ("CantBeEnchantedAndDoesntRemove", [Objects(set)]) => attached(event_filter(set)?, true),
        ("CantBeEnchanted", []) => attached(with_keyword(crate::mtg_types::SubType::Aura), false),
        ("CantBeEquipped", []) => {
            attached(with_keyword(crate::mtg_types::SubType::Equipment), false)
        }
        ("CrewsVehiclesUsingToughnessRatherThanPower", []) => {
            Some(R::TapCostUsesToughness(TapPowerCost::Crew))
        }
        ("SaddlesMountsUsingToughnessRatherThanPower", []) => {
            Some(R::TapCostUsesToughness(TapPowerCost::Saddle))
        }
        ("StationsPermanentsUsingToughnessRatherThanPower", []) => {
            Some(R::TapCostUsesToughness(TapPowerCost::Station))
        }
        ("StationsPermanentsAsThoughPowerWereGreater", [Number(bonus)]) => Some(R::TapPowerBonus {
            cost: TapPowerCost::Station,
            bonus: bonus.clone(),
        }),
        ("CantBeTurnedFaceUp", []) => prohibit(A::TurnFaceUp),
        ("CantBecomeSuspected", []) => prohibit(A::BecomeSuspected),
        ("CantBeBeamedUp", []) => prohibit(A::BeBeamedUp),
        ("CantBeGainedControlOf", []) => prohibit(A::ChangeControl),
        ("PlaneswalkerCantBeAttacked", []) => prohibit(A::BeAttacked),
        ("CantHaveCountersOfTypeRemovedFromIt", [kind]) => {
            prohibit(A::LoseCounters(counter_kind(kind)?))
        }
        ("CantHaveMoreThanNumberCountersOfType", [Number(max), kind]) => Some(R::CounterLimit {
            kind: counter_kind(kind)?,
            max: max.clone(),
        }),
        ("CanBoastTwice", []) => Some(R::ActivationsEachTurn {
            abilities: ObjectFilter::OnStack(StackTrait::Keyword(AbilityKeyword::Boast)),
            times: Expr::Constant(2),
        }),
        ("DecreaseEquipAbilityCostWhenTargetingAPermanent", [amount, Objects(set)]) => {
            Some(R::OwnActivationCost {
                abilities: ObjectFilter::AllOf(vec![
                    ObjectFilter::OnStack(StackTrait::Keyword(AbilityKeyword::Equip)),
                    ObjectFilter::TargetsAny(Box::new(Recipients::Objects(set.clone()))),
                ]),
                change: ActivationCostChange::Reduce(Box::new(amount.clone())),
            })
        }
        (
            "MayAssignCombatDamageDividedAsYouChooseAmongPlayerOrCreaturesAndPlaneswalkers",
            [Operand::Player(player), Objects(set)],
        ) => Some(R::AssignDamageTo {
            players: Some(player.clone()),
            permanents: event_filter(set)?,
        }),
        ("MayAssignCombatDamageToAPermanent", [Objects(set)]) => Some(R::AssignDamageTo {
            players: None,
            permanents: event_filter(set)?,
        }),
        ("MustBeBlockedByExactlyOneDefender", [Objects(_)]) => {
            unique(UniqueObjectRule::BlockedByExactlyOne)
        }
        ("IsAColorlessSourceOfDamage", []) => unique(UniqueObjectRule::ColorlessSourceOfDamage),
        ("CantBecomeTappedUnlessItIsBeingDeclaredAsAnAttacker", []) => {
            unique(UniqueObjectRule::TapsOnlyToAttack)
        }
        ("CantBeBlockedUnlessAllDefendersBlockIt", []) => {
            unique(UniqueObjectRule::BlockedOnlyByAll)
        }
        ("CanOnlyBeDestroyedByLethalDamageFromASingleSource", []) => {
            unique(UniqueObjectRule::DestroyedOnlyBySingleSource)
        }
        ("ArentPutIntoGraveyardForHaving0Loyalty", []) => {
            unique(UniqueObjectRule::SurvivesZeroLoyalty)
        }
        ("DamageDealtToItCantBePreventedOrRedirected", []) => {
            unique(UniqueObjectRule::DamageToItUnpreventable)
        }
        ("DetermineLethalDamageUsingPowerRatherThanToughness", []) => {
            unique(UniqueObjectRule::LethalDamageUsesPower)
        }
        ("AssignsCombatDamageAsThoughPowerWerePositive", []) => {
            unique(UniqueObjectRule::AssignsAsThoughPowerPositive)
        }
        _ => None,
    }
}

pub(super) fn mana_form(form: &str, operands: &[Operand]) -> Option<ManaSymbol> {
    use ManaColor::{Black, Blue, Colorless, Green, Red, White};
    let hybrid = |a, b| Some(ManaSymbol::Hybrid(a, b));
    let color = |c| Some(ManaSymbol::Color(c));
    match (form, operands) {
        ("ManaCostGeneric", [Operand::Integer(n)]) => {
            Some(ManaSymbol::Generic((*n).try_into().ok()?))
        }
        ("ManaCostX", []) => Some(ManaSymbol::X),
        ("ManaCostW", []) => color(White),
        ("ManaCostU", []) => color(Blue),
        ("ManaCostB", []) => color(Black),
        ("ManaCostR", []) => color(Red),
        ("ManaCostG", []) => color(Green),
        ("ManaCostC", []) => color(Colorless),
        ("ManaCostS", []) => Some(ManaSymbol::Snow),
        ("ManaCostWU", []) => hybrid(White, Blue),
        ("ManaCostUB", []) => hybrid(Blue, Black),
        ("ManaCostBR", []) => hybrid(Black, Red),
        ("ManaCostRG", []) => hybrid(Red, Green),
        ("ManaCostGW", []) => hybrid(Green, White),
        ("ManaCostWB", []) => hybrid(White, Black),
        ("ManaCostUR", []) => hybrid(Blue, Red),
        ("ManaCostBG", []) => hybrid(Black, Green),
        ("ManaCostRW", []) => hybrid(Red, White),
        ("ManaCostGU", []) => hybrid(Green, Blue),
        _ => None,
    }
}

/// Mana symbols a row left as terms, flat or in lists.
pub(super) fn mana_terms(operands: &[Operand]) -> Option<Vec<ManaSymbol>> {
    let mut symbols = vec![];
    for operand in operands {
        match operand {
            Operand::List(items) => symbols.extend(mana_terms(items)?),
            Operand::Term { form, operands, .. } => symbols.push(mana_form(form, operands)?),
            Operand::Vocabulary { form, .. } => symbols.push(mana_form(form, &[])?),
            _ => return None,
        }
    }
    Some(symbols)
}

pub(super) fn mana_cost(operands: &[Operand]) -> Option<Cost> {
    Some(Cost {
        parts: vec![CostPart::Mana(mana_terms(operands)?)],
    })
}

pub(super) fn vocabulary_value<T: serde::de::DeserializeOwned>(operand: &Operand) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(vocabulary_form(operand)?.into())).ok()
}

/// A color a card effect sets or adds, chosen or listed.
pub(super) fn settable_color(operand: &Operand, mode: ChangeMode) -> Option<Change> {
    let Operand::Term { form, operands, .. } = operand else {
        return None;
    };
    match (form.as_str(), operands.as_slice()) {
        ("TheChosenColor", []) => Some(Change::ChosenColor { mode }),
        ("Colorless", []) => Some(Change::Colors {
            mode,
            colors: vec![],
        }),
        ("SimpleColorList", colors) => Some(Change::Colors {
            mode,
            colors: colors.iter().map(color_form).collect::<Option<_>>()?,
        }),
        _ => None,
    }
}

pub(super) fn keyword_for(mechanic: Mechanic, cost: Option<Cost>, amount: Option<Expr>) -> Ability {
    Ability::Mechanic {
        mechanic,
        cost,
        amount,
        filter: None,
    }
}

/// Entry flags that say what the entering object is or how long its exile lasts.
pub(super) fn reduced_entry_flag(form: &str, operands: &[Operand]) -> Option<EntryState> {
    use Operand::{Duration as Lasting, List, Objects};
    let face_down = |pt: &Operand, kind: Option<&Operand>, artifact| {
        crate::source_values::face_down(pt, kind, artifact).ok()
    };
    Some(match (form, operands) {
        ("EntersAsFaceDownArtifactCreature", [pt, kind]) => {
            EntryState::FaceDownAs(face_down(pt, Some(kind), true)?)
        }
        ("EntersAsFaceDownCreatureWithAbilitiesAndNotedName", [pt, List(abilities), named]) => {
            let Operand::Term { operands, .. } = named else {
                return None;
            };
            let [Operand::Object(card)] = operands.as_slice() else {
                return None;
            };
            EntryState::FaceDownWith {
                face_down: face_down(pt, None, false)?,
                abilities: abilities
                    .iter()
                    .map(|a| {
                        if let Operand::Ability(a) = a {
                            Some((**a).clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Option<Vec<_>>>()?,
                noted: Some(card.clone()),
            }
        }
        ("EntersAsFaceDownLand", [land]) => {
            EntryState::FaceDownLand(crate::source_values::word(land, "LandType").ok()?)
        }
        ("EntersAsNonAuraEnchantment", []) => EntryState::NotAnAura,
        ("EntersWithNotedCounters", []) => EntryState::NotedCounters,
        ("EntersWithPerpetualEffect", effects) => {
            EntryState::Perpetual(perpetual_changes(effects)?)
        }
        ("MayActions", flags) if !flags.is_empty() => EntryState::Optional {
            decider: PlayerRef::AbilityController,
            entry: Box::new(crate::movement::entry_from_source(flags)),
        },
        ("Limited", [Lasting(until)]) => EntryState::ExiledFor(until.clone()),
        ("LimitedWithTriggerEnters", [Lasting(until), Objects(objects), body]) => {
            EntryState::ExiledForWith {
                until: until.clone(),
                objects: objects.clone(),
                body: if let Operand::Program(program) = body {
                    program.clone()
                } else {
                    return None;
                },
            }
        }
        ("EntersWithRuleEffectUntil", [List(rules), Lasting(until)]) => EntryState::ChangesUntil {
            changes: crate::source_values::rules(rules).ok()?,
            duration: until.clone(),
        },
        _ => return None,
    })
}

pub fn reduce_entry_flag(name: String, operands: Vec<Operand>) -> EntryState {
    let reduced = name
        .split_once("::")
        .and_then(|(_, form)| reduced_entry_flag(form, &operands));
    reduced.unwrap_or(EntryState::Draft { name, operands })
}
