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
    // `power and toughness` qualifies the head; a joined subject has no single number.
    assert!(!agrees(
        "other creatures you control with base power and toughness 1/1 gets +1/+1"
    ));
    assert!(agrees(
        "other creatures you control with base power and toughness 1/1 get +1/+1"
    ));
    assert!(agrees("target creature and target land get tapped"));
    assert!(!agrees(
        "creatures you control becomes an artifact creature"
    ));
    // An infinitive or a subject `you` carries no agreement.
    assert!(agrees("target creature can't become tapped"));
    assert!(agrees("whenever you become the monarch"));
    // A causative `have` takes the bare verb, whatever its object's number.
    assert!(agrees(
        "you may have your life total become the total toughness"
    ));
    assert!(!agrees(
        "you may have your life total becomes the total toughness"
    ));
    assert!(agrees(
        "you may have target creature get +2/+2 until end of turn"
    ));
}

#[test]
fn may_takes_a_verb_so_a_subject_after_it_needs_have() {
    let verb = |text: &str| {
        !breaches(&words(text), &[])
            .iter()
            .any(|(rule, _, _)| *rule == "may takes a verb")
    };
    assert!(!verb("you may target opponent loses 1 life"));
    assert!(!verb("you may that player loses 1 life"));
    assert!(!verb("you may this turn"));
    assert!(!verb("you may until end of turn"));
    assert!(!verb("you may have this turn"));
    assert!(verb(
        "you may have target creature get +2/+0 until end of turn"
    ));
    assert!(verb("you may have target opponent lose 1 life"));
    assert!(verb(
        "any number of target players may each search their library"
    ));
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

#[test]
fn a_quoted_sentence_keeps_its_one_period() {
    let periods = |tokens: &[Token]| {
        breaches(tokens, &[])
            .iter()
            .filter(|(rule, _, _)| *rule == "period after a quoted sentence")
            .count()
    };
    let mut tokens = words("cards in your hand perpetually gain");
    tokens.extend([
        Token::Mark('"'),
        Token::Word("flying".into()),
        Token::Mark('.'),
        Token::Mark('"'),
    ]);
    assert_eq!(periods(&tokens), 0);
    tokens.push(Token::Mark('.'));
    assert_eq!(periods(&tokens), 1);
}
