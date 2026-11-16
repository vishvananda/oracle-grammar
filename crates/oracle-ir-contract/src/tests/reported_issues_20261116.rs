//! Portable contracts for the issue cohort; these do not simulate a game engine.
use crate::{intrinsic, ir::*, migrate, mtg_types::*, validate};
use serde_json::json;

fn options() -> Vec<ManaReplacementOption> {
    vec![
        ManaReplacementOption {
            sources: ObjectFilter::Subtype(SubType::Plains),
            color: ManaColor::Red,
        },
        ManaReplacementOption {
            sources: ObjectFilter::Subtype(SubType::Island),
            color: ManaColor::Green,
        },
    ]
}

fn replacement(event: ReplacementEvent, outputs: Vec<ManaReplacementOption>) -> Ability {
    Ability::Replacement {
        active: ReplacementActive::Battlefield,
        event,
        body: Program {
            instructions: vec![Instruction::Effect {
                result: ResultId(0),
                operation: Operation::ModifyEvent {
                    change: EventChange::ReplaceManaChoices(outputs),
                },
            }],
        },
    }
}

fn mana_event() -> ReplacementEvent {
    ReplacementEvent::ProduceMana {
        sources: ObjectFilter::Any,
        by: None,
    }
}

#[test]
fn grouped_mana_is_one_replacement_with_explicit_applicable_outputs() {
    let ability = replacement(mana_event(), options());
    validate::validate_ability(&ability).unwrap();
    let wire = serde_json::to_value(&ability).unwrap();
    assert_eq!(
        wire["args"]["body"]["instructions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let outputs = &wire["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["change"]["ReplaceManaChoices"];
    assert_eq!(outputs.as_array().unwrap().len(), 2);
    assert_eq!(outputs[0]["color"], "Red");
    assert_eq!(outputs[1]["color"], "Green");
    let bytes = bincode::encode_to_vec(&ability, bincode::config::standard()).unwrap();
    let (decoded, used): (Ability, usize) =
        bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    assert_eq!(used, bytes.len());
    validate::validate_ability(&decoded).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
}

#[test]
fn grouped_mana_rejects_unrelated_events_empty_choices_and_unknown_colors() {
    let unrelated = ReplacementEvent::Player {
        event: ReplacedPlayerEvent::Draw,
        players: PlayerSet::All,
    };
    assert!(validate::validate_ability(&replacement(unrelated, options())).is_err());
    assert!(validate::validate_ability(&replacement(mana_event(), vec![])).is_err());
    let mut outputs = options();
    outputs[0].color = ManaColor::Chosen;
    assert!(validate::validate_ability(&replacement(mana_event(), outputs)).is_err());
    let mixed = ReplacementEvent::Either(vec![
        mana_event(),
        ReplacementEvent::Enter {
            objects: ObjectFilter::Any,
        },
    ]);
    assert!(validate::validate_ability(&replacement(mixed, options())).is_err());
    assert!(
        validate::validate_ability(&replacement(mana_event(), vec![options()[0].clone(); 65]))
            .is_err()
    );
    let mut extra_action = replacement(mana_event(), options());
    if let Ability::Replacement { body, .. } = &mut extra_action {
        body.instructions.push(body.instructions[0].clone());
    }
    assert!(validate::validate_ability(&extra_action).is_err());
}

fn death_history(to: EventZone) -> ObjectSet {
    ObjectSet::FromHistoryDestinations {
        event: Box::new(EventSpec::ZoneChange {
            objects: ObjectFilter::AllOf(vec![
                ObjectFilter::CardType(CardType::Creature),
                ObjectFilter::History {
                    event: ObjectHistory::DamagedBy(ObjectRef::Source),
                    window: ObjectWindow::ThisTurn,
                },
            ]),
            from: Some(EventZone::Battlefield),
            to,
        }),
        window: HistoryWindow::ThisTurn,
    }
}

#[test]
fn historical_destinations_retain_the_departing_predicate_and_projection() {
    let objects = death_history(EventZone::Graveyard);
    let operand = Operand::Objects(objects);
    validate::operand(&operand).unwrap();
    let wire = serde_json::to_value(&operand).unwrap();
    let decoded: Operand = serde_json::from_value(wire.clone()).unwrap();
    validate::operand(&decoded).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
    let ObjectSet::FromHistoryDestinations { event, window } = death_history(EventZone::Graveyard)
    else {
        unreachable!()
    };
    let subjects = serde_json::to_value(ObjectSet::FromHistory { event, window }).unwrap();
    assert_ne!(wire["args"], subjects);
    assert!(validate::operand(&Operand::Objects(death_history(EventZone::Hand))).is_err());
    assert!(validate::operand(&Operand::Objects(death_history(EventZone::Library))).is_err());
}

#[test]
fn linked_creation_and_entry_history_keep_their_own_relations() {
    let linked = ObjectFilter::CreatedWith(ObjectRef::Source);
    let broad = ObjectFilter::MadeBy(ObjectRef::Source);
    assert_ne!(
        serde_json::to_value(&linked).unwrap(),
        serde_json::to_value(broad).unwrap()
    );
    let entry = Operand::Number(Expr::CountObjects(Box::new(ObjectSet::Matching {
        objects: Box::new(ObjectSet::SacrificedAsEntered(ObjectRef::Source)),
        filter: ObjectFilter::Subtype(SubType::Forest),
    })));
    validate::operand(&entry).unwrap();
    let mut changed = serde_json::to_value(&entry).unwrap();
    changed["args"]["args"]["args"]["filter"]["args"] = json!("Island");
    let changed: Operand = serde_json::from_value(changed).unwrap();
    validate::operand(&changed).unwrap();
    assert_ne!(
        serde_json::to_value(entry).unwrap(),
        serde_json::to_value(changed).unwrap()
    );
}

fn characteristics() -> Characteristics {
    Characteristics {
        supertypes: vec![],
        card_types: vec![CardType::Land],
        subtypes: vec![SubType::Swamp, SubType::Gate],
        mana_cost: None,
        color_indicator: vec![],
        power_toughness: None,
        loyalty: None,
        defense: None,
    }
}

fn printed_mana(color: ManaColor) -> Ability {
    Ability::Activated {
        origin: ActivationOrigin::Ordinary,
        cost: Cost {
            parts: vec![CostPart::TapSymbol],
        },
        limits: ActivationLimits {
            once: false,
            once_each_turn: false,
            sorcery: false,
            condition: None,
            activators: None,
            keyword: None,
            modifiers: vec![],
        },
        announcement: Announcement {
            targets: vec![],
            modes: None,
        },
        body: Program {
            instructions: vec![Instruction::Effect {
                result: ResultId(0),
                operation: Operation::AddMana {
                    players: PlayerSet::One(PlayerRef::AbilityController),
                    mana: ManaProduction::Amount {
                        color,
                        amount: Expr::Constant(1),
                    },
                    restriction: None,
                },
            }],
        },
    }
}

#[test]
fn intrinsic_mana_uses_types_and_coalesces_only_the_plain_printed_manifestation() {
    let mut c = characteristics();
    assert_eq!(intrinsic::basic_land_mana(&c), [ManaColor::Black]);
    let black = printed_mana(ManaColor::Black);
    assert_eq!(
        intrinsic::printed_basic_land_mana(&c, &black),
        Some(ManaColor::Black)
    );
    assert!(intrinsic::printed_basic_land_mana(&c, &printed_mana(ManaColor::Green)).is_none());
    let mut modified = black.clone();
    if let Ability::Activated { limits, .. } = &mut modified {
        limits.once = true;
    }
    assert!(intrinsic::printed_basic_land_mana(&c, &modified).is_none());
    let face = Card {
        schema_version: SCHEMA_VERSION,
        name: "Typed land".into(),
        characteristics: c.clone(),
        abilities: vec![black, modified],
    };
    assert_eq!(intrinsic::nonintrinsic_printed_abilities(&face).count(), 1);
    c.card_types = vec![CardType::Creature];
    assert!(intrinsic::basic_land_mana(&c).is_empty());
    c.card_types = vec![CardType::Land];
    c.subtypes.clear();
    c.supertypes.push(SuperType::Basic);
    assert!(intrinsic::basic_land_mana(&c).is_empty());
}

#[test]
fn every_basic_land_type_has_one_intrinsic_output_with_or_without_printed_text() {
    for (subtype, color) in [
        (SubType::Plains, ManaColor::White),
        (SubType::Island, ManaColor::Blue),
        (SubType::Swamp, ManaColor::Black),
        (SubType::Mountain, ManaColor::Red),
        (SubType::Forest, ManaColor::Green),
    ] {
        let mut c = characteristics();
        c.subtypes = vec![subtype];
        let printed = Card {
            schema_version: SCHEMA_VERSION,
            name: "Intrinsic control".into(),
            characteristics: c,
            abilities: vec![printed_mana(color.clone())],
        };
        assert_eq!(
            intrinsic::basic_land_mana(&printed.characteristics),
            [color]
        );
        let mut unprinted = printed.clone();
        unprinted.abilities.clear();
        assert_eq!(
            serde_json::to_value(crate::canonical::semantic_card(&printed).unwrap()).unwrap(),
            serde_json::to_value(crate::canonical::semantic_card(&unprinted).unwrap()).unwrap()
        );
        assert_eq!(printed.abilities.len(), 1);
        let mut reminded = printed.clone();
        reminded.abilities[0] = Ability::WithPresentation {
            label: Some("Mana".into()),
            reminder: true,
            ability: Box::new(reminded.abilities[0].clone()),
        };
        assert_eq!(
            intrinsic::nonintrinsic_printed_abilities(&reminded).count(),
            0
        );
        assert_eq!(
            serde_json::to_value(crate::canonical::semantic_card(&reminded).unwrap()).unwrap(),
            serde_json::to_value(crate::canonical::semantic_card(&unprinted).unwrap()).unwrap()
        );
        assert_eq!(
            crate::canonical::card(&reminded).unwrap().abilities.len(),
            1
        );
    }
}

#[test]
fn schema_191_inputs_migrate_with_their_original_binding_sites() {
    let face = Card {
        schema_version: SCHEMA_VERSION,
        name: "Schema control".into(),
        characteristics: characteristics(),
        abilities: vec![printed_mana(ManaColor::Black)],
    };
    let card = UnifiedCard {
        schema_version: SCHEMA_VERSION,
        layout: Layout::Card,
        faces: vec![face],
        data: vec![],
    };
    let mut old = serde_json::to_value(card).unwrap();
    old["schema_version"] = json!(191);
    old["faces"][0]["schema_version"] = json!(191);
    let migrated = migrate::mapped_read_json(&old.to_string()).unwrap();
    assert_eq!(migrated.source_version, 191);
    assert_eq!(migrated.card.schema_version, SCHEMA_VERSION);
    assert_eq!(migrated.card.faces[0].schema_version, SCHEMA_VERSION);
    assert!(!migrated.map.sites.is_empty());
    old["faces"][0]["abilities"][0]["args"]["body"]["instructions"][0]["args"]["operation"] = json!({
        "kind":"BindValue","args":{"value":{"kind":"CountObjects","args":{
            "kind":"SacrificedAsEntered","args":{"kind":"Source"}
        }}}
    });
    assert!(
        migrate::read_json(&old.to_string()).is_err(),
        "schema 191 must reject a smuggled schema-192 relation"
    );
}

#[test]
fn independently_granted_mana_is_not_a_printed_intrinsic_manifestation() {
    let granted = Ability::Static {
        kind: StaticKind::Ordinary,
        affected: EffectSubjects::Objects(ObjectSet::One(ObjectRef::Source)),
        changes: vec![Change::GrantAbility(Box::new(printed_mana(
            ManaColor::Black,
        )))],
    };
    assert!(intrinsic::printed_basic_land_mana(&characteristics(), &granted).is_none());
    let mut c = characteristics();
    c.subtypes = vec![SubType::Island, SubType::Swamp, SubType::Island];
    assert_eq!(
        intrinsic::basic_land_mana(&c),
        [ManaColor::Blue, ManaColor::Black]
    );
}
