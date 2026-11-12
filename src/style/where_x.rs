//! `where X is` defines an X its own sentence already used: `create an X/X token, where
//! X is …`. Oracle never defines X ahead of the sentence that uses it.
use super::{BROKEN, Found, is};
use crate::tokens::Token;

pub(super) fn defined_after_use(tokens: &[Token], found: &mut Found) {
    let mut start = 0;
    for (at, token) in tokens.iter().enumerate() {
        if matches!(token, Token::Newline | Token::Mark('.')) {
            start = at + 1;
        } else if is(tokens, at, "where")
            && is(tokens, at + 1, "X")
            && is(tokens, at + 2, "is")
            && !(start..at).any(|used| is(tokens, used, "X"))
        {
            found.push(("where X follows its use", BROKEN, at));
        }
    }
}
