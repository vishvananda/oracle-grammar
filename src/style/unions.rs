//! One object of either kind joins the kinds with `or`: "an artifact or creature card",
//! "a Spirit or Arcane spell". Oracle writes `and` there only between adjectives, as in
//! "a tapped and attacking token", in 602 singular unions of kinds. A spell names its
//! kinds without `permanent`, except as "a permanent spell": every `permanent` sits
//! right before `spell` in 2,503 of 2,504 Oracle phrases `a … spell`.
use super::{BROKEN, Found, is};
use crate::tokens::Token;

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

pub(super) fn singular_or(tokens: &[Token], found: &mut Found) {
    for at in 0..tokens.len() {
        if !(is(tokens, at, "a") || is(tokens, at, "an"))
            || !is(tokens, at + 2, "and")
            || !kind(tokens, at + 1)
            || !kind(tokens, at + 3)
        {
            continue;
        }
        if HEADS.iter().any(|head| is(tokens, at + 4, head)) {
            found.push(("one object joins kinds with or", BROKEN, at + 2));
        }
    }
}

/// "a Knight permanent or Equipment spell" or "a permanent Knight spell": `permanent`
/// is a kind of spell only as "a permanent spell".
pub(super) fn spell_kinds(tokens: &[Token], found: &mut Found) {
    for spell in 0..tokens.len() {
        if !is(tokens, spell, "spell") {
            continue;
        }
        let Some(article) = (spell.saturating_sub(6)..spell)
            .rev()
            .take_while(|&at| matches!(tokens[at], Token::Word(_)) && !is(tokens, at, "as"))
            .find(|&at| is(tokens, at, "a") || is(tokens, at, "an"))
        else {
            continue;
        };
        if let Some(at) = (article + 1..spell - 1).find(|&at| is(tokens, at, "permanent")) {
            found.push(("a spell's kinds", BROKEN, at));
        }
    }
}

/// A card type or a subtype: the words a union of kinds joins.
fn kind(tokens: &[Token], at: usize) -> bool {
    match tokens.get(at) {
        Some(Token::Word(word)) => {
            TYPES.contains(&word.as_str()) || word.chars().next().is_some_and(char::is_uppercase)
        }
        _ => false,
    }
}
