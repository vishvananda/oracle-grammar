use crate::{ir::*, migrate, validate};

fn event() -> EventSpec {
    EventSpec::Player {
        event: PlayerEvent::PutsOntoBattlefield,
        players: PlayerSet::One(PlayerRef::AbilityController),
        objects: Some(ObjectFilter::Any),
    }
}

fn history(window: HistoryWindow) -> Operand {
    Operand::Number(Expr::EventHistory {
        event: Box::new(event()),
        window,
        measure: HistoryMeasure::Occurrences,
    })
}

#[test]
fn named_last_turn_keeps_the_actor_and_survives_both_codecs() {
    let value = history(HistoryWindow::LastTurnOf(PlayerRef::AbilityController));
    validate::operand(&value).unwrap();
    let json = serde_json::to_value(&value).unwrap();
    let decoded: Operand = serde_json::from_value(json.clone()).unwrap();
    validate::operand(&decoded).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), json);
    let bytes = bincode::encode_to_vec(&value, bincode::config::standard()).unwrap();
    let (decoded, used): (Operand, usize) =
        bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    assert_eq!(used, bytes.len());
    assert_eq!(serde_json::to_value(decoded).unwrap(), json);
    assert_ne!(
        json,
        serde_json::to_value(history(HistoryWindow::LastTurn)).unwrap()
    );
}

#[test]
fn history_windows_validate_their_own_player_references() {
    for window in [
        HistoryWindow::LastTurnOf(PlayerRef::Bound(LoopBinding(999))),
        HistoryWindow::SinceLastTurnOf(PlayerRef::Bound(LoopBinding(999))),
    ] {
        assert!(validate::operand(&history(window.clone())).is_err());
        let player = Operand::Player(PlayerRef::AsOf {
            moment: Box::new(Moment::LastEvent {
                event: Box::new(event()),
                window,
            }),
            player: Box::new(PlayerRef::AbilityController),
        });
        assert!(validate::operand(&player).is_err());
    }
}

#[test]
fn the_new_window_preserves_existing_wire_tags_and_storage_bound() {
    let windows = [
        HistoryWindow::ThisTurn,
        HistoryWindow::LastTurn,
        HistoryWindow::ThisCombat,
        HistoryWindow::ThisGame,
        HistoryWindow::SinceLastTurnOf(PlayerRef::AbilityController),
        HistoryWindow::LastTurnOf(PlayerRef::AbilityController),
    ];
    for (tag, window) in windows.into_iter().enumerate() {
        let bytes = bincode::encode_to_vec(&window, bincode::config::standard()).unwrap();
        assert_eq!(usize::from(bytes[0]), tag);
    }
    // The appended variant carries the same payload as SinceLastTurnOf.
    assert!(
        std::mem::size_of::<HistoryWindow>()
            <= std::mem::size_of::<PlayerRef>() + std::mem::align_of::<PlayerRef>()
    );
}

#[test]
fn schema_190_migrates_identically_but_cannot_smuggle_in_the_new_variant() {
    let packet: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/extraction-190.json")).unwrap();
    let source = &packet["records"][0]["card"];
    let mapped = migrate::mapped_read_json(&source.to_string()).unwrap();
    assert_eq!(mapped.source_version, 190);
    assert!(mapped.map.introduced.is_empty());
    for (to, from) in &mapped.map.sites {
        assert_eq!(from, &vec![to.clone()]);
    }
    let mut card = mapped.card;
    assert_eq!(card.faces[0].name, "Agent of Raffine");
    let Ability::Activated { limits, .. } = &mut card.faces[0].abilities[0] else {
        panic!("the frozen control must contain its activated ability")
    };
    let Operand::Number(left) = history(HistoryWindow::LastTurnOf(PlayerRef::AbilityController))
    else {
        unreachable!()
    };
    limits.condition = Some(Condition::Compare {
        left,
        test: NumberTest::Compare {
            comparison: Comparison::GreaterOrEqual,
            value: Expr::Constant(1),
        },
    });
    validate::unified(&card).unwrap();
    card.schema_version = 190;
    for face in &mut card.faces {
        face.schema_version = 190;
    }
    let error = migrate::read_json(&serde_json::to_string(&card).unwrap()).unwrap_err();
    assert!(error.contains("LastTurnOf"), "{error}");
}
