use super::*;

#[test]
fn gifted_card_is_only_visible_in_its_callback() {
    let mut scope = Scope::default();
    let gifted = ObjectSet::OutsideGiftedCard;
    assert!(scope.objects(&gifted).is_err());

    let program = Program {
        instructions: vec![Instruction::Effect {
            result: ResultId(0),
            operation: Operation::GiftOutsideCard {
                giver: PlayerRef::AbilityController,
                recipients: PlayerSet::OpponentsOf(PlayerRef::AbilityController),
                eligible: ObjectFilter::Any,
                on_gift: Program {
                    instructions: vec![Instruction::Effect {
                        result: ResultId(1),
                        operation: Operation::Reveal {
                            objects: gifted.clone(),
                        },
                    }],
                },
            },
        }],
    };
    assert!(scope.program(&program).is_ok());
    assert!(scope.objects(&gifted).is_err());
}
