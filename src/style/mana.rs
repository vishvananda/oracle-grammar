//! Canonical color order for inherent mana headers, and spending restrictions that agree
//! with the mana they follow; keys retain every other wording.
use super::view::{BreachSink, F_ACTIVATE, F_ADD, F_CAST, F_LBRACE, F_LPAREN, F_SPEND, TokenSlice};
use super::{BROKEN, SOFTLY};

fn color<T: TokenSlice + ?Sized>(tokens: &T, at: usize) -> Option<u8> {
    if !tokens.is_mark(at, '{') || !tokens.is_mark(at + 2, '}') {
        return None;
    }
    match tokens.word(at + 1)? {
        "W" => Some(0),
        "U" => Some(1),
        "B" => Some(2),
        "R" => Some(3),
        "G" => Some(4),
        "C" => Some(5),
        _ => None,
    }
}

fn options<T: TokenSlice + ?Sized>(tokens: &T, mut at: usize) -> Option<Vec<u8>> {
    let mut colors = Vec::new();
    for _ in 0..5 {
        colors.push(color(tokens, at)?);
        at += 3;
        if tokens.is_mark(at, '.') && tokens.is_mark(at + 1, ')') {
            return Some(colors);
        }
        if tokens.is_mark(at, ',') {
            at += 1;
            at += usize::from(tokens.is_word(at, "or"));
        } else if tokens.is_word(at, "or") {
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

pub(super) fn wording<T: TokenSlice + ?Sized, S: BreachSink>(
    tokens: &T,
    flags: u64,
    found: &mut S,
) {
    if (flags & F_ADD != 0) && (flags & F_LBRACE != 0) {
        production_bundles(tokens, found);
    }
    if flags & F_SPEND != 0 {
        spending_agrees(tokens, found);
    }
    if (flags & F_CAST != 0) && (flags & F_ACTIVATE != 0) {
        uses_agree(tokens, found);
    }
    if flags & F_LBRACE == 0 {
        return;
    }
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
        found.push("literal mana order", BROKEN, 0);
    }
    if flags & F_LPAREN == 0 || flags & F_ADD == 0 {
        return;
    }
    for open in 0..tokens.len() {
        if !tokens.is_mark(open, '(')
            || !tokens.is_mark(open + 1, '{')
            || !tokens.is_word(open + 2, "T")
            || !tokens.is_mark(open + 3, '}')
            || !tokens.is_mark(open + 4, ':')
            || !tokens.is_word(open + 5, "add")
        {
            continue;
        }
        let Some(colors) = options(tokens, open + 6) else {
            continue;
        };
        let mut sorted = colors.clone();
        sorted.sort_unstable();
        if canonical(&sorted).is_some_and(|wanted| wanted != colors) {
            found.push("mana header order", BROKEN, open);
        }
    }
}

/// Literal totals admit every order. Prefer grouped units in the established
/// color-cycle order; a spelling key can still select any other permutation.
fn production_bundles<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    for verb in 0..tokens.len() {
        if !tokens.is_word(verb, "add") && !tokens.is_word(verb, "adds") {
            continue;
        }
        let mut at = verb + 1;
        if tokens.is_word(at, "an") && tokens.is_word(at + 1, "additional") {
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
                found.push("literal mana order", BROKEN, start);
            }
            if tokens.is_mark(at, ',') {
                at += 1;
                at += usize::from(tokens.is_word(at, "or"));
            } else if tokens.is_word(at, "or") {
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

/// One mana is spent on one spell or ability, more on spells: "Add {G}. Spend this mana
/// only to cast a creature spell" and "Add two mana in any combination of colors. Spend
/// this mana only to cast Elemental spells". Oracle says otherwise in 11 of 146
/// restrictions, which a key keeps.
fn spending_agrees<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    let len = tokens.len();
    for at in 0..len {
        let spend = ["spend", "this", "mana", "only", "to"];
        if !(0..spend.len()).all(|offset| tokens.is_word(at + offset, spend[offset])) {
            continue;
        }
        let Some(verb) = (at.saturating_sub(64)..at)
            .rev()
            .find(|&i| tokens.is_word(i, "add"))
        else {
            continue;
        };
        let mut amount = verb + 1;
        if tokens.is_word(amount, "an") && tokens.is_word(amount + 1, "additional") {
            amount += 2;
        }
        let one = one_unit(tokens, amount)
            || (tokens.is_word(amount, "one") || tokens.number(amount) == Some(1))
                && tokens.is_word(amount + 1, "mana");
        let mut word = at + spend.len();
        while word < len && !tokens.is_mark(word, '.') {
            let number = use_number(tokens, word);
            if one && number == Some(false) || !one && number == Some(true) {
                found.push("spending agrees with the mana", 2.0 * SOFTLY, word);
            }
            word += 1;
        }
    }
}

/// "cast a creature spell or activate an ability of a creature", "cast artifact spells or
/// activate abilities of artifacts": Oracle never mixes the two, in 64 of 64.
fn uses_agree<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    let len = tokens.len();
    for at in 0..len {
        let named = ["it", "that", "this", "the", "them", "those"]
            .iter()
            .any(|word| tokens.is_word(at + 1, word));
        if !tokens.is_word(at, "cast") || named || tokens.is_self_name(at + 1) {
            continue;
        }
        let Some(cast) = use_number(tokens, at) else {
            continue;
        };
        let mut word = at + 1;
        while word < len && !tokens.is_mark(word, '.') && !tokens.is_newline(word) {
            if tokens.is_word(word, "or") && tokens.is_word(word + 1, "activate") {
                if use_number(tokens, word + 1).is_some_and(|activate| activate != cast) {
                    found.push("cast and activate agree", BROKEN, word + 1);
                }
                break;
            }
            word += 1;
        }
    }
}

/// Whether "cast …" or "activate …" at `at` names one spell or ability, or several.
fn use_number<T: TokenSlice + ?Sized>(tokens: &T, at: usize) -> Option<bool> {
    let article = tokens.is_word(at + 1, "a") || tokens.is_word(at + 1, "an");
    if tokens.is_word(at, "cast") {
        Some(article)
    } else if tokens.is_word(at, "activate") && tokens.is_word(at + 1, "abilities") {
        Some(false)
    } else if tokens.is_word(at, "activate") && article && tokens.is_word(at + 2, "ability") {
        Some(true)
    } else {
        None
    }
}

/// One mana symbol, or a choice of them, ends the sentence: `{G}.`, `{R}, {W}, or {B}.`
fn one_unit<T: TokenSlice + ?Sized>(tokens: &T, mut at: usize) -> bool {
    loop {
        if !(tokens.is_mark(at, '{')
            && tokens.word(at + 1).is_some()
            && tokens.is_mark(at + 2, '}'))
        {
            return false;
        }
        at += 3;
        if tokens.is_mark(at, '.') {
            return true;
        }
        if tokens.is_mark(at, ',') {
            at += 1;
        }
        if tokens.is_word(at, "or") {
            at += 1;
        } else if !tokens.is_mark(at - 1, ',') {
            return false;
        }
    }
}
