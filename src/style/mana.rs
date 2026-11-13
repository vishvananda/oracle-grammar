//! Canonical color order for inherent mana headers; keys retain every other order.
use super::{BROKEN, Found, is};
use crate::tokens::Token;

fn color(tokens: &[Token], at: usize) -> Option<u8> {
    if tokens.get(at) != Some(&Token::Mark('{')) || tokens.get(at + 2) != Some(&Token::Mark('}')) {
        return None;
    }
    let Token::Word(word) = tokens.get(at + 1)? else {
        return None;
    };
    match word.as_str() {
        "W" => Some(0),
        "U" => Some(1),
        "B" => Some(2),
        "R" => Some(3),
        "G" => Some(4),
        "C" => Some(5),
        _ => None,
    }
}

fn options(tokens: &[Token], mut at: usize) -> Option<Vec<u8>> {
    let mut colors = Vec::new();
    for _ in 0..5 {
        colors.push(color(tokens, at)?);
        at += 3;
        if tokens.get(at) == Some(&Token::Mark('.'))
            && tokens.get(at + 1) == Some(&Token::Mark(')'))
        {
            return Some(colors);
        }
        if tokens.get(at) == Some(&Token::Mark(',')) {
            at += 1;
            at += usize::from(is(tokens, at, "or"));
        } else if is(tokens, at, "or") {
            at += 1;
        } else {
            return None;
        }
    }
    None
}

fn canonical(colors: &[u8]) -> Option<&'static [u8]> {
    Some(match colors {
        [0, 1] => &[0, 1],
        [0, 2] => &[0, 2],
        [0, 3] => &[3, 0],
        [0, 4] => &[4, 0],
        [1, 2] => &[1, 2],
        [1, 3] => &[1, 3],
        [1, 4] => &[4, 1],
        [2, 3] => &[2, 3],
        [2, 4] => &[2, 4],
        [3, 4] => &[3, 4],
        [0, 1, 2] => &[0, 1, 2],
        [0, 1, 3] => &[1, 3, 0],
        [0, 1, 4] => &[4, 0, 1],
        [0, 2, 3] => &[3, 0, 2],
        [0, 2, 4] => &[0, 2, 4],
        [0, 3, 4] => &[3, 4, 0],
        [1, 2, 3] => &[1, 2, 3],
        [1, 2, 4] => &[2, 4, 1],
        [1, 3, 4] => &[4, 1, 3],
        [2, 3, 4] => &[2, 3, 4],
        [0, 1, 2, 3] => &[0, 1, 2, 3],
        [0, 1, 2, 4] => &[4, 0, 1, 2],
        [0, 1, 3, 4] => &[3, 4, 0, 1],
        [0, 2, 3, 4] => &[2, 3, 4, 0],
        [1, 2, 3, 4] => &[1, 2, 3, 4],
        [0, 1, 2, 3, 4] => &[0, 1, 2, 3, 4],
        _ => return None,
    })
}

pub(super) fn wording(tokens: &[Token], found: &mut Found) {
    production_bundles(tokens, found);
    let mut fragment = Vec::new();
    let mut at = 0;
    while fragment.len() < 128 {
        let Some(unit) = color(tokens, at) else {
            break;
        };
        fragment.push(unit);
        at += 3;
    }
    if at == tokens.len() && !fragment.is_empty() && noncanonical(&fragment) {
        found.push(("literal mana order", BROKEN, 0));
    }
    for open in 0..tokens.len() {
        if tokens.get(open) != Some(&Token::Mark('('))
            || tokens.get(open + 1) != Some(&Token::Mark('{'))
            || !is(tokens, open + 2, "T")
            || tokens.get(open + 3) != Some(&Token::Mark('}'))
            || tokens.get(open + 4) != Some(&Token::Mark(':'))
            || !is(tokens, open + 5, "add")
        {
            continue;
        }
        let Some(colors) = options(tokens, open + 6) else {
            continue;
        };
        let mut sorted = colors.clone();
        sorted.sort_unstable();
        if canonical(&sorted).is_some_and(|wanted| wanted != colors) {
            found.push(("mana header order", BROKEN, open));
        }
    }
}

/// Literal totals admit every order. Prefer grouped units in the established
/// color-cycle order; a spelling key can still select any other permutation.
fn production_bundles(tokens: &[Token], found: &mut Found) {
    for verb in 0..tokens.len() {
        if !is(tokens, verb, "add") && !is(tokens, verb, "adds") {
            continue;
        }
        let mut at = verb + 1;
        if is(tokens, at, "an") && is(tokens, at + 1, "additional") {
            at += 2;
        }
        loop {
            let start = at;
            let mut units = Vec::new();
            while units.len() < 128 {
                let Some(unit) = color(tokens, at) else {
                    break;
                };
                units.push(unit);
                at += 3;
            }
            if units.is_empty() {
                break;
            }
            if noncanonical(&units) {
                found.push(("literal mana order", BROKEN, start));
            }
            if tokens.get(at) == Some(&Token::Mark(',')) {
                at += 1;
                at += usize::from(is(tokens, at, "or"));
            } else if is(tokens, at, "or") {
                at += 1;
            } else {
                break;
            }
        }
    }
}

fn noncanonical(units: &[u8]) -> bool {
    let mut distinct = units.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    let colorless = distinct.last() == Some(&5);
    if colorless {
        distinct.pop();
    }
    let mut order = canonical(&distinct).unwrap_or(&distinct).to_vec();
    if colorless {
        order.insert(0, 5);
    }
    let desired: Vec<_> = order
        .into_iter()
        .flat_map(|unit| std::iter::repeat_n(unit, units.iter().filter(|u| **u == unit).count()))
        .collect();
    desired != units
}
