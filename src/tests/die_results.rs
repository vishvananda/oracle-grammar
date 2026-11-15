//! "The result" and "that much" after a die roll name that roll's result wherever
//! the mention sits: in the test, in its consequence or in a "when you do" trigger.
use super::*;

/// The result address of the one die roll in `reading`.
fn roll(reading: &Value) -> Value {
    let mut effects = Vec::new();
    visit(reading, "Effect", &mut effects);
    let rolls: Vec<_> = effects
        .iter()
        .filter(|effect| {
            effect["operation"]["args"]["action"]
                .get("RollDie")
                .is_some()
        })
        .collect();
    assert_eq!(rolls.len(), 1, "{reading:#}");
    rolls[0]["result"].clone()
}

#[test]
fn the_result_names_the_roll_however_far_back_it_is() {
    for (text, types, uses) in [
        (
            "When this creature enters, roll a six-sided die. If the result is 3 or less, you \
             lose that much life.",
            "Creature",
            2,
        ),
        (
            "Return target creature to its owner's hand, then roll a six-sided die. If the \
             result is 3 or less, scry a number of cards equal to the result.",
            "Instant",
            2,
        ),
        (
            "Whenever this creature deals combat damage to a player, roll a d20. When you do, \
             you gain life equal to the result.",
            "Creature",
            1,
        ),
    ] {
        let reading = compositional(text);
        let mut results = Vec::new();
        visit(&reading, "DieResult", &mut results);
        assert_eq!(results.len(), uses, "{text}: {reading:#}");
        let roll = roll(&reading);
        assert!(
            results.iter().all(|result| *result == roll),
            "{text}: {reading:#}"
        );
        printed_types(&reading, &[types]);
    }
}

#[test]
fn the_result_needs_a_roll() {
    let text = "Draw cards equal to the result.";
    let audited = engine()
        .audited("Context example", text, &crate::parse::RuleSet::default())
        .unwrap_or_default();
    assert!(
        !audited
            .iter()
            .any(|(_, rules)| rules.iter().all(Vec::is_empty)),
        "{text}: {audited:?}"
    );
}
