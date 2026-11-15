//! A trigger that returns this card from your graveyard works from the graveyard,
//! unless its event is this object or its host leaving the battlefield: those look
//! back and work on the battlefield.
use super::*;

/// The zones each scoped ability of `reading` works in.
fn scoped_zones(reading: &Value) -> Vec<Value> {
    let mut found = Vec::new();
    visit(reading, "Scoped", &mut found);
    found
        .into_iter()
        .map(|args| args["zones"].clone())
        .collect()
}

#[test]
fn a_return_from_the_graveyard_works_there() {
    for text in [
        "Whenever a Demon you control enters, return this card from your graveyard to your \
         hand.",
        "Whenever you cast a multicolored spell, you may return this card from your \
         graveyard to the battlefield.",
        "Whenever a creature you control dies, you may pay {1}{B}. If you do, return this \
         card from your graveyard to the battlefield.",
    ] {
        let reading = compositional(text);
        assert_eq!(scoped_zones(&reading), [json!(["Graveyard"])], "{text}");
        printed(&reading);
    }
}

#[test]
fn a_departure_trigger_never_works_from_the_graveyard() {
    for text in [
        "When this creature dies, return this card from your graveyard to the battlefield.",
        "Enchant creature\nWhen enchanted creature dies, return this card from your \
         graveyard to the battlefield.",
    ] {
        let readings = engine()
            .parse("Context example", text)
            .map(|parsed| parsed.readings)
            .unwrap_or_default();
        assert!(
            readings
                .iter()
                .all(|reading| !scoped_zones(reading).contains(&json!(["Graveyard"]))),
            "{text}: {readings:#?}"
        );
    }
}
