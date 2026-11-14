use super::*;

#[test]
fn a_reachable_text_has_no_frontier_and_a_longer_one_names_where_it_stops() {
    let text = "Draw a card.";
    let reading = engine().parse("Example", text).unwrap().readings.remove(0);
    assert!(
        engine()
            .print_frontier("Example", &reading, text)
            .unwrap()
            .is_none()
    );
    let frontier = engine()
        .print_frontier("Example", &reading, "Draw a card and gain 3 life.")
        .unwrap()
        .expect("an unreachable text has a frontier");
    // Every derivation shares "Draw a card"; none reaches the end of the text.
    let at = frontier["at"].as_u64().unwrap();
    assert!((3..8).contains(&at), "{frontier}");
    assert!(
        frontier["before"]
            .as_str()
            .unwrap()
            .starts_with("Draw a card")
    );
    assert!(!frontier["blamed"].as_array().unwrap().is_empty());
}
