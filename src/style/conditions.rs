//! A combat condition trails only a sentence about the object it names: "This creature
//! has flying as long as it's attacking", in 19 of 19 trailing `as long as … attacking`
//! or `… blocking` conditions. About a player it leads: "As long as this creature is
//! attacking, defending player can't cast spells". A triggered ability asks whether
//! "its additional cost was paid", never "this spell's", and a spell that may behold
//! one creature asks whether that kind "was beheld".
use super::BROKEN;
use super::view::{BreachSink, TokenSlice};

const PLAYERS: [&[&str]; 7] = [
    &["defending", "player"],
    &["each", "player"],
    &["each", "opponent"],
    &["target", "player"],
    &["your", "opponents"],
    &["players"],
    &["you"],
];

pub(super) fn combat_condition_leads<T: TokenSlice + ?Sized, S: BreachSink>(
    tokens: &T,
    found: &mut S,
) {
    let len = tokens.len();
    for at in 1..len {
        if !(tokens.is_word(at, "as")
            && tokens.is_word(at + 1, "long")
            && tokens.is_word(at + 2, "as"))
        {
            continue;
        }
        let combat = (at + 3..(at + 8).min(len))
            .any(|i| tokens.is_word(i, "attacking") || tokens.is_word(i, "blocking"));
        let start = (0..at)
            .rev()
            .find(|&i| tokens.is_newline(i) || matches!(tokens.mark(i), Some('.' | '"' | ',')))
            .map_or(0, |i| i + 1);
        let player = PLAYERS.iter().any(|words| {
            words
                .iter()
                .enumerate()
                .all(|(offset, word)| tokens.is_word(start + offset, word))
        });
        if combat && start < at && player {
            found.push("a combat condition about a player leads", BROKEN, at);
        }
    }
}

/// "When this creature enters, if its additional cost was paid": by the time a
/// permanent's ability triggers it is no longer a spell (Graven Archfiend, 1 of 1).
pub(super) fn trigger_asks_its_cost<T: TokenSlice + ?Sized, S: BreachSink>(
    tokens: &T,
    found: &mut S,
) {
    for at in 0..tokens.len() {
        if !(tokens.is_word(at, "this")
            && tokens.is_word(at + 1, "spell's")
            && tokens.is_word(at + 2, "additional")
            && tokens.is_word(at + 3, "cost"))
        {
            continue;
        }
        let start = (0..at)
            .rev()
            .find(|&i| tokens.is_newline(i) || matches!(tokens.mark(i), Some('.' | '"')))
            .map_or(0, |i| i + 1);
        if ["when", "whenever", "at"]
            .iter()
            .any(|word| tokens.is_word(start, word))
        {
            found.push(
                "a trigger asks whether this spell's cost was paid",
                BROKEN,
                at,
            );
        }
    }
}

/// "As an additional cost to cast this spell, you may behold a Dragon. ... If a Dragon
/// was beheld": with one creature to behold, the test names it (6 of 7, Hulk's
/// Thunderclap the exception), not "this spell's additional cost was paid".
pub(super) fn behold_names_its_kind<T: TokenSlice + ?Sized, S: BreachSink>(
    tokens: &T,
    found: &mut S,
) {
    let len = tokens.len();
    let Some(behold) = (0..len).find(|&at| {
        tokens.is_word(at, "you")
            && tokens.is_word(at + 1, "may")
            && tokens.is_word(at + 2, "behold")
            && (tokens.is_word(at + 3, "a") || tokens.is_word(at + 3, "an"))
    }) else {
        return;
    };
    for at in behold..len {
        if tokens.is_word(at, "this")
            && tokens.is_word(at + 1, "spell's")
            && tokens.is_word(at + 2, "additional")
            && tokens.is_word(at + 3, "cost")
            && tokens.is_word(at + 4, "was")
            && tokens.is_word(at + 5, "paid")
        {
            found.push("a beheld kind is tested by name", BROKEN, at);
        }
    }
}
