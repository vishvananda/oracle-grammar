//! A printed verb agrees with its subject's head noun.
use crate::style::breaches;
use crate::tokens::Token;

fn words(text: &str) -> Vec<Token> {
    text.split(' ')
        .map(|word| Token::Word(word.into()))
        .collect()
}

fn agrees(text: &str) -> bool {
    !breaches(&words(text), &[])
        .iter()
        .any(|(rule, _, _)| *rule == "verb agrees")
}

#[test]
fn becoming_agrees_with_the_head_of_its_subject() {
    assert!(agrees(
        "up to one other target Vehicle you control becomes an artifact creature"
    ));
    assert!(!agrees(
        "up to one other target Vehicle you control become artifact creatures"
    ));
    assert!(agrees("creatures you control become artifact creatures"));
    assert!(!agrees(
        "creatures you control becomes an artifact creature"
    ));
    // An infinitive or a subject `you` carries no agreement.
    assert!(agrees("target creature can't become tapped"));
    assert!(agrees("whenever you become the monarch"));
}
