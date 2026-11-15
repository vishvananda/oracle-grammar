//! "Can't be the target of …": a spell is a stack object of kind Spell, "abilities"
//! are the other stack objects, and an ability "from" a source is an ability of it.
use super::*;

/// The filter of every targeting ban in `reading`, in order.
fn bans(reading: &Value) -> Vec<Value> {
    fn walk(value: &Value, found: &mut Vec<Value>) {
        match value {
            Value::Object(fields) => {
                if let Some(filter) = fields.get("BeTargeted") {
                    found.push(filter.clone());
                }
                fields.values().for_each(|v| walk(v, found));
            }
            Value::Array(items) => items.iter().for_each(|v| walk(v, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(reading, &mut found);
    found
}

const SPELL: &str = r#"{"kind":"OnStack","args":"Spell"}"#;

fn spell() -> Value {
    serde_json::from_str(SPELL).unwrap()
}

fn opponents() -> Value {
    json!({"kind":"ControlledBy","args":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}}})
}

#[test]
fn a_spell_ban_names_spells_and_an_ability_ban_names_their_sources() {
    let black = json!({"kind":"Color","args":"Black"});
    // Canonical filters keep their members sorted, whatever the written order.
    let reading = compositional(
        "This creature can't be the target of black spells or abilities from black sources.",
    );
    assert_eq!(
        bans(&reading),
        [
            json!({"kind":"AllOf","args":[black, spell()]}),
            json!({"kind":"AllOf","args":[{"kind":"AbilityOf","args":black},
                {"kind":"Not","args":spell()}]}),
        ]
    );
    printed(&reading);
}

#[test]
fn opposing_spells_or_abilities_name_no_stack_kind() {
    let reading = compositional(
        "Enchant creature\nEnchanted creature can't be the target of spells or abilities your \
         opponents control.",
    );
    assert_eq!(bans(&reading), [opponents()]);
    printed_types(&reading, &["Enchantment"]);
    let reading = compositional(
        "This creature can't be the target of blue or black spells your opponents control.",
    );
    let [ban] = bans(&reading).try_into().unwrap();
    assert!(ban["args"].as_array().unwrap().contains(&spell()), "{ban}");
    assert!(ban.to_string().contains(&opponents().to_string()), "{ban}");
    printed(&reading);
}

#[test]
fn a_plural_subject_is_the_targets_of_anything() {
    let reading = compositional("Cards in graveyards can't be the targets of spells or abilities.");
    assert_eq!(bans(&reading), [json!({"kind":"Any"})]);
    assert_eq!(
        printed_types(&reading, &["Enchantment"]),
        "Cards in graveyards can't be the targets of spells or abilities."
    );
}

#[test]
fn a_prohibition_ends_this_turn_after_its_verb() {
    let reading = compositional(
        "Target creature can't be the target of spells or abilities your opponents control \
         this turn.",
    );
    assert_eq!(bans(&reading), [opponents()]);
    assert_eq!(
        printed_types(&reading, &["Instant"]),
        "Target creature can't be the target of spells or abilities your opponents control \
         this turn."
    );
    let after_the_verb = engine()
        .parse(
            "Context example",
            "Target creature can't block until end of turn.",
        )
        .map(|parsed| parsed.readings)
        .unwrap_or_default();
    assert!(after_the_verb.is_empty(), "{after_the_verb:#?}");
}
