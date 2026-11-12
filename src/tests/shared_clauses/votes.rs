//! Council's dilemma: each player votes in turn for one of two words, and later
//! sentences count a word's votes or ask which word won.
use super::*;

#[test]
fn a_word_vote_is_counted_and_won_by_its_words() {
    let text = "Starting with you, each player votes for death or torture. If death gets more \
                votes, each opponent loses 2 life. If torture gets more votes or the vote is \
                tied, draw a card for each torture vote.";
    let reading = compositional(text);
    let body = &reading[0]["args"]["body"]["instructions"];
    assert_eq!(
        body[0]["args"]["starting_with"],
        json!({"kind":"AbilityController"})
    );
    let mut votes = Vec::new();
    visit(&reading, "Vote", &mut votes);
    assert_eq!(votes[0]["ballot"], json!({"Words":["Death","Torture"]}));
    let mut won = Vec::new();
    visit(&reading, "MostVotes", &mut won);
    assert_eq!(
        won,
        [
            json!({"option":{"Word":"Death"},"tied":false}),
            json!({"option":{"Word":"Torture"},"tied":true})
        ]
    );
    let mut counted = Vec::new();
    visit(&reading, "Votes", &mut counted);
    assert_eq!(counted, [json!({"Word":"Torture"})]);
    assert_eq!(printed_types(&reading, &["Sorcery"]), text);
}
