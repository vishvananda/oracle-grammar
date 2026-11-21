//! One object of either kind joins the kinds with `or`: "an artifact or creature card",
//! "a Spirit or Arcane spell". Oracle writes `and` there only between adjectives, as in
//! "a tapped and attacking token", in 602 singular unions of kinds. A spell names its
//! kinds without `permanent`, except as "a permanent spell": every `permanent` sits
//! right before `spell` in 2,503 of 2,504 Oracle phrases `a … spell`.
use super::BROKEN;
use super::view::{BreachSink, TokenSlice};

const HEADS: [&str; 5] = ["spell", "card", "permanent", "source", "token"];
const TYPES: [&str; 9] = [
    "artifact",
    "battle",
    "creature",
    "enchantment",
    "instant",
    "land",
    "planeswalker",
    "sorcery",
    "kindred",
];

pub(super) fn singular_or<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    for at in 0..tokens.len() {
        if !(tokens.is_word(at, "a") || tokens.is_word(at, "an"))
            || !tokens.is_word(at + 2, "and")
            || !kind(tokens, at + 1)
            || !kind(tokens, at + 3)
        {
            continue;
        }
        if HEADS.iter().any(|head| tokens.is_word(at + 4, head)) {
            found.push("one object joins kinds with or", BROKEN, at + 2);
        }
    }
}

/// "a Knight permanent or Equipment spell" or "a permanent Knight spell": `permanent`
/// is a kind of spell only as "a permanent spell".
pub(super) fn spell_kinds<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    for spell in 0..tokens.len() {
        if !tokens.is_word(spell, "spell") {
            continue;
        }
        let Some(article) = (spell.saturating_sub(6)..spell)
            .rev()
            .take_while(|&at| tokens.word(at).is_some() && !tokens.is_word(at, "as"))
            .find(|&at| tokens.is_word(at, "a") || tokens.is_word(at, "an"))
        else {
            continue;
        };
        if let Some(at) = (article + 1..spell - 1).find(|&at| tokens.is_word(at, "permanent")) {
            found.push("a spell's kinds", BROKEN, at);
        }
    }
}

/// A card type or a subtype: the words a union of kinds joins.
fn kind<T: TokenSlice + ?Sized>(tokens: &T, at: usize) -> bool {
    tokens.word(at).is_some_and(|word| {
        TYPES.contains(&word) || word.chars().next().is_some_and(char::is_uppercase)
    })
}
