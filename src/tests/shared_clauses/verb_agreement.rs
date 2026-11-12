//! A printed verb agrees with its subject's head noun, and an article with the next sound.
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

#[test]
fn where_x_follows_the_x_its_sentence_uses() {
    let defines = |text: &str| {
        let tokens: Vec<Token> = text
            .split(' ')
            .map(|word| match word {
                "." | "," | "/" => Token::Mark(word.chars().next().unwrap()),
                _ => Token::Word(word.into()),
            })
            .collect();
        !breaches(&tokens, &[])
            .iter()
            .any(|(rule, _, _)| *rule == "where X follows its use")
    };
    assert!(defines(
        "create an X / X token , where X is your life total"
    ));
    // An X in an earlier sentence does not count.
    assert!(!defines(
        "draw X cards . you gain 2 life , where X is your life total"
    ));
}

#[test]
fn articles_agree_with_the_next_sound() {
    let agreed = |text: &str| {
        crate::tokens::agree_articles(&words(text))
            .iter()
            .map(|token| match token {
                Token::Word(word) => word.as_str(),
                _ => "?",
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(agreed("an creature card"), "a creature card");
    assert_eq!(agreed("a artifact card"), "an artifact card");
    assert_eq!(agreed("a X"), "an X");
    assert_eq!(agreed("a hour"), "an hour");
    assert_eq!(agreed("an one-time boon"), "a one-time boon");
    assert_eq!(agreed("an unity counter"), "a unity counter");
}
