//! Casting-state conditions describe this spell; damage subjects can name it.
use super::view::{BreachSink, TokenSlice};
use super::{BROKEN, SECONDLY};

pub(super) fn wording<T: TokenSlice + ?Sized, S: BreachSink>(
    tokens: &T,
    context: &[String],
    found: &mut S,
) {
    if !context
        .iter()
        .any(|kind| matches!(kind.as_str(), "Instant" | "Sorcery"))
    {
        return;
    }
    let len = tokens.len();
    for at in 0..len {
        if (tokens.is_self_name(at) || tokens.is_word(at, "he") || tokens.is_word(at, "she"))
            && kicked_predicate(tokens, at + 1)
        {
            found.push("casting-state subject", BROKEN, at);
        }
        if tokens.is_word(at, "it") && kicked_predicate(tokens, at + 1) {
            found.push("casting-state subject", SECONDLY, at);
        }
        if tokens.is_word(at, "this")
            && !tokens.is_word(at + 1, "spell")
            && kicked_predicate(tokens, at + 2)
        {
            found.push("casting-state subject", BROKEN, at);
        }
        if tokens.is_word(at, "then") && tokens.is_word(at + 1, "if") {
            let end = (at + 2..(at + 10).min(len)).find(|&i| kicked_predicate(tokens, i));
            if end.is_some() {
                found.push("casting-state continuation", SECONDLY, at);
            }
        }
        if !tokens.is_newline(at)
            || !tokens.is_word(at + 1, "if")
            || at == 0
            || !tokens.is_mark(at - 1, '.')
        {
            continue;
        }
        let subject = at + 2;
        let end = if tokens.is_word(subject, "this") && tokens.is_word(subject + 1, "spell") {
            subject + 2
        } else if tokens.is_word(subject, "it") || tokens.is_self_name(subject) {
            subject + 1
        } else {
            continue;
        };
        if !kicked_predicate(tokens, end) {
            continue;
        }
        let begin = (0..at)
            .rev()
            .find(|&i| tokens.is_newline(i))
            .map_or(0, |i| i + 1);
        // A worded Kicker cost ends in a period too, but its header must retain
        // the newline separating it from the spell's resolution.
        if !(begin..at).any(|i| tokens.is_word(i, "kicker")) {
            found.push("casting-state continuation", SECONDLY, at);
        }
    }
}

// The generic filter grammar also admits copular descriptions of Kicked.
// They must not evade the source-subject preference by adding an article.
fn kicked_predicate<T: TokenSlice + ?Sized>(tokens: &T, at: usize) -> bool {
    if !(tokens.is_word(at, "was") || tokens.is_word(at, "is")) {
        return false;
    }
    let end = (at + 6).min(tokens.len());
    for i in at + 1..end {
        if tokens.is_newline(i) || matches!(tokens.mark(i), Some(',' | '.')) {
            return false;
        }
        if tokens.is_word(i, "kicked") {
            return true;
        }
    }
    false
}
