use super::*;

fn gain_during_visit(chooser: PlayerRef) -> Instruction {
    Instruction::ForEachPlayerInChosenOrder {
        players: PlayerSet::OpponentsOf(PlayerRef::AbilityController),
        binding: LoopBinding(8),
        chooser,
        body: Program {
            instructions: vec![Instruction::Effect {
                result: ResultId(21),
                operation: Operation::GainLife {
                    players: PlayerSet::One(PlayerRef::Bound(LoopBinding(8))),
                    amount: Expr::Constant(1),
                },
            }],
        },
    }
}

#[test]
fn the_visit_binding_and_receipt_stay_inside_the_visit() {
    let mut scope = Scope::default();
    let mut program = Program {
        instructions: vec![gain_during_visit(PlayerRef::AbilityController)],
    };
    assert!(scope.program(&program).is_ok());
    assert!(scope.player(&PlayerRef::Bound(LoopBinding(8))).is_err());
    assert!(
        scope
            .players(&PlayerSet::GainedLifeBy(ResultId(21)))
            .is_err()
    );
    program.instructions[0] = gain_during_visit(PlayerRef::Bound(LoopBinding(8)));
    assert!(Scope::default().program(&program).is_err());
}
