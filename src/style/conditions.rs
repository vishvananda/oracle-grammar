//! A combat condition trails only a sentence about the object it names: "This creature
//! has flying as long as it's attacking", in 19 of 19 trailing `as long as … attacking`
//! or `… blocking` conditions. About a player it leads: "As long as this creature is
//! attacking, defending player can't cast spells".
use super::{BROKEN, Found, is};
use crate::tokens::Token;

const PLAYERS: [&[&str]; 7] = [
    &["defending", "player"],
    &["each", "player"],
    &["each", "opponent"],
    &["target", "player"],
    &["your", "opponents"],
    &["players"],
    &["you"],
];

pub(super) fn combat_condition_leads(tokens: &[Token], found: &mut Found) {
    for at in 1..tokens.len() {
        if !(is(tokens, at, "as") && is(tokens, at + 1, "long") && is(tokens, at + 2, "as")) {
            continue;
        }
        let combat = (at + 3..(at + 8).min(tokens.len()))
            .any(|i| is(tokens, i, "attacking") || is(tokens, i, "blocking"));
        let start = (0..at)
            .rev()
            .find(|&i| matches!(tokens[i], Token::Newline | Token::Mark('.' | '"' | ',')))
            .map_or(0, |i| i + 1);
        let player = PLAYERS.iter().any(|words| {
            words
                .iter()
                .enumerate()
                .all(|(offset, word)| is(tokens, start + offset, word))
        });
        if combat && start < at && player {
            found.push(("a combat condition about a player leads", BROKEN, at));
        }
    }
}
