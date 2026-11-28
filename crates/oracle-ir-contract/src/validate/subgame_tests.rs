use super::*;

#[test]
fn subgame_conditions_require_the_subgame_receipt() {
    let result = ResultId(0);
    let played = Condition::PlayedSubgame(result.clone());
    let won = Condition::WonSubgame {
        player: PlayerRef::AbilityController,
        result: result.clone(),
    };
    let mut scope = Scope::default();
    assert!(scope.condition(&played).is_err());
    assert!(scope.condition(&won).is_err());

    scope
        .program(&Program {
            instructions: vec![Instruction::Effect {
                result,
                operation: Operation::StartSubgame {
                    agreement: Some(PlayerSet::All),
                    retain_in_main: vec![ObjectRef::Source],
                },
            }],
        })
        .unwrap();
    assert!(scope.condition(&played).is_ok());
    assert!(scope.condition(&won).is_ok());
}
