use super::*;

/// Lines that lower to separate statics for one player print as written,
/// though reduction holds them as one static with every change.
#[test]
fn statics_lowered_apart_print_where_they_were_written() {
    let lines = [
        "You may look at the top card of your library any time.",
        "You may play lands and cast spells from the top of your library.",
        "Once during each of your turns, you may cast a spell from your hand or the top of your library without paying its mana cost.",
    ];
    let mut lowered = Vec::new();
    for line in lines {
        let mut parsed = engine().parse("Example", line).unwrap().readings;
        assert_eq!(parsed.len(), 1, "{line}");
        lowered.extend(parsed.remove(0).as_array().unwrap().iter().cloned());
    }
    let abilities = Value::Array(lowered);
    let text = lines.join("\n");
    let key = engine()
        .derivation_of("Example", &abilities, &text)
        .unwrap()
        .expect("the written lines are reachable");
    assert_eq!(
        engine()
            .print_keyed("Example", &abilities, &[], &key)
            .unwrap()
            .as_deref(),
        Some(text.as_str())
    );
}
