//! Extracted portable semantic contracts.
use super::*;
pub(super) fn event_bindings_case_2(event: &EventSpec) -> EventBindings {
    match event {
        EventSpec::Damage(event) => {
            let source = matches!(
                event.grouping,
                DamageGrouping::Source | DamageGrouping::SourceRecipient
            );
            let recipient = matches!(
                event.grouping,
                DamageGrouping::Recipient | DamageGrouping::SourceRecipient
            );
            let objects = matches!(event.recipients, DamageRecipients::Objects(_));
            let players = matches!(event.recipients, DamageRecipients::Players(_));
            let object = match event.subject {
                DamageSubject::Source => source,
                DamageSubject::Recipient => recipient && objects,
            };
            EventBindings {
                object,
                permanent: object
                    && match event.subject {
                        DamageSubject::Source => matches!(event.source_kind, ObjectKind::Permanent),
                        DamageSubject::Recipient => true,
                    },
                spell: object
                    && matches!(event.subject, DamageSubject::Source)
                    && matches!(event.source_kind, ObjectKind::Spell),
                player: recipient && players,
                amount: true,
                counters_placed: false,
                moved: false,
                moved_to_hand: false,
                ring_choice: false,
                damage: true,
                damage_source: source,
                damage_source_permanent: source
                    && matches!(event.source_kind, ObjectKind::Permanent),
                damage_recipient: recipient && objects,
                damage_any_recipient: recipient,
                block_pair: false,
                group: false,
                stack_object: false,
                other: false,
                other_player: false,
                moved_owner: false,
                enchanted_left: false,
                damage_objects: !players,
                damage_players: !objects,
                damage_subject: Some(event.subject),
                discard: false,
                discarded: false,
                activation_x: false,
            }
        }
        EventSpec::Spell { .. } => EventBindings {
            object: true,
            spell: true,
            player: true,
            ..EventBindings::default()
        },
        EventSpec::Activated { .. } => EventBindings {
            object: true,
            permanent: true,
            player: true,
            stack_object: true,
            ..EventBindings::default()
        },
        EventSpec::Turn { players, .. } => EventBindings {
            object: matches!(players, PlayerSet::One(PlayerRef::ControllerOf(_))),
            permanent: matches!(players, PlayerSet::One(PlayerRef::ControllerOf(_))),
            player: true,
            ..EventBindings::default()
        },
        EventSpec::State { .. } => EventBindings::default(),
        EventSpec::StateWithPlayer { .. } => EventBindings {
            player: true,
            ..EventBindings::default()
        },
        EventSpec::WithOtherObject { event, .. } => EventBindings {
            other: true,
            ..event_bindings(event)
        },
        EventSpec::Filtered { event, .. } => event_bindings(event),
        EventSpec::AnyOf(events) => events
            .iter()
            .map(event_bindings)
            .reduce(EventBindings::shared)
            .unwrap_or_default(),
        _ => unreachable!("validated dispatch"),
    }
}

/// A filter a row spells in `CardsInHand` terms.
pub(super) fn hand_filter(operand: &Operand) -> Option<ObjectFilter> {
    match operand {
        Operand::Filter(filter) => Some(filter.clone()),
        Operand::Term { form, operands, .. } => match (form.as_str(), operands.as_slice()) {
            ("AnyCard", []) => Some(ObjectFilter::Any),
            ("IsCardtype", [kind]) => Some(ObjectFilter::CardType(vocabulary_value(kind)?)),
            ("IsNonCardtype", [kind]) => Some(ObjectFilter::Not(Box::new(ObjectFilter::CardType(
                vocabulary_value(kind)?,
            )))),
            _ => None,
        },
        _ => None,
    }
}

/// A power or toughness change a `ModX` term spells, given X.
pub(super) fn mod_x(term: &Operand, x: &Expr) -> Option<Expr> {
    match term {
        Operand::Term { form, operands, .. } => match (form.as_str(), operands.as_slice()) {
            ("PlusX", []) => Some(x.clone()),
            ("MinusX", []) => Some(Expr::multiply(Expr::Constant(-1), x.clone())),
            ("Zero", []) => Some(Expr::Constant(0)),
            ("Integer", [Operand::Integer(n)]) => Some(Expr::Constant(*n)),
            _ => None,
        },
        _ => None,
    }
}

/// A base power and toughness a `PT` term spells.
pub(super) fn set_pt(term: &Operand) -> Option<Change> {
    use Operand::{Integer, Number, Term};
    let Term { form, operands, .. } = term else {
        return None;
    };
    let (power, toughness) = match (form.as_str(), operands.as_slice()) {
        ("PT", [Integer(power), Integer(toughness)]) => {
            (Expr::Constant(*power), Expr::Constant(*toughness))
        }
        ("PTX", [power, toughness, Number(x)]) => {
            let component = |term: &Operand| match term {
                Term { form, operands, .. } => match (form.as_str(), operands.as_slice()) {
                    ("X", []) => Some(x.clone()),
                    ("Integer", [Integer(n)]) => Some(Expr::Constant(*n)),
                    _ => None,
                },
                _ => None,
            };
            (component(power)?, component(toughness)?)
        }
        ("ManualPT", [Number(power), Number(toughness)]) => (power.clone(), toughness.clone()),
        _ => return None,
    };
    Some(Change::SetBasePowerToughness { power, toughness })
}

/// The enclosing scope plus what a would-be event binds, for its own amount.
pub fn with_replacement(outer: &EventBindings, event: &ReplacementEvent) -> EventBindings {
    with_bindings(outer, &replacement_bindings(event))
}

/// Whether a delayed trigger's body reads the event it waits for, not its creator's.
pub fn delayed_reads_event(event: &EventSpec) -> bool {
    matches!(
        event,
        EventSpec::Primitive { .. }
            | EventSpec::Activated { .. }
            | EventSpec::SpellOrActivation { .. }
    )
}

/// Changes to objects and spells, past `reduce_layer`'s table.
pub(super) fn reduce_card_layer(form: &str, operands: &[Operand]) -> Option<Change> {
    use Operand::{Ability as Held, Cost as Paid, Filter, Player, Term, Text};
    let grant = |ability| Some(Change::GrantAbility(Box::new(ability)));
    match (form, operands) {
        (
            "AddAbility_ActivatedWithModifiers",
            [
                Paid(cost),
                Held(ability),
                Term {
                    form: modifier,
                    operands: of,
                    ..
                },
            ],
        ) => {
            let (Ability::Spell { announcement, body }, "OnlyPlayerMayActivate", [Player(player)]) =
                (&**ability, modifier.as_str(), of.as_slice())
            else {
                return None;
            };
            grant(Ability::Activated {
                origin: ActivationOrigin::Ordinary,
                cost: cost.clone(),
                limits: ActivationLimits {
                    once: false,
                    once_each_turn: false,
                    sorcery: false,
                    condition: None,
                    activators: None,
                    keyword: None,
                    modifiers: vec![Change::ActivationRule(ActivationRule::OnlyBy(
                        PlayerSet::One(player.clone()),
                    ))],
                },
                announcement: announcement.clone(),
                body: body.clone(),
            })
        }
        ("Freerunning", symbols) if !symbols.is_empty() => grant(keyword_for(
            Mechanic::Freerunning,
            Some(mana_cost(symbols)?),
            None,
        )),
        ("StickerKicker", [Paid(cost)]) => grant(keyword_for(
            Mechanic::StickerKicker,
            Some(cost.clone()),
            None,
        )),
        ("AdditionalCostForEachColorManaSymbolInCosts", [Paid(cost), color]) => Some(
            Change::SpellCost(SpellCostModifier::Increase(repeated_cost(
                cost,
                Expr::CountManaSymbols {
                    objects: Box::new(ObjectSet::One(ObjectRef::Affected)),
                    colors: vec![color_form(color)?],
                },
            ))),
        ),
        (
            "AddAbilityFromCardsRemovedFromDraftWithCardsNamedHasable",
            [Filter(cards), Text(card), kinds],
        ) => Some(gain_abilities(
            ObjectSet::OutsideTheGame(ObjectFilter::AllOf(vec![
                cards.clone(),
                ObjectFilter::DraftNoted(draft_note(
                    DraftNoteKind::Removed,
                    &PlayerRef::AbilityController,
                    card,
                )),
            ])),
            kinds,
        )),
        _ => None,
    }
}

/// `cost`, paid `times` times over.
pub(super) fn repeated_cost(cost: &Cost, times: Expr) -> Cost {
    Cost {
        parts: cost
            .parts
            .iter()
            .map(|part| CostPart::Repeated {
                part: Box::new(part.clone()),
                times: times.clone(),
            })
            .collect(),
    }
}

/// One or more cost symbols a row left as terms, scaled.
pub(super) fn reduction_terms(operand: &Operand, factor: &Expr) -> Option<Vec<ManaReduction>> {
    let items = match operand {
        Operand::List(items) => items.as_slice(),
        single => std::slice::from_ref(single),
    };
    items
        .iter()
        .map(|item| {
            let (form, operands): (&str, &[Operand]) = match item {
                Operand::Term { form, operands, .. } => (form, operands),
                Operand::Vocabulary { form, .. } => (form, &[]),
                _ => return None,
            };
            let (color, amount) = match (form, operands) {
                ("CostReduceGeneric", [Operand::Integer(n)]) if *n >= 0 => (None, *n),
                ("CostReduceX", []) => (None, 1),
                ("CostReduceW", []) => (Some(ManaColor::White), 1),
                ("CostReduceU", []) => (Some(ManaColor::Blue), 1),
                ("CostReduceB", []) => (Some(ManaColor::Black), 1),
                ("CostReduceR", []) => (Some(ManaColor::Red), 1),
                ("CostReduceG", []) => (Some(ManaColor::Green), 1),
                _ => return None,
            };
            Some(ManaReduction {
                color,
                amount: Expr::multiply(Expr::Constant(amount), factor.clone()),
            })
        })
        .collect()
}

pub(super) fn reduced_cost(operand: &Operand, factor: &Expr) -> Option<SpellCostModifier> {
    Some(SpellCostModifier::Reduce {
        reductions: reduction_terms(operand, factor)?,
        spill_into_generic: true,
    })
}

pub(super) fn gain_abilities(objects: ObjectSet, kinds: &Operand) -> Change {
    Change::GainAbilitiesOf {
        objects,
        kinds: Box::new(kinds.clone()),
        once_each_turn: false,
        spend_as: None,
    }
}

/// The objects a term of activated abilities reads.
pub(super) fn activated_sources(abilities: &Operand) -> Option<ObjectSet> {
    let Operand::Term { form, operands, .. } = abilities else {
        return None;
    };
    match (form.as_str(), operands.as_slice()) {
        ("AbilityOfAPermanent", [Operand::Objects(objects)]) => Some(objects.clone()),
        ("AbilityOfPermanent", [Operand::Object(object)]) => Some(ObjectSet::One(object.clone())),
        ("And", items) => items.iter().find_map(activated_sources),
        _ => None,
    }
}

/// Activated abilities, with leave to spend mana on them differently.
pub(super) fn activated_with_spending(abilities: &Operand, spend_as: SpendAs) -> Option<Change> {
    let narrowed = matches!(abilities, Operand::Term { form, .. } if form == "And");
    Some(Change::GainAbilitiesOf {
        objects: activated_sources(abilities)?,
        kinds: Box::new(if narrowed {
            abilities.clone()
        } else {
            Operand::Term {
                family: "CheckHasable".into(),
                form: "ActivatedAbility".into(),
                operands: vec![],
            }
        }),
        once_each_turn: false,
        spend_as: Some(spend_as),
    })
}

pub(super) fn copy_of(object: ObjectRef, effects: &Operand) -> Option<Change> {
    Some(Change::CopyOf {
        object,
        exceptions: crate::copying::from_source(std::slice::from_ref(effects)).ok()?,
    })
}

pub(super) fn earlier_object(what: EarlierObjects) -> ObjectRef {
    ObjectRef::Earlier { what, by: None }
}

pub(super) fn type_mode(form: &str) -> Option<ChangeMode> {
    if form.starts_with("Add") {
        Some(ChangeMode::Add)
    } else if form.starts_with("Set") {
        Some(ChangeMode::Set)
    } else {
        None
    }
}

/// A chosen, listed or borrowed type a change adds or sets.
pub(super) fn type_variable(form: &str, variable: &Operand) -> Option<Change> {
    let mode = type_mode(form)?;
    if let Some(kind) = chosen_kind(variable) {
        return Some(Change::ChosenType { kind, mode });
    }
    let Operand::Term { form, operands, .. } = variable else {
        return None;
    };
    match (form.as_str(), operands.as_slice()) {
        ("EachBasicLandType", []) => Some(Change::LandTypes {
            scope: LandTypeScope::Basic,
            mode,
        }),
        ("CreatureTypesOfExiled", [Operand::Object(object)]) => Some(Change::CreatureTypesOf {
            object: object.clone(),
            mode,
        }),
        _ => None,
    }
}

/// A continuous change a layer, static or spell row names, by what it changes.
pub(super) fn reduce_layer(form: &str, operands: &[Operand]) -> Option<Change> {
    let one = Expr::Constant(1);
    let cards_of = |zone, player: &PlayerRef, filter: &ObjectFilter| ObjectSet::Zone {
        scope: ZoneScope::Players {
            zone,
            players: PlayerSet::One(player.clone()),
        },
        kind: ObjectKind::Card,
        filter: filter.clone(),
    };
    Some(match (form, operands) {
        (
            "AddCreatureTypeVariable"
            | "AddLandTypeVariable"
            | "SetCreatureTypeVariable"
            | "SetLandTypeVariable",
            [variable],
        ) => match (type_variable(form, variable), form) {
            (None, "SetLandTypeVariable") => Change::SetLandType(Box::new(variable.clone())),
            (change, _) => return change,
        },
        ("RemoveAllLandTypes", []) => Change::RemoveAllLandTypes,
        ("HasAllLandTypes", []) => Change::LandTypes {
            scope: LandTypeScope::All,
            mode: ChangeMode::Add,
        },
        ("HasAllCreatureTypes", []) => Change::CreatureTypes {
            mode: ChangeMode::Set,
            types: CreatureTypeSet::All,
        },
        ("AddCreatureType", [kind]) => Change::CreatureTypes {
            mode: ChangeMode::Add,
            types: CreatureTypeSet::Listed(vec![vocabulary_value(kind)?]),
        },
        ("RemoveCreatureType", [kind]) => {
            Change::RemoveSubtype(crate::subtype::creature_type(&vocabulary_value(kind)?))
        }
        ("RemoveArtifactType", [kind]) => {
            Change::RemoveSubtype(crate::subtype::artifact_type(&vocabulary_value(kind)?))
        }
        ("AddCardtype", [kind]) => Change::AddCardType(vocabulary_value(kind)?),
        ("SetColor", [color]) => return settable_color(color, ChangeMode::Set),
        ("AddColor", [color]) => return settable_color(color, ChangeMode::Add),
        ("SetLandTypes", kinds) if !kinds.is_empty() => {
            Change::SetLandType(Box::new(Operand::List(kinds.to_vec())))
        }
        ("ReplaceCreatureTypeVariableWithNewCreatureType", [from, to]) => Change::ReplaceWord {
            from: Box::new(from.clone()),
            to: Box::new(to.clone()),
        },
        ("SetNameToTheChosenName", []) => Change::ChosenType {
            kind: ValueKind::CardName,
            mode: ChangeMode::Set,
        },
        ("HasAllNamesOfNonlegendaryCreatures", []) => Change::AddNames(ObjectFilter::AllOf(vec![
            ObjectFilter::Not(Box::new(ObjectFilter::Supertype(
                crate::mtg_types::SuperType::Legendary,
            ))),
            ObjectFilter::CardType(crate::mtg_types::CardType::Creature),
        ])),
        ("IsAColorlessSourceOfDamage", []) => Change::ColorlessDamageSource,
        ("AdjustPT", [Operand::Integer(power), Operand::Integer(toughness)]) => Change::Pump {
            power: Expr::Constant(*power),
            toughness: Expr::Constant(*toughness),
        },
        ("AddAbilityIfItDoesntHaveIt", [Operand::Ability(ability)]) => {
            Change::GrantAbilityIfAbsent(ability.clone())
        }
        ("AddAbilityFromTopOfLibraryHasable", [kinds]) => gain_abilities(
            ObjectSet::LibraryTops(PlayerSet::One(PlayerRef::AbilityController)),
            kinds,
        ),
        (
            "AddAbilityFromCardsInPlayersGraveyardHasable",
            [Operand::Filter(cards), Operand::Player(owner), kinds],
        ) => gain_abilities(cards_of(EventZone::Graveyard, owner, cards), kinds),
        ("AddAbilityFromCardsInHandHasable", [cards, kinds]) => gain_abilities(
            cards_of(
                EventZone::Hand,
                &PlayerRef::AbilityController,
                &hand_filter(cards)?,
            ),
            kinds,
        ),
        (
            "AddAbilityFromAnExiledHasable_MayOnlyActivateOnecEachTurn",
            [Operand::Objects(objects), kinds],
        ) => Change::GainAbilitiesOf {
            objects: objects.clone(),
            kinds: Box::new(kinds.clone()),
            once_each_turn: true,
            spend_as: None,
        },
        ("AddActivatedAbilitiesAndMaySpendManaAsThoughItWasAnyColorToActivate", [abilities]) => {
            return activated_with_spending(abilities, SpendAs::AnyColor);
        }
        (
            "AddActivatedAbilitiesAndMaySpendColorManaAsThoughAnyColorToActivate",
            [abilities, color],
        ) => {
            return activated_with_spending(
                abilities,
                SpendAs::ColorAsAnyColor(color_form(color)?),
            );
        }
        ("IsACopyOfLibraryCard", [Operand::Objects(objects), effects]) => copy_of(
            ObjectRef::Query {
                objects: Box::new(objects.clone()),
                index: 0,
            },
            effects,
        )?,
        ("IsACopyOfPermanentSpell", [Operand::Object(object), effects]) => {
            copy_of(object.clone(), effects)?
        }
        ("IsACopyOfTheRevealedCard", [effects]) => {
            copy_of(earlier_object(EarlierObjects::Revealed), effects)?
        }
        ("IsACopyOf_TheObjectChosenToCopy", [effects]) => {
            copy_of(earlier_object(EarlierObjects::Selected), effects)?
        }
        ("HasTextOfGraveyardCardAndTheText", [Operand::Object(object), abilities]) => {
            Change::TextOf {
                object: object.clone(),
                plus: Box::new(abilities.clone()),
            }
        }
        ("IncreaseManaCost", [mana]) => Change::SpellCost(SpellCostModifier::Increase(mana_cost(
            std::slice::from_ref(mana),
        )?)),
        ("IncreaseManaCostForEach", [mana, Operand::Number(each)]) => {
            let symbols = mana_terms(std::slice::from_ref(mana))?;
            Change::SpellCost(SpellCostModifier::Increase(Cost {
                parts: vec![CostPart::Repeated {
                    part: Box::new(CostPart::Mana(symbols)),
                    times: each.clone(),
                }],
            }))
        }
        ("DecreaseManaCost", [symbols]) => Change::SpellCost(reduced_cost(symbols, &one)?),
        ("DecreaseManaCostForEach", [symbols, Operand::Number(each)]) => {
            Change::SpellCost(reduced_cost(symbols, each)?)
        }
        _ => return None,
    })
}

pub(super) fn draft_note(kind: DraftNoteKind, player: &PlayerRef, card: &str) -> DraftNote {
    DraftNote {
        kind,
        player: player.clone(),
        card: card.to_string(),
    }
}

/// What a card must be to count as this hand or card term.
pub(super) fn card_term(operand: &Operand) -> Option<ObjectFilter> {
    match operand {
        Operand::Filter(filter) => Some(filter.clone()),
        Operand::Vocabulary { form, .. } if form == "ThisCardInHand" || form == "ThisCard" => {
            Some(ObjectFilter::SameObject(ObjectRef::Source))
        }
        Operand::Term { form, operands, .. } if form == "SingleCardInHand" => {
            card_term(operands.first()?)
        }
        Operand::Term { form, operands, .. } if form == "IsCreatureType" => {
            Some(ObjectFilter::Subtype(vocabulary_value(operands.first()?)?))
        }
        Operand::Term { form, operands, .. } if form == "And" || form == "Or" => {
            let filters = operands.iter().map(card_term).collect::<Option<Vec<_>>>()?;
            Some(if form == "And" {
                ObjectFilter::AllOf(filters)
            } else {
                ObjectFilter::AnyOf(filters)
            })
        }
        Operand::Term { form, operands, .. } if form == "Other" => {
            Some(ObjectFilter::Not(Box::new(card_term(operands.first()?)?)))
        }
        Operand::Term { form, operands, .. } if form == "IsCardtype" => {
            Some(ObjectFilter::CardType(vocabulary_value(operands.first()?)?))
        }
        Operand::Term { form, operands, .. } if form == "IsLandType" => {
            Some(ObjectFilter::Subtype(crate::subtype::land_type(
                &vocabulary_value(operands.first()?)?,
            )))
        }
        Operand::Term { form, operands, .. } if form == "IsNamed" => match operands.first()? {
            Operand::Term { operands, .. } => match operands.as_slice() {
                [Operand::Text(name)] => Some(ObjectFilter::Name(name.clone())),
                _ => None,
            },
            _ => None,
        },
        other => hand_filter(other),
    }
}

pub(super) fn perpetual_changes(effects: &[Operand]) -> Option<Vec<Change>> {
    let mut changes = Vec::new();
    for effect in effects {
        let Operand::Term { form, operands, .. } = effect else {
            return None;
        };
        let reduced = match (form.as_str(), operands.as_slice()) {
            ("AdjustPT", [Operand::Integer(power), Operand::Integer(toughness)]) => {
                vec![Change::Pump {
                    power: Expr::Constant(*power),
                    toughness: Expr::Constant(*toughness),
                }]
            }
            ("AddAbility", [Operand::Ability(ability)]) => {
                vec![Change::GrantAbility(ability.clone())]
            }
            ("AdjustPTX", [power, toughness, Operand::Number(x)]) => vec![Change::Pump {
                power: mod_x(power, x)?,
                toughness: mod_x(toughness, x)?,
            }],
            ("SetPT", [pt]) => vec![set_pt(pt)?],
            _ => reduce_changes(format!("LayerEffect::{form}"), operands.clone()),
        };
        if reduced
            .iter()
            .any(|change| matches!(change, Change::Primitive { .. }))
        {
            return None;
        }
        changes.extend(reduced);
    }
    Some(changes)
}

pub(super) fn vocabulary_form(operand: &Operand) -> Option<&str> {
    match operand {
        Operand::Vocabulary { form, .. } => Some(form),
        _ => None,
    }
}

pub(super) fn color_form(operand: &Operand) -> Option<ManaColor> {
    match vocabulary_form(operand)? {
        "White" => Some(ManaColor::White),
        "Blue" => Some(ManaColor::Blue),
        "Black" => Some(ManaColor::Black),
        "Red" => Some(ManaColor::Red),
        "Green" => Some(ManaColor::Green),
        _ => None,
    }
}

/// A set of permanents as a predicate; a chosen set stays that set.
pub(super) fn objects_filter(objects: &ObjectSet) -> ObjectFilter {
    event_filter(objects).unwrap_or_else(|| ObjectFilter::InSet(Box::new(objects.clone())))
}
