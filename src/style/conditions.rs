//! A combat condition trails only a sentence about the object it names: "This creature
//! has flying as long as it's attacking", in 19 of 19 trailing `as long as … attacking`
//! or `… blocking` conditions. About a player it leads: "As long as this creature is
//! attacking, defending player can't cast spells". A triggered ability asks whether
//! "its additional cost was paid", never "this spell's", and a spell that may behold
//! one creature asks whether that kind "was beheld".
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

/// "When this creature enters, if its additional cost was paid": by the time a
/// permanent's ability triggers it is no longer a spell (Graven Archfiend, 1 of 1).
pub(super) fn trigger_asks_its_cost(tokens: &[Token], found: &mut Found) {
    for at in 0..tokens.len() {
        if !(is(tokens, at, "this")
            && is(tokens, at + 1, "spell's")
            && is(tokens, at + 2, "additional")
            && is(tokens, at + 3, "cost"))
        {
            continue;
        }
        let start = (0..at)
            .rev()
            .find(|&i| matches!(tokens[i], Token::Newline | Token::Mark('.' | '"')))
            .map_or(0, |i| i + 1);
        if ["when", "whenever", "at"]
            .iter()
            .any(|word| is(tokens, start, word))
        {
            found.push((
                "a trigger asks whether this spell's cost was paid",
                BROKEN,
                at,
            ));
        }
    }
}

/// "As an additional cost to cast this spell, you may behold a Dragon. ... If a Dragon
/// was beheld": with one creature to behold, the test names it (6 of 7, Hulk's
/// Thunderclap the exception), not "this spell's additional cost was paid".
pub(super) fn behold_names_its_kind(tokens: &[Token], found: &mut Found) {
    let Some(behold) = (0..tokens.len()).find(|&at| {
        is(tokens, at, "you")
            && is(tokens, at + 1, "may")
            && is(tokens, at + 2, "behold")
            && (is(tokens, at + 3, "a") || is(tokens, at + 3, "an"))
    }) else {
        return;
    };
    for at in behold..tokens.len() {
        if is(tokens, at, "this")
            && is(tokens, at + 1, "spell's")
            && is(tokens, at + 2, "additional")
            && is(tokens, at + 3, "cost")
            && is(tokens, at + 4, "was")
            && is(tokens, at + 5, "paid")
        {
            found.push(("a beheld kind is tested by name", BROKEN, at));
        }
    }
}
