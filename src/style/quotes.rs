//! A quoted ability that ends its sentence keeps the one period inside the quote:
//! `gain "Evoke—Exile a blue card from your hand."`. Oracle never writes `.".`.
use super::BROKEN;
use super::view::{BreachSink, TokenSlice};

pub(super) fn one_period<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    let len = tokens.len();
    if len < 3 {
        return;
    }
    for at in 0..=len - 3 {
        if tokens.is_mark(at, '.') && tokens.is_mark(at + 1, '"') && tokens.is_mark(at + 2, '.') {
            found.push("period after a quoted sentence", BROKEN, at + 2);
        }
    }
}
