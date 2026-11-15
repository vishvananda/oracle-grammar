//! A copy effect reads its object's copiable values (CR 707.2). After a creature dies,
//! "that creature" is the creature as it last existed, not the card it left behind.
use super::*;

#[test]
fn a_copy_of_a_dead_creature_uses_its_last_known_values() {
    let text = "Whenever a creature dies, you may pay {1}. If you do, this creature becomes a \
                copy of that creature, except it has this ability.";
    let reading = compositional(text);
    let mut copies = Vec::new();
    visit(&reading, "CopyOf", &mut copies);
    assert_eq!(copies.len(), 1, "{reading:#}");
    assert_eq!(
        copies[0]["object"],
        json!({"kind":"EventSubject"}),
        "{reading:#}"
    );
    printed(&reading);
}
