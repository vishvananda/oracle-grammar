//! Extracted portable semantic contracts.
use super::*;
/// Structural equality of typed references, without imposing Eq on the entire
/// recursive query/expression language. This compares data, never source text.
pub fn same_player(a: &PlayerRef, b: &PlayerRef) -> bool {
    serde_json::to_value(a).expect("typed player") == serde_json::to_value(b).expect("typed player")
}

pub fn shuffle_closes(library: &PlayerRef, players: &PlayerSet) -> bool {
    match players {
        PlayerSet::One(player) => same_player(library, player),
        PlayerSet::All => true,
        PlayerSet::Union(sets) => sets.iter().any(|s| shuffle_closes(library, s)),
        _ => false,
    }
}

/// References available in the body of a replacement on this event.
pub fn replacement_bindings(event: &ReplacementEvent) -> EventBindings {
    match event {
        ReplacementEvent::ZoneChange { objects, from, to } => {
            let mut bindings = event_bindings(&EventSpec::ZoneChange {
                objects: objects.clone(),
                from: from.clone(),
                to: to.clone(),
            });
            // A proposed departure has not granted the post-departure Aura exception.
            bindings.enchanted_left = false;
            bindings
        }
        ReplacementEvent::Primitive { name, operands } => event_bindings(&EventSpec::Primitive {
            name: name.clone(),
            operands: operands.clone(),
        }),
        ReplacementEvent::CopySpell { .. } => {
            row_bindings("ReplacableEventWouldCopyASpell::APlayerWouldCopyASpellAnyNumberOfTimes")
        }
        ReplacementEvent::Countered { .. } => {
            row_bindings("ReplacableEventWouldCounterASpell::ASpellOrAbilityWouldCounterASpell")
        }
        ReplacementEvent::DrawCausedBy { .. } => row_bindings(
            "ReplacableEventWouldDraw::ACyclingAbilityOfAPermanentWouldCausePlayerToDrawACard",
        ),
        ReplacementEvent::ProduceColor { .. } => {
            row_bindings("ReplacableEventWouldProduceMana::TappingPermanentWouldProduceManaOfColor")
        }
        ReplacementEvent::Enter { .. } => EventBindings {
            object: true,
            permanent: true,
            ..Default::default()
        },
        ReplacementEvent::Discard { .. } => EventBindings {
            player: true,
            discard: true,
            discarded: true,
            ..Default::default()
        },
        ReplacementEvent::Measured { event, .. } | ReplacementEvent::EachPoint(event) => {
            replacement_bindings(event)
        }
        ReplacementEvent::Caused { event, .. } => EventBindings {
            stack_object: true,
            ..replacement_bindings(event)
        },
        ReplacementEvent::PutOntoBattlefield { .. } => EventBindings {
            object: true,
            permanent: true,
            player: true,
            ..Default::default()
        },
        ReplacementEvent::Damage(damage) => event_bindings(&EventSpec::Damage(damage.clone())),
        ReplacementEvent::Player { event, .. } => EventBindings {
            player: true,
            amount: matches!(
                event,
                ReplacedPlayerEvent::GainLife
                    | ReplacedPlayerEvent::GainLifeByEffect
                    | ReplacedPlayerEvent::PayLife
                    | ReplacedPlayerEvent::LoseLife
                    | ReplacedPlayerEvent::GetEnergy
                    | ReplacedPlayerEvent::Mill
                    | ReplacedPlayerEvent::Scry
                    | ReplacedPlayerEvent::DrawCards
            ),
            ..Default::default()
        },
        ReplacementEvent::Object { .. } => EventBindings {
            object: true,
            permanent: true,
            ..Default::default()
        },
        ReplacementEvent::Tokens { .. } => EventBindings {
            amount: true,
            ..EventBindings::default()
        },
        ReplacementEvent::Counters { players, .. } => EventBindings {
            object: players.is_none(),
            permanent: players.is_none(),
            amount: true,
            ..Default::default()
        },
        ReplacementEvent::ProduceMana { .. } => EventBindings {
            amount: true,
            ..Default::default()
        },
        ReplacementEvent::FirstEachTurn(event) => replacement_bindings(event),
        ReplacementEvent::Either(events) => events
            .iter()
            .map(replacement_bindings)
            .reduce(EventBindings::shared)
            .unwrap_or_default(),
    }
}

/// Whether an operation affects at most one object, given which earlier results do.
pub fn singular_operation(operation: &Operation, single: impl Fn(&ResultId) -> bool) -> bool {
    let operation = operation.action();
    let one = |count: &Count| {
        matches!(
            count,
            Count::Exactly(Expr::Constant(1))
                | Count::AsManyAsCan(Expr::Constant(1))
                | Count::UpTo(Expr::Constant(1))
        )
    };
    let chosen = |objects: &ObjectSet| match objects {
        ObjectSet::One(_) => true,
        ObjectSet::Selected(result) => single(result),
        _ => false,
    };
    match operation {
        Operation::ChooseObjects { count, .. } => one(count),
        Operation::Sacrifice {
            objects, choose, ..
        } => choose.as_ref().map_or_else(|| chosen(objects), one),
        Operation::SetTapped { objects, .. } | Operation::Destroy { objects, .. } => {
            chosen(objects)
        }
        Operation::Discard {
            players: PlayerSet::One(_),
            amount: Expr::Constant(1),
            ..
        }
        | Operation::Draw {
            players: PlayerSet::One(_),
            amount: Expr::Constant(1),
        } => true,
        Operation::DiscardObjects { objects } => chosen(objects),
        Operation::CreateTokens {
            players: PlayerSet::One(_),
            tokens,
            ..
        } => matches!(
            tokens.as_slice(),
            [TokenSpec {
                amount: Expr::Constant(1),
                ..
            }]
        ),
        _ => matches!(
            yields(operation),
            Some(
                "observed"
                    | "exiled"
                    | "entered"
                    | "hand"
                    | "extra_turn"
                    | "created"
                    | "amassed"
                    | "paid"
                    | "graveyard"
            )
        ),
    }
}

/// Manifesting puts one card onto the battlefield (701.40a), as a reviewed
/// primitive with no ordinary movement to read the permanent from.
/// What a reviewed action row leaves for a later "this way" to name.
pub fn yields(operation: &Operation) -> Option<&'static str> {
    if matches!(operation, Operation::RevealFromHand { .. }) {
        return Some("observed");
    }
    let operation = operation.action();
    // Every manifest/cloak source supplies the same object-transition result,
    // including an inline hand choice and multiple cards. Cardinality is a
    // separate concern; consumers must not infer a singular object from this.
    if matches!(operation, Operation::Manifest { .. }) {
        return crate::primitive_spec::lookup("Action::ManifestTheTopCardOfPlayersLibrary")?
            .yields
            .as_deref();
    }
    if matches!(operation, Operation::PutChosenCounters { .. }) {
        return Some("counters");
    }
    if let Operation::DraftChangeZone { destination, .. } = operation {
        return Some(match destination {
            Destination::OwnersHand | Destination::Hand(_) => "hand",
            Destination::OwnersGraveyard => "graveyard",
            Destination::Exile => "exiled",
            Destination::Battlefield(_) | Destination::OwnersBattlefield => "entered",
            _ => "moved",
        });
    }
    let name = match operation {
        Operation::Primitive { name, .. } => name.as_str(),
        Operation::ModifyEvent {
            change: EventChange::Entry(BattlefieldState::CopyOf { .. }),
        } => "ReplacementActionWouldEnter::EnterAsACopyOfACardInAPlayersGraveyard",
        Operation::Reveal {
            objects: ObjectSet::One(_) | ObjectSet::LibraryTops(_) | ObjectSet::Take { .. },
        } => "Action::RevealFaceDownPermanent",
        Operation::Exchange {
            kind: ExchangeKind::Control,
            until: Some(_),
            ..
        } => "Action::ExchangeControlOfEachPermanentWithPlayerUntil",
        Operation::ChooseValue {
            kind: ValueKind::Object,
            terms,
            ..
        } if terms.secret && matches!(terms.options.as_slice(), [Operand::Filter(_)]) => {
            "Action::SecretlyChooseAGraveyardCard"
        }
        Operation::Keyword { action, .. } => {
            KEYWORD_ACTIONS
                .iter()
                .find(|(_, row)| *row == crate::keyword_actions::parts(action).kind)?
                .0
        }
        Operation::Conjure {
            destination: ConjureDestination::Hand(_),
            ..
        } => "Action::ConjureCardIntoHand",
        Operation::Conjure {
            destination: ConjureDestination::Battlefield(_),
            ..
        } => "Action::ConjureCardOntoBattlefield",
        Operation::KeywordEach {
            action: ObjectKeywordAction::Goad { .. },
            ..
        } => "Action::GoadEachCreature",
        Operation::PayAnyAmount {
            resource: PaidResource::Energy,
            ..
        } => return None,
        Operation::PayAnyAmount { .. } => "Action::PayAnyAmountOfMana",
        Operation::Seek { top: None, .. } => "Action::SeekCards",
        Operation::ChooseObjects {
            objects: ObjectSet::AnyZone(_) | ObjectSet::FromHistory { .. },
            ..
        } => "Action::ChooseASpellThatResolvedThisTurn",
        Operation::ChooseObjects {
            objects: ObjectSet::Union(_),
            ..
        } => "Action::ChooseACommanderOnTheBattlefieldOrInTheCommandZone",
        Operation::ChooseDefender { .. } => {
            "Action::ChooseAPlayerOrPlaneswalkerCurrentlyAttackedByPlayer"
        }
        Operation::EachPlayer { action, .. }
            if matches!(**action, Operation::GiveControl { .. }) =>
        {
            "Action::GainControlOfAPermanentControlledByEachPlayerUntil"
        }
        Operation::MillUntil { .. } => {
            "Action::MillCardsUntilACardOfTypeIsMilledOrUntilNumberCardsHaveBeenPutIntoGraveyardThisWay"
        }
        Operation::DrawUpTo { .. } => "Action::DrawUptoNumberCards",
        Operation::DiscardAllBut { .. } => "Action::DiscardAllButNumberCards",
        Operation::RevealFromHand {
            count: Some(Expr::Constant(1)),
            ..
        } => "Action::RevealACardFromHand",
        Operation::DraftFromSpellbook { .. } => "Action::DraftACardFromSpellBook",
        Operation::TowardPlayer {
            action: PlayerPairAction::ExchangeLife,
            ..
        } => "Action::ExchangeLifeTotalWithPlayer",
        Operation::Manifest {
            source:
                ManifestSource::TopOfLibrary {
                    count: Expr::Constant(1),
                    ..
                },
            ..
        } => "Action::ManifestTheTopCardOfPlayersLibrary",
        Operation::Manifest {
            source: ManifestSource::Cards(ObjectSet::One(_)),
            ..
        } => "Action::CloakCardFromHand",
        _ => return None,
    };
    crate::primitive_spec::lookup(name)?.yields.as_deref()
}

/// Action rows that are one keyword action. Each keeps its reviewed `yields`.
pub fn requires_typed_keyword(name: &str) -> bool {
    KEYWORD_ACTIONS.iter().any(|(row, _)| *row == name)
}

/// A reviewed row that makes a card a later reference can read.
pub fn makes_card(operation: &Operation) -> bool {
    let operation = operation.action();
    matches!(
        operation,
        Operation::Conjure { .. } | Operation::DraftFromSpellbook { .. }
    ) || matches!(operation, Operation::Primitive { name, .. }
            if name.starts_with("Action::Conjure") || name.starts_with("Action::Draft"))
}

/// The kind of value a variable form reads, when it is the one chosen.
pub(super) fn chosen_kind(operand: &Operand) -> Option<ValueKind> {
    let form = match operand {
        Operand::Term { form, .. } | Operand::Vocabulary { form, .. } => form,
        _ => return None,
    };
    match form.as_str() {
        "TheChosenCreatureType" | "TheChosenCreatureTypes" => Some(ValueKind::CreatureType),
        "TheChosenLandType" => Some(ValueKind::LandType),
        "TheChosenCardtype" => Some(ValueKind::CardType),
        "TheChosenPlaneswalkerType" => Some(ValueKind::PlaneswalkerType),
        _ => None,
    }
}

/// A row's change in generic form, or the row itself when it has none.
pub fn reduce_changes(name: String, operands: Vec<Operand>) -> Vec<Change> {
    if matches!(
        name.split_once("::"),
        Some((
            "LayerEffect" | "StaticLayerEffect" | "SpellEffect",
            "AddAbilityAndLoseAllOtherAbilities"
        ))
    ) && !operands.is_empty()
        && let Some(abilities) = ability_list(&operands)
    {
        return crate::change_lists::replace_abilities(abilities);
    }
    vec![reduce_single_change(name, operands)]
}

/// A single ordinary change. Compound source forms are handled at the list edge.
pub(super) fn reduce_single_change(name: String, operands: Vec<Operand>) -> Change {
    match (name.as_str(), operands.as_slice()) {
        ("SettableColor::TheManaColorChosenThisWay", [Operand::Result(mana)]) => {
            Change::ColorOfMana {
                mode: ChangeMode::Set,
                mana: mana.clone(),
            }
        }
        ("LayerEffect::SwitchPT", []) => Change::SwitchPowerToughness,
        (
            "LayerEffect::AddCreatureTypeVariable" | "LayerEffect::AddLandTypeVariable",
            [variable],
        ) if chosen_kind(variable).is_some() => Change::ChosenType {
            kind: chosen_kind(variable).unwrap(),
            mode: ChangeMode::Add,
        },
        (
            "LayerEffect::SetCreatureTypeVariable" | "LayerEffect::SetLandTypeVariable",
            [variable],
        ) if chosen_kind(variable).is_some() => Change::ChosenType {
            kind: chosen_kind(variable).unwrap(),
            mode: ChangeMode::Set,
        },
        ("LayerEffect::LosesAbility", [ability]) => Change::LosesAbility(Box::new(ability.clone())),
        ("LayerEffect::SetLandType", [land]) => Change::SetLandType(Box::new(land.clone())),
        ("LayerEffect::SetName", [Operand::Text(name)]) => Change::SetName(name.clone()),
        ("LayerEffect::IsACopyOfExiledCard", [Operand::Object(object), effects]) => {
            copy_of(object.clone(), effects).unwrap_or_else(|| Change::Primitive {
                name: name.clone(),
                operands: operands.clone(),
            })
        }
        ("SpellEffect::Convoke", []) => Change::GrantAbility(Box::new(Ability::Mechanic {
            mechanic: Mechanic::Convoke,
            cost: None,
            amount: None,
            filter: None,
        })),
        ("SpellEffect::CantBeCast", []) => Change::Rule(RuleChange::Prohibit(ObjectAction::Cast)),
        (
            "LayerEffect::IsACopyOfLibraryCard",
            [Operand::Objects(ObjectSet::Selected(result)), effects],
        ) => copy_of(
            ObjectRef::Result {
                result: result.clone(),
                phase: ObjectPhase::After,
                index: 0,
            },
            effects,
        )
        .unwrap_or_else(|| Change::Primitive {
            name: name.clone(),
            operands: operands.clone(),
        }),
        ("LayerEffect::IsACopyOfGraveyardCard", [Operand::Object(object), effects]) => {
            copy_of(object.clone(), effects).unwrap_or_else(|| Change::Primitive {
                name: name.clone(),
                operands: operands.clone(),
            })
        }
        (
            "StaticLayerEffect::AddAbilityFromCardsInGraveyardsHasable",
            [Operand::Filter(cards), kinds],
        ) => gain_abilities(
            ObjectSet::Zone {
                scope: ZoneScope::Players {
                    zone: EventZone::Graveyard,
                    players: PlayerSet::All,
                },
                kind: ObjectKind::Card,
                filter: cards.clone(),
            },
            kinds,
        ),
        (
            "LayerEffect::AddAbilityFromPermanentHasable"
            | "LayerEffect::AddAbilityFromGraveyardCardHasable"
            | "StaticLayerEffect::AddAbilityFromExiledHasable",
            [Operand::Object(object), kinds],
        ) => gain_abilities(ObjectSet::One(object.clone()), kinds),
        (
            "LayerEffect::AddAbilityFromEachPermanentHasable"
            | "StaticLayerEffect::AddAbilityFromAnExiledHasable",
            [Operand::Objects(objects), kinds],
        ) => gain_abilities(objects.clone(), kinds),
        (
            "LayerEffect::ReplaceColorWordVariableWithNewColorWordVariable"
            | "LayerEffect::ReplaceLandTypeVariableWithNewLandTypeVariable",
            [from, to],
        ) => Change::ReplaceWord {
            from: Box::new(from.clone()),
            to: Box::new(to.clone()),
        },
        ("LayerEffect::AddEnchantmentType", [kind]) => {
            Change::AddEnchantmentType(Box::new(kind.clone()))
        }
        ("LayerEffect::IsACopyOfPermanent", [Operand::Object(object), effects]) => {
            copy_of(object.clone(), effects).unwrap_or_else(|| Change::Primitive {
                name: name.clone(),
                operands: operands.clone(),
            })
        }
        _ => {
            let layer = name
                .split_once("::")
                .filter(|(family, _)| {
                    matches!(*family, "LayerEffect" | "StaticLayerEffect" | "SpellEffect")
                })
                .and_then(|(_, form)| {
                    reduce_layer(form, &operands).or_else(|| reduce_card_layer(form, &operands))
                });
            layer.unwrap_or(Change::Primitive { name, operands })
        }
    }
}

/// A row's rule about an object in generic form, or the row itself.
pub fn reduce_rule(name: String, operands: Vec<Operand>) -> RuleChange {
    let not_spell = || ObjectFilter::Not(Box::new(ObjectFilter::OnStack(StackTrait::Spell)));
    let bare = name.strip_prefix("PermanentRule::").unwrap_or(&name);
    match (bare, operands.as_slice()) {
        ("IsGoadedByPlayer", [Operand::Player(player)]) => RuleChange::GoadedBy(player.clone()),
        ("MustAttackPlayer", [Operand::Player(player)]) => RuleChange::RequireCombat {
            action: ObjectAction::Attack(AttackDefenders::Players(PlayerSet::One(player.clone()))),
            amount: RequirementAmount::AtLeast(Expr::Constant(1)),
        },
        ("CantAttackIfDefendingPlayer", [Operand::Condition(condition)]) => {
            RuleChange::CantAttackDefenderIf(condition.clone())
        }
        ("TheLegendsRuleDoesntApply", []) => RuleChange::LegendRuleExempt,
        ("CantBeTheTargetOfSpells", [Operand::Filter(spells)]) => {
            RuleChange::Prohibit(ObjectAction::BeTargeted(ObjectFilter::AllOf(vec![
                ObjectFilter::OnStack(StackTrait::Spell),
                spells.clone(),
            ])))
        }
        ("CantBeTheTargetOfAbilities", [abilities]) if stack_filter(abilities).is_some() => {
            RuleChange::Prohibit(ObjectAction::BeTargeted(ObjectFilter::AllOf(vec![
                not_spell(),
                stack_filter(abilities).unwrap(),
            ])))
        }
        ("CantBeTheTargetOfSpellsOrAbilities", [sources]) if stack_filter(sources).is_some() => {
            RuleChange::Prohibit(ObjectAction::BeTargeted(stack_filter(sources).unwrap()))
        }
        ("CantHaveCountersPutOnIt", []) => {
            RuleChange::Prohibit(ObjectAction::ReceiveCounters(None))
        }
        ("CantHaveCountersOfTypePutOnIt", [kind]) if counter_kind(kind).is_some() => {
            RuleChange::Prohibit(ObjectAction::ReceiveCounters(counter_kind(kind)))
        }
        ("CantTransform", []) => RuleChange::Prohibit(ObjectAction::Transform),
        ("CantPhaseOut", []) => RuleChange::Prohibit(ObjectAction::PhaseOut),
        ("CantCrew", []) => RuleChange::Prohibit(ObjectAction::Crew),
        ("CrewsVehiclesAsThoughPowerWereGreater", [Operand::Number(bonus)]) => {
            RuleChange::TapPowerBonus {
                cost: TapPowerCost::Crew,
                bonus: bonus.clone(),
            }
        }
        ("SaddlesMountsAsThoughPowerWereGreater", [Operand::Number(bonus)]) => {
            RuleChange::TapPowerBonus {
                cost: TapPowerCost::Saddle,
                bonus: bonus.clone(),
            }
        }
        ("MayBeExertedAsItAttacks", []) => RuleChange::MayExert(None),
        ("MayBeExertedAsItAttacksWithTrigger", [then]) => {
            RuleChange::MayExert(Some(Box::new(then.clone())))
        }
        ("CanBeBlockedAsThoughItDidntHave", [ability]) => {
            RuleChange::BlockedAsThoughLacking(Box::new(ability.clone()))
        }
        ("CantGainAbility", [ability]) => RuleChange::CantGainAbility(Box::new(ability.clone())),
        _ => match reduce_object_static(bare, &operands) {
            Some(rule) => rule,
            None => RuleChange::Primitive { name, operands },
        },
    }
}

pub(super) fn ability_list(operands: &[Operand]) -> Option<Vec<Ability>> {
    operands
        .iter()
        .map(|operand| match operand {
            Operand::Ability(ability) => Some((**ability).clone()),
            _ => None,
        })
        .collect()
}

/// What a reviewed row says its body can name.
pub fn row_bindings(name: &str) -> EventBindings {
    let binds = crate::primitive_spec::lookup(name).map_or(&[][..], |r| &r.binds);
    let has = |field: &str| binds.iter().any(|b| b == field);
    EventBindings {
        object: has("object"),
        permanent: has("permanent"),
        spell: has("spell"),
        player: has("player"),
        amount: has("amount"),
        block_pair: has("block_pair"),
        group: has("group"),
        stack_object: has("stack_object"),
        other: has("other"),
        other_player: has("other_player"),
        moved: has("moved"),
        ..EventBindings::default()
    }
}
