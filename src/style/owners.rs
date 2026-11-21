//! An owner's possessive agrees with what it owns: `return target creature to its owner's
//! hand`, `return two target creatures to their owners' hands`. Oracle also writes `their
//! owner's hand` for a group, but never `its` for one, nor `their` for one object.
use super::view::{BreachSink, TokenSlice};
use super::{BROKEN, PLURAL, SINGULAR, clause_start};

const SINGULAR_ZONES: [&str; 3] = ["hand", "library", "graveyard"];
const PLURAL_ZONES: [&str; 3] = ["hands", "libraries", "graveyards"];
/// The verbs that move what an owner's zone receives.
const MOVES: [&str; 6] = ["return", "returns", "put", "puts", "shuffle", "shuffles"];
/// Words that leave the moved object's head in doubt: another noun phrase, or a list.
const NESTED: [&str; 7] = ["to", "of", "from", "with", "attached", "and", "or"];

pub(super) fn agreement<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    for at in 2..tokens.len() {
        let one = tokens.is_word(at, "owner's");
        if !one && !tokens.is_word(at, "owners'") {
            continue;
        }
        let its = tokens.is_word(at - 1, "its");
        if !its && !tokens.is_word(at - 1, "their") {
            continue;
        }
        let zones = if one { PLURAL_ZONES } else { SINGULAR_ZONES };
        let zone_breaks = zones.iter().any(|zone| tokens.is_word(at + 1, zone));
        let number_breaks = moved_number(tokens, at - 2).is_some_and(|many| many == its);
        if zone_breaks || number_breaks {
            found.push("owner agrees", BROKEN, at);
        }
    }
}

/// Whether the object a move sends to `to` is plural, when its head noun says so.
fn moved_number<T: TokenSlice + ?Sized>(tokens: &T, to: usize) -> Option<bool> {
    if !["to", "into", "under"]
        .iter()
        .any(|word| tokens.is_word(to, word))
    {
        return None;
    }
    let verb = (clause_start(tokens, to)..to)
        .rev()
        .find(|&at| MOVES.iter().any(|word| tokens.is_word(at, word)))?;
    let start = verb + 1;
    // `Of` in a count (`any number of`, `each of`) is not another noun phrase.
    let nested = |at: usize| {
        tokens.word(at).is_some_and(|word| NESTED.contains(&word))
            && !(tokens.is_word(at, "of")
                && at > start
                && (tokens.is_word(at - 1, "number") || tokens.is_word(at - 1, "each")))
    };
    if (start..to).any(nested) {
        return None;
    }
    let mut head = None;
    for at in start..to {
        let word = tokens.word(at)?;
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
