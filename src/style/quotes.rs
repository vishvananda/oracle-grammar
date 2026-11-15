//! A quoted ability that ends its sentence keeps the one period inside the quote:
//! `gain "Evoke—Exile a blue card from your hand."`. Oracle never writes `.".`.
use super::{BROKEN, Found};
use crate::tokens::Token;

pub(super) fn one_period(tokens: &[Token], found: &mut Found) {
    for (at, window) in tokens.windows(3).enumerate() {
        if matches!(
            window,
            [Token::Mark('.'), Token::Mark('"'), Token::Mark('.')]
        ) {
            found.push(("period after a quoted sentence", BROKEN, at + 2));
        }
    }
}
