//! A spell that checks its own kicker is one cost branch, a postposed condition's
//! "its" names the object its consequent acts on, and a combat condition about a
//! player leads its sentence.
use super::*;

const REMINDER: &str = "(You may pay an additional {2} as you cast this spell.)";

fn branch(reading: &Value) -> &Value {
    let ability = &reading[0]["args"]["ability"];
    assert_eq!(ability["kind"], "CostBranch", "{reading:#}");
    &ability["args"]
}

fn conditions(program: &Value) -> Vec<Value> {
    let mut found = Vec::new();
    visit(program, "ObjectMatches", &mut found);
    found
        .into_iter()
        .map(|test| test["object"].clone())
        .collect()
}

#[test]
fn a_spell_asking_whether_it_was_kicked_is_one_cost_branch() {
    let reading = compositional(&format!(
        "Kicker {{2}} {REMINDER}\nReturn target nonland permanent to its owner's hand. If this \
         spell was kicked, draw a card."
    ));
    let branch = branch(&reading);
    assert_eq!(branch["keyword"], "Kicker");
    assert!(conditions(&branch["paid"]).is_empty(), "{branch:#}");
    printed_types(&reading, &["Instant"]);
}

#[test]
fn its_after_that_spell_names_that_spell() {
    let reading = compositional(&format!(
        "Kicker {{2}} {REMINDER}\nCounter target spell if its mana value is 2 or less. If this \
         spell was kicked, counter that spell if its mana value is 4 or less instead."
    ));
    let target = json!({"kind":"Target","args":{"slot":0,"index":0}});
    let branch = branch(&reading);
    assert_eq!(conditions(&branch["paid"]), conditions(&branch["unpaid"]));
    assert_eq!(conditions(&branch["paid"]), [target]);
    printed_types(&reading, &["Instant"]);
}

#[test]
fn a_player_casting_a_spell_is_any_player() {
    let reading = compositional(
        "When a player casts a spell, sacrifice this enchantment and counter that spell.",
    );
    let event = &reading[0]["args"]["event"]["Spell"];
    assert_eq!(event["players"], json!({"kind":"All"}));
    printed_types(&reading, &["Enchantment"]);
}

#[test]
fn a_combat_condition_about_a_player_leads_the_sentence() {
    let rule = "a combat condition about a player leads";
    for (text, broken) in [
        (
            "As long as this creature is attacking, defending player can't cast spells.",
            false,
        ),
        (
            "Defending player can't cast spells as long as it's attacking.",
            true,
        ),
        ("This creature has flying as long as it's attacking.", false),
        (
            "You may cast that card for as long as it remains exiled.",
            false,
        ),
    ] {
        let tokens = engine().tokens_of("Context example", text).unwrap();
        let found = crate::style::breaches(&tokens, &[]);
        assert_eq!(
            found.iter().any(|(name, ..)| *name == rule),
            broken,
            "{text}"
        );
    }
    // The leading sentence still reads through its seeded line, which prints it first.
    let text = "Flying\nAs long as this creature is attacking, defending player can't cast spells.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1);
    assert_eq!(printed(&readings[0]), text);
}
