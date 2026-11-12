//! An owner's possessive agrees with what it owns: `return target creature to its owner's
//! hand`, `return two target creatures to their owners' hands`. Oracle also writes `their
//! owner's hand` for a group, but never `its` for one, nor `their` for one object.
use super::{BROKEN, Found, PLURAL, SINGULAR, clause_start, is};
use crate::tokens::Token;

const SINGULAR_ZONES: [&str; 3] = ["hand", "library", "graveyard"];
const PLURAL_ZONES: [&str; 3] = ["hands", "libraries", "graveyards"];
/// The verbs that move what an owner's zone receives.
const MOVES: [&str; 6] = ["return", "returns", "put", "puts", "shuffle", "shuffles"];
/// Words that leave the moved object's head in doubt: another noun phrase, or a list.
const NESTED: [&str; 7] = ["to", "of", "from", "with", "attached", "and", "or"];

pub(super) fn agreement(tokens: &[Token], found: &mut Found) {
    for at in 2..tokens.len() {
        let one = is(tokens, at, "owner's");
        if !one && !is(tokens, at, "owners'") {
            continue;
        }
        let its = is(tokens, at - 1, "its");
        if !its && !is(tokens, at - 1, "their") {
            continue;
        }
        let zones = if one { PLURAL_ZONES } else { SINGULAR_ZONES };
        let zone_breaks = zones.iter().any(|zone| is(tokens, at + 1, zone));
        let number_breaks = moved_number(tokens, at - 2).is_some_and(|many| many == its);
        if zone_breaks || number_breaks {
            found.push(("owner agrees", BROKEN, at));
        }
    }
}

/// Whether the object a move sends to `to` is plural, when its head noun says so.
fn moved_number(tokens: &[Token], to: usize) -> Option<bool> {
    if !["to", "into", "under"]
        .iter()
        .any(|word| is(tokens, to, word))
    {
        return None;
    }
    let verb = (clause_start(tokens, to)..to)
        .rev()
        .find(|&at| MOVES.iter().any(|word| is(tokens, at, word)))?;
    let moved = &tokens[verb + 1..to];
    // `Of` in a count (`any number of`, `each of`) is not another noun phrase.
    let nested = |at: usize| {
        matches!(&moved[at], Token::Word(word) if NESTED.contains(&word.as_str()))
            && !(is(moved, at, "of")
                && at > 0
                && (is(moved, at - 1, "number") || is(moved, at - 1, "each")))
    };
    if (0..moved.len()).any(nested) {
        return None;
    }
    let mut head = None;
    for token in moved {
        let Token::Word(word) = token else {
            return None;
        };
        let word = word.as_str();
        let number = if SINGULAR.contains(&word) {
            Some(false)
        } else if PLURAL.contains(&word) && word != "they" {
            Some(true)
        } else {
            None
        };
        match number {
            Some(many) => head = Some(many),
            None if head.is_some() => break,
            None => {}
        }
    }
    head
}
