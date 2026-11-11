//! Casting-state conditions describe this spell; damage subjects can name it.
use super::{BROKEN, Found, SECONDLY, is};
use crate::tokens::Token;

pub(super) fn wording(tokens: &[Token], context: &[String], found: &mut Found) {
    if !context
        .iter()
        .any(|kind| matches!(kind.as_str(), "Instant" | "Sorcery"))
    {
        return;
    }
    for (at, token) in tokens.iter().enumerate() {
        if (matches!(token, Token::SelfName(_)) || is(tokens, at, "he") || is(tokens, at, "she"))
            && kicked_predicate(tokens, at + 1)
        {
            found.push(("casting-state subject", BROKEN, at));
        }
        if is(tokens, at, "it") && kicked_predicate(tokens, at + 1) {
            found.push(("casting-state subject", SECONDLY, at));
        }
        if is(tokens, at, "this")
            && !is(tokens, at + 1, "spell")
            && kicked_predicate(tokens, at + 2)
        {
            found.push(("casting-state subject", BROKEN, at));
        }
        if is(tokens, at, "then") && is(tokens, at + 1, "if") {
            let end = (at + 2..(at + 10).min(tokens.len())).find(|&i| kicked_predicate(tokens, i));
            if end.is_some() {
                found.push(("casting-state continuation", SECONDLY, at));
            }
        }
        if *token != Token::Newline
            || !is(tokens, at + 1, "if")
            || at == 0
            || tokens[at - 1] != Token::Mark('.')
        {
            continue;
        }
        let subject = at + 2;
        let end = if is(tokens, subject, "this") && is(tokens, subject + 1, "spell") {
            subject + 2
        } else if is(tokens, subject, "it")
            || matches!(tokens.get(subject), Some(Token::SelfName(_)))
        {
            subject + 1
        } else {
            continue;
        };
        if !kicked_predicate(tokens, end) {
            continue;
        }
        let begin = tokens[..at]
            .iter()
            .rposition(|t| *t == Token::Newline)
            .map_or(0, |i| i + 1);
        // A worded Kicker cost ends in a period too, but its header must retain
        // the newline separating it from the spell's resolution.
        if !(begin..at).any(|i| is(tokens, i, "kicker")) {
            found.push(("casting-state continuation", SECONDLY, at));
        }
    }
}

// The generic filter grammar also admits copular descriptions of Kicked.
// They must not evade the source-subject preference by adding an article.
fn kicked_predicate(tokens: &[Token], at: usize) -> bool {
    (is(tokens, at, "was") || is(tokens, at, "is"))
        && tokens
            .iter()
            .skip(at + 1)
            .take(5)
            .take_while(|t| !matches!(t, Token::Mark(',' | '.') | Token::Newline))
            .any(|t| matches!(t, Token::Word(word) if word == "kicked"))
}
