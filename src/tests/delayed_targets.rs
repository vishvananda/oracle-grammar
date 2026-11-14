//! A target chosen in its own sentence before a delayed trigger leaves the trigger's
//! own target to introduce in the trigger's text.
use super::*;

#[test]
fn delayed_trigger_target_after_chosen_target() {
    for text in [
        "Choose target creature. When that creature dies this turn, destroy target artifact.",
        "Choose target creature. When that creature dies this turn, earthbend 4.",
    ] {
        let readings = engine().parse("Context example", text).unwrap().readings;
        assert_eq!(readings.len(), 1, "{text}");
        let trail = engine()
            .trail_of("Context example", &readings[0], text)
            .unwrap()
            .expect("the text is reachable");
        assert_eq!(
            engine()
                .print_trail("Context example", &readings[0], &trail)
                .unwrap(),
            Some(text.to_string())
        );
    }
}
