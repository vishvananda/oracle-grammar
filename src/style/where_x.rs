//! `where X is` defines an X its own sentence already used: `create an X/X token, where
//! X is …`. Oracle never defines X ahead of the sentence that uses it.
use super::BROKEN;
use super::view::{BreachSink, TokenSlice};

pub(super) fn defined_after_use<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    let mut start = 0;
    for at in 0..tokens.len() {
        if tokens.is_newline(at) || tokens.is_mark(at, '.') {
            start = at + 1;
        } else if tokens.is_word(at, "where")
            && tokens.is_word(at + 1, "X")
            && tokens.is_word(at + 2, "is")
            && !(start..at).any(|used| tokens.is_word(used, "X"))
        {
            found.push("where X follows its use", BROKEN, at);
        }
    }
}
