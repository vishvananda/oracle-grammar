//! A number reads the same with or without thousands separators, and its print keeps
//! the separators the text was written with.
use super::*;

#[test]
fn numbers_print_as_written() {
    for text in [
        "Each opponent loses 1,000 life.",
        "Each opponent loses 9999 life.",
    ] {
        let readings = engine().parse("Context example", text).unwrap().readings;
        assert_eq!(readings.len(), 1, "{text}");
        let trail = engine()
            .trail_of("Context example", &readings[0], text)
            .unwrap()
            .unwrap();
        assert_eq!(
            engine()
                .print_trail("Context example", &readings[0], &trail)
                .unwrap(),
            Some(text.to_string())
        );
    }
}
