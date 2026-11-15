//! "Target player exiles a card from their hand": that player chooses the card and exiles
//! it, and the exiled card is what "it" and "that card" name in the next sentence.
use super::*;

fn moved_result(reading: &Value) -> Value {
    let mut effects = Vec::new();
    visit(reading, "Effect", &mut effects);
    let moves: Vec<_> = effects
        .iter()
        .filter(|effect| effect["operation"]["kind"] == "Move")
        .collect();
    assert_eq!(moves.len(), 1, "{reading:#}");
    json!({"kind":"Result","args":{"result":moves[0]["result"],"phase":"After","index":0}})
}

#[test]
fn a_player_exiles_a_card_of_their_own_choice() {
    for (text, types) in [
        ("Target opponent exiles a card from their hand.", "Sorcery"),
        ("Each opponent exiles a card from their hand.", "Sorcery"),
        (
            "{T}: Target player exiles a card from their graveyard.",
            "Artifact",
        ),
    ] {
        let reading = compositional(text);
        let mut choices = Vec::new();
        visit(&reading, "ChooseObjects", &mut choices);
        assert_eq!(choices.len(), 1, "{text}: {reading:#}");
        assert_ne!(choices[0]["player"]["kind"], "AbilityController", "{text}");
        printed_types(&reading, &[types]);
        assert!(
            engine()
                .renders_to("Context example", &reading, text)
                .unwrap(),
            "{text} must stay reachable by the printer"
        );
    }
}

#[test]
fn the_exiled_card_is_what_the_next_sentence_names() {
    let text = "{2}, {T}: Target player exiles a card from their graveyard. If it's a creature \
                card, you gain 2 life.";
    let reading = compositional(text);
    let mut tests = Vec::new();
    visit(&reading, "ObjectMatches", &mut tests);
    assert_eq!(tests.len(), 1, "{reading:#}");
    assert_eq!(tests[0]["object"], moved_result(&reading), "{reading:#}");

    let text = "Whenever this creature deals combat damage to a player, that player exiles a \
                card from their hand. You may play that card for as long as it remains exiled.";
    let reading = compositional(text);
    let mut permissions = Vec::new();
    visit(&reading, "One", &mut permissions);
    assert!(
        permissions.contains(&moved_result(&reading)),
        "{text}: {reading:#}"
    );
    printed_types(&reading, &["Creature"]);
}
