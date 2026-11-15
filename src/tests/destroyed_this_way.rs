//! "A creature destroyed this way can't be regenerated" forbids regenerating what
//! the sentence before it destroys; its noun and number agree with what is destroyed.
use super::*;

fn destroys(reading: &Value) -> Vec<Value> {
    let mut found = Vec::new();
    visit(reading, "Destroy", &mut found);
    found
}

#[test]
fn a_creature_destroyed_this_way_cant_be_regenerated() {
    for text in [
        "Destroy target creature. A creature destroyed this way can't be regenerated.",
        "Destroy target creature if it's white. A creature destroyed this way can't be \
         regenerated.",
        "Destroy all creatures. Creatures destroyed this way can't be regenerated.",
        // A destroyed reference names its noun only through what it refers to.
        "{3}: Target creature gains islandwalk until end of turn. When this artifact leaves \
         the battlefield this turn, destroy that creature. A creature destroyed this way \
         can't be regenerated.",
    ] {
        let reading = compositional(text);
        let found = destroys(&reading);
        assert!(!found.is_empty(), "{text}: {reading:#}");
        assert!(
            found.iter().all(|d| d["regeneration"] == false),
            "{text}: {reading:#}"
        );
        // "It can't be regenerated." is an attested spelling of the same reading.
        printed_types(&reading, &["Sorcery"]);
        assert!(
            engine()
                .renders_to("Context example", &reading, text)
                .unwrap(),
            "{text} must stay reachable by the printer"
        );
    }
}

#[test]
fn destroyed_this_way_agrees_with_what_was_destroyed() {
    for text in [
        "Destroy target artifact. A creature destroyed this way can't be regenerated.",
        "Destroy all creatures. A creature destroyed this way can't be regenerated.",
        "Destroy target creature. Creatures destroyed this way can't be regenerated.",
        "Draw a card. A creature destroyed this way can't be regenerated.",
    ] {
        let parsed = engine().parse("Context example", text);
        assert!(
            parsed.as_ref().is_err() || parsed.as_ref().unwrap().readings.is_empty(),
            "{text}: {parsed:?}"
        );
    }
}
