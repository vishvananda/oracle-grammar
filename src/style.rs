//! Printing style: how likely a run of tokens is, by the trigrams of Oracle text. One
//! meaning has many texts in the grammar, and this ranks them. It never adds or removes
//! a text, so what prints still reads back through the same rules.
use crate::tokens::Token;
use rustc_hash::FxHashMap as HashMap;

mod casting;
mod mana;
mod owners;

const BACKOFF: f32 = 0.4;
/// The score of a token the corpus never had.
const UNSEEN: f32 = -14.0;
/// What the table calls a line's start.
pub const LINE: &str = "$";

/// A token as the table spells it. Numbers count alike, and so do card names.
pub fn key_of(token: &Token) -> String {
    match token {
        Token::Word(word) => word.clone(),
        Token::Number(_) | Token::Grouped(_) => "#".to_string(),
        Token::Mark(mark) => mark.to_string(),
        Token::SelfName(_) => "@".to_string(),
        Token::Newline => LINE.to_string(),
    }
}

/// What breaking a canonical style rule costs: more than any run of trigrams, and more
/// than a `@prefer`, but less than one step of a card's key.
const BROKEN: f32 = 500.0;
/// What a wording costs that Oracle uses, but seldom.
const SOFTLY: f32 = 4.0;
/// A second name for the card in a sentence: rarer than `it`, but it does happen. It
/// outweighs the trigrams and the `it` that passes over the card's word for itself,
/// so `it` reads after the card is named and nowhere else, but not a broken rule.
const SECONDLY: f32 = 150.0;

/// Canonical style rules the trigrams are too short to see, with what breaking each costs.
pub fn broken_rules(tokens: &[Token], context: &[String]) -> f32 {
    breaches(tokens, context)
        .iter()
        .map(|(_, cost, _)| cost)
        .sum()
}

/// Each rule `tokens` breaks: its name, its cost and where.
pub fn breaches(tokens: &[Token], context: &[String]) -> Vec<(&'static str, f32, usize)> {
    let mut found = Vec::new();
    sacrificed_and_drew(tokens, &mut found);
    for (at, token) in tokens.iter().enumerate() {
        let Token::Word(word) = token else {
            continue;
        };
        counted_noun(tokens, at, word, &mut found);
        verb_agrees(tokens, at, word, &mut found);
        self_noun(tokens, at, word, &mut found);
        attachment(tokens, at, word, context, &mut found);
    }
    signs_match(tokens, &mut found);
    has_for_a_while(tokens, &mut found);
    self_named_again(tokens, &mut found);
    joined_verbs(tokens, &mut found);
    plain_phrasing(tokens, &mut found);
    subject_named(tokens, &mut found);
    numeric_stat_phrasing(tokens, &mut found);
    shared_defender_condition(tokens, &mut found);
    reminder_wording(tokens, &mut found);
    casting::wording(tokens, context, &mut found);
    mana::wording(tokens, &mut found);
    owners::agreement(tokens, &mut found);
    found
}

type Found = Vec<(&'static str, f32, usize)>;

/// Prefer the common keyword templates; a key retains other printed variants.
fn reminder_wording(tokens: &[Token], found: &mut Found) {
    for (open, token) in tokens.iter().enumerate() {
        if *token != Token::Mark('(') || open == 0 {
            continue;
        }
        let bare_source = |at| is(tokens, at, "this") && is(tokens, at + 1, "deals");
        let deathtouch = is(tokens, open - 1, "deathtouch")
            && is(tokens, open + 1, "any")
            && is(tokens, open + 4, "damage")
            && !bare_source(open + 5);
        let wither = is(tokens, open - 1, "wither") && !bare_source(open + 1);
        if deathtouch || wither {
            found.push(("reminder source", BROKEN, open));
        }
        if is(tokens, open - 1, "haste") {
            let end = tokens[open + 1..]
                .iter()
                .position(|token| *token == Token::Mark(')'))
                .map_or(tokens.len(), |at| open + 1 + at);
            if (open + 1..end).any(|at| {
                is(tokens, at, "can") && is(tokens, at + 1, "attack") && is(tokens, at + 2, "as")
            }) {
                found.push(("haste reminder without tap", BROKEN, open));
            }
        }
    }
}

/// A condition shared by characteristic and defender changes precedes their
/// conjunction, keeping its scope clear across both parts of the sentence.
fn shared_defender_condition(tokens: &[Token], found: &mut Found) {
    for at in 0..tokens.len() {
        if !is(tokens, at, "can")
            || !is(tokens, at + 1, "attack")
            || !is(tokens, at + 2, "as")
            || !is(tokens, at + 3, "though")
        {
            continue;
        }
        let start = clause_start(tokens, at);
        let joined = (start..at).any(|i| is(tokens, i, "and"));
        let characteristic = (start..at).any(|i| {
            ["get", "gets", "gain", "gains", "has", "have"]
                .iter()
                .any(|word| is(tokens, i, word))
        });
        let end = (at..tokens.len())
            .find(|&i| matches!(tokens[i], Token::Newline | Token::Mark('.' | '"')))
            .unwrap_or(tokens.len());
        if joined
            && characteristic
            && (at + 4..end).any(|i| {
                is(tokens, i, "as") && is(tokens, i + 1, "long") && is(tokens, i + 2, "as")
            })
        {
            found.push(("shared defender condition", SOFTLY, at));
        }
    }
}

/// A single numeric bound takes `power 3 or less`, while a compound expression
/// keeps `equal to 1 plus ...`. Both wordings remain available to the grammar.
fn numeric_stat_phrasing(tokens: &[Token], found: &mut Found) {
    for at in 0..tokens.len() {
        if !is(tokens, at, "with") {
            continue;
        }
        let start = if is(tokens, at + 1, "mana") && is(tokens, at + 2, "value") {
            at + 3
        } else if is(tokens, at + 1, "power") || is(tokens, at + 1, "toughness") {
            at + 2
        } else {
            continue;
        };
        let number = if is(tokens, start, "equal") && is(tokens, start + 1, "to") {
            start + 2
        } else if (is(tokens, start, "less") || is(tokens, start, "greater"))
            && is(tokens, start + 1, "than")
            && is(tokens, start + 2, "or")
            && is(tokens, start + 3, "equal")
            && is(tokens, start + 4, "to")
        {
            start + 5
        } else {
            continue;
        };
        let numeric =
            is(tokens, number, "X") || matches!(tokens.get(number), Some(Token::Number(_)));
        let compound = is(tokens, number + 1, "plus") || is(tokens, number + 1, "minus");
        if numeric && !compound {
            found.push(("numeric stat phrase", SOFTLY, start));
        }
    }
}

fn is(tokens: &[Token], at: usize, text: &str) -> bool {
    matches!(tokens.get(at), Some(Token::Word(word)) if word == text)
}

fn names_self(tokens: &[Token], at: usize) -> bool {
    is(tokens, at, "this") || matches!(tokens.get(at), Some(Token::SelfName(_)))
}

/// Where a run of words ends going back from `at`: a sentence, line or clause boundary.
fn clause_start(tokens: &[Token], at: usize) -> usize {
    (0..at)
        .rev()
        .find(|&i| {
            matches!(
                tokens[i],
                Token::Newline | Token::Mark('.' | ',' | ':' | '"' | '\u{2022}')
            )
        })
        .map_or(0, |i| i + 1)
}

/// A cost that sacrifices the card leaves it gone, so the effect calls it `It`. One who
/// draws and loses life is named twice: `You draw a card and you lose 1 life`.
fn sacrificed_and_drew(tokens: &[Token], found: &mut Found) {
    let (mut sacrificed, mut drew) = (false, false);
    for (at, token) in tokens.iter().enumerate() {
        match token {
            Token::Newline => (sacrificed, drew) = (false, false),
            Token::Mark('.') => {
                if drew && is(tokens, at + 1, "you") && is(tokens, at + 2, "lose") {
                    found.push(("draw and lose", BROKEN, at));
                }
                drew = false;
            }
            Token::Word(w) if w == "sacrifice" && names_self(tokens, at + 1) => sacrificed = true,
            Token::Word(w) if w == "draw" => drew = true,
            Token::Word(w) if w == "and" && drew && is(tokens, at + 1, "lose") => {
                found.push(("draw and lose", BROKEN, at));
            }
            Token::Word(w)
                if w == "to"
                    && is(tokens, at + 1, "them")
                    && at > 0
                    && is(tokens, at - 1, "damage") =>
            {
                found.push(("damage to them", SOFTLY, at));
            }
            Token::Mark(':') => {
                if sacrificed && names_self(tokens, at + 1) {
                    found.push(("sacrificed self", SECONDLY, at));
                }
                sacrificed = false;
            }
            _ => {}
        }
    }
}

/// Whether a token counts more than one thing, when it counts at all.
fn count_of(tokens: &[Token], at: usize) -> Option<bool> {
    // The sides of a `+1/+1` or an `X/X` count nothing.
    let side = at
        .checked_sub(1)
        .is_some_and(|i| matches!(tokens[i], Token::Mark('/' | '+' | '-' | '\u{2212}')))
        || matches!(tokens.get(at + 1), Some(Token::Mark('/')));
    let sized = matches!(&tokens[at], Token::Number(_)) || is(tokens, at, "X");
    if sized && side {
        return None;
    }
    match &tokens[at] {
        Token::Number(number) => Some(*number != 1),
        Token::Word(word) => match word.as_str() {
            "a" | "an" | "one" | "another" => Some(false),
            "two" | "three" | "four" | "five" | "six" | "seven" | "eight" | "nine" | "ten"
            | "X" | "many" | "more" | "those" | "these" => Some(true),
            _ => None,
        },
        _ => None,
    }
}

/// `two +1/+1 counters`, `a Treasure token`: the noun takes the number of its count.
fn counted_noun(tokens: &[Token], at: usize, word: &str, found: &mut Found) {
    let plural = match word {
        "counter" | "token" => false,
        "counters" | "tokens" => true,
        _ => return,
    };
    // `counters target spell` is the verb.
    let verb = ["a", "an", "target", "that", "this", "it", "the"];
    if word == "counters" && verb.iter().any(|w| is(tokens, at + 1, w)) {
        return;
    }
    for index in (clause_start(tokens, at)..at).rev() {
        if is(tokens, index, "of") {
            return;
        }
        if let Some(many) = count_of(tokens, index) {
            if many != plural {
                found.push(("counted noun", BROKEN, at));
            }
            return;
        }
    }
}

const SINGULAR: [&str; 15] = [
    "creature",
    "permanent",
    "land",
    "artifact",
    "enchantment",
    "planeswalker",
    "player",
    "opponent",
    "spell",
    "card",
    "token",
    "Aura",
    "Equipment",
    "Vehicle",
    "it",
];
const PLURAL: [&str; 13] = [
    "creatures",
    "permanents",
    "lands",
    "artifacts",
    "enchantments",
    "planeswalkers",
    "players",
    "opponents",
    "spells",
    "cards",
    "tokens",
    "they",
    "those",
];

/// Whether a subject is plural, when its head can be told from its words. The head ends
/// the first run of nouns: `land creatures`, `creature cards`. A list or a subordinate
/// clause leaves it in doubt; a relative clause after the head (`creatures that entered
/// this turn`) does not.
fn plural_subject(subject: &[Token]) -> Option<bool> {
    let is_word = |token: &Token, words: &[&str]| matches!(token, Token::Word(word) if words.contains(&word.as_str()));
    if subject
        .iter()
        .any(|t| is_word(t, &["and", "or", "as", "if"]))
    {
        return None;
    }
    let mut head = None;
    for token in subject {
        match token {
            Token::SelfName(_) => return Some(false),
            _ if head.is_none() && is_word(token, &["that"]) => return None,
            // `Creature you control`, `creature they control`: a new subject ends the head.
            Token::Word(word) if word == "you" || word == "they" && head.is_some() => return head,
            Token::Word(word) if SINGULAR.contains(&word.as_str()) => head = Some(false),
            Token::Word(word) if PLURAL.contains(&word.as_str()) => head = Some(true),
            _ if head.is_some() => return head,
            _ => {}
        }
    }
    head
}

/// `Creatures you control get`, `Target creature gets`.
fn verb_agrees(tokens: &[Token], at: usize, word: &str, found: &mut Found) {
    let plural = match word {
        "get" | "gain" | "have" | "lose" => true,
        "gets" | "gains" | "has" | "loses" => false,
        _ => return,
    };
    // In `causes you to gain` and `as though it didn't have`, the infinitive has no
    // subject agreement.
    let bare = [
        "to", "didn't", "doesn't", "don't", "can", "can't", "may", "must", "would", "will",
    ];
    if plural && at > 0 && bare.iter().any(|word| is(tokens, at - 1, word)) {
        return;
    }
    let subject = &tokens[clause_start(tokens, at)..at];
    // `counter you have`: the subject is `you`, whatever came before it.
    let you = at > 0 && (is(tokens, at - 1, "you") || is(tokens, at - 1, "they"));
    if !you && plural_subject(subject).is_some_and(|many| many != plural) {
        found.push(("verb agrees", BROKEN, at));
    }
}

const CARD_NOUNS: [&str; 8] = [
    "creature",
    "permanent",
    "land",
    "artifact",
    "enchantment",
    "planeswalker",
    "Aura",
    "Equipment",
];

/// `Cast this spell`, `from your hand`: what a card is called off the battlefield.
fn self_noun(tokens: &[Token], at: usize, word: &str, found: &mut Found) {
    if word != "this"
        || !matches!(tokens.get(at + 1), Some(Token::Word(w)) if CARD_NOUNS.contains(&w.as_str()))
    {
        return;
    }
    let spent = at >= 3 && is(tokens, at - 3, "spent");
    let cast = at > 0 && is(tokens, at - 1, "cast") && !spent || is(tokens, at + 2, "costs");
    let from = is(tokens, at + 2, "from") || is(tokens, at + 2, "in");
    let zone = from
        && is(tokens, at + 3, "your")
        && (is(tokens, at + 4, "hand") || is(tokens, at + 4, "graveyard"));
    if cast || zone {
        found.push(("self off the battlefield", BROKEN, at));
    }
}

/// An Equipment says `equipped creature`, an Aura `enchanted creature`.
fn attachment(tokens: &[Token], at: usize, word: &str, context: &[String], found: &mut Found) {
    let held = |name: &str| context.iter().any(|c| c == name);
    let wrong = (word == "enchanted" && held("Equipment")) || (word == "equipped" && held("Aura"));
    let noun =
        matches!(tokens.get(at + 1), Some(Token::Word(w)) if w == "creature" || w == "permanent");
    if wrong && noun {
        found.push(("attachment", BROKEN, at));
    }
}

/// `-2/-0`: a zero takes the sign beside it.
fn signs_match(tokens: &[Token], found: &mut Found) {
    let sign = |at: usize| match tokens.get(at) {
        Some(Token::Mark(mark @ ('+' | '-' | '\u{2212}'))) => Some(*mark == '+'),
        _ => None,
    };
    for at in 0..tokens.len() {
        let slash = matches!(tokens.get(at + 2), Some(Token::Mark('/')));
        if !slash || !matches!(tokens.get(at + 1), Some(Token::Number(_))) {
            continue;
        }
        if let (Some(left), Some(right), Some(Token::Number(0))) =
            (sign(at), sign(at + 3), tokens.get(at + 4))
            && left != right
        {
            found.push(("sign of zero", BROKEN, at));
        }
    }
}

/// `Until end of turn` goes with `gains`: `has` is for what lasts. `May have it gain`
/// and `if it has` are other verbs, and `has base power` is a set value.
fn has_for_a_while(tokens: &[Token], found: &mut Found) {
    let mut start = 0;
    for at in 0..=tokens.len() {
        if at < tokens.len() && !matches!(tokens[at], Token::Newline | Token::Mark('.' | '"')) {
            continue;
        }
        let sentence = &tokens[start..at];
        let until = (0..sentence.len()).any(|i| {
            is(sentence, i, "until") && is(sentence, i + 1, "end") && is(sentence, i + 3, "turn")
        });
        let has = (0..sentence.len()).find(|&i| {
            (is(sentence, i, "has") || is(sentence, i, "have"))
                && !is(sentence, i + 1, "base")
                && !(i > 0
                    && (is(sentence, i - 1, "may")
                        || is(sentence, i - 1, "you")
                        || is(sentence, i - 1, "it")))
                && !sentence
                    .iter()
                    .any(|t| matches!(t, Token::Word(w) if w == "if"))
        });
        if let (true, Some(index)) = (until, has) {
            found.push(("has until", BROKEN, start + index));
        }
        start = at + 1;
    }
}

/// Sentences as ranges, ending at a period or a line.
fn sentences(tokens: &[Token]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    for at in 0..=tokens.len() {
        if at == tokens.len() || matches!(tokens[at], Token::Newline | Token::Mark('.' | '"')) {
            out.push((start, at));
            start = at + 1;
        }
    }
    out
}

/// Words before a second name for the card that make the name right: `it` would point
/// somewhere else after them.
const NAMES_AGAIN: [&str; 7] = ["until", "named", "to", "from", "than", "and", "or"];

/// Nouns `it` could point to instead of the card.
const RIVALS: [&str; 9] = [
    "target",
    "creature",
    "creatures",
    "card",
    "cards",
    "permanent",
    "permanents",
    "spell",
    "spells",
];
/// Words that open a clause whose subject follows them.
const OPENERS: [&str; 4] = ["when", "whenever", "if", "as"];

/// After a clause's subject names the card, the rest of it says `it`, unless what stands
/// between could take the pronoun. A card named as an object does not lend `it` its
/// name: `a spell that targets this creature, put a counter on this creature`. A sentence
/// that opens `Whenever it` never names the card after: the card comes first.
fn self_named_again(tokens: &[Token], found: &mut Found) {
    let names = |at: usize| {
        let this = is(tokens, at, "this")
            && matches!(tokens.get(at + 1), Some(Token::Word(w)) if CARD_NOUNS.contains(&w.as_str()));
        this || matches!(tokens[at], Token::SelfName(_))
    };
    let between = |at: usize, start: usize| {
        let plain = matches!(&tokens[at], Token::Word(w) if RIVALS.contains(&w.as_str()));
        let another =
            is(tokens, at, "another") && matches!(tokens.get(at + 1), Some(Token::Word(_)));
        (plain || another) && !(at > start && names(at - 1))
    };
    let subject = |at: usize, start: usize| {
        at == start
            || clause_start(tokens, at) == at
            || matches!(&tokens[at - 1], Token::Word(w) if OPENERS.contains(&w.as_str()))
    };
    for (start, end) in sentences(tokens) {
        let mut named = false;
        let mut rival = false;
        let mut pronoun = ["when", "whenever", "as"]
            .iter()
            .any(|opener| is(tokens, start, opener))
            && is(tokens, start + 1, "it");
        for at in start..end {
            if names(at) {
                let blocked = at > start
                    && matches!(&tokens[at - 1], Token::Word(w) if NAMES_AGAIN.contains(&w.as_str()));
                if named && !rival && !blocked {
                    found.push(("self named again", SECONDLY, at));
                }
                if pronoun {
                    found.push(("pronoun before name", BROKEN, start + 1));
                    pronoun = false;
                }
                named |= subject(at, start);
                rival = false;
            } else if between(at, start) {
                rival = true;
            }
        }
    }
}

/// `get +1/+0 and gain first strike`: verbs joined by `and` share a subject.
fn joined_verbs(tokens: &[Token], found: &mut Found) {
    const PAIRS: [(&str, &str); 4] = [
        ("get", "gets"),
        ("gain", "gains"),
        ("have", "has"),
        ("lose", "loses"),
    ];
    let number = |at: usize| {
        PAIRS.iter().find_map(|(many, one)| {
            if is(tokens, at, many) {
                Some(true)
            } else if is(tokens, at, one) {
                Some(false)
            } else {
                None
            }
        })
    };
    for (start, end) in sentences(tokens) {
        let mut seen = None;
        for at in start..end {
            let Some(many) = number(at) else {
                continue;
            };
            if at > start && is(tokens, at - 1, "and") && seen.is_some_and(|first| first != many) {
                found.push(("joined verbs", BROKEN, at));
            }
            seen = Some(many);
        }
    }
}

/// A verb of an object names its subject: `This land deals 1 damage to you`. Only a list
/// of what one subject does goes on without it: `gets +1/+1, gains flying`. A phrase
/// ranked alone has its subject outside it, so only a boundary before the verb counts.
fn subject_named(tokens: &[Token], found: &mut Found) {
    for (at, token) in tokens.iter().enumerate().skip(1) {
        let Token::Word(word) = token else {
            continue;
        };
        let sentence = matches!(
            tokens[at - 1],
            Token::Newline | Token::Mark('.' | ':' | '\u{2022}' | '"')
        );
        let clause = sentence || tokens[at - 1] == Token::Mark(',');
        let listed = matches!(word.as_str(), "gets" | "gains" | "has" | "loses");
        if (word == "deals" && clause) || (listed && sentence) {
            found.push(("verb without subject", BROKEN, at));
        }
    }
}

/// Short wordings Oracle always takes: `you gain 3 life`, `enters or attacks`,
/// `a creature of their choice`, `has` where nothing wears off.
fn plain_phrasing(tokens: &[Token], found: &mut Found) {
    for (at, token) in tokens.iter().enumerate() {
        let Token::Word(word) = token else {
            continue;
        };
        let opens = at == 0 || clause_start(tokens, at) == at;
        let life =
            matches!(tokens.get(at + 1), Some(Token::Number(_))) && is(tokens, at + 2, "life");
        if opens && life && (word == "gain" || word == "lose") {
            found.push(("life without you", BROKEN, at));
        }
        if opens && word == "have" {
            found.push(("instruction opens with have", BROKEN, at));
        }
        // A selection across permanent kinds uses `target artifacts and/or
        // enchantments`; a sweep over all of them keeps ordinary `and`.
        let permanent = |at| {
            matches!(tokens.get(at), Some(Token::Word(w))
            if ["artifacts", "creatures", "enchantments", "lands", "planeswalkers", "permanents"].contains(&w.as_str()))
        };
        if word == "and"
            && at > 0
            && permanent(at - 1)
            && permanent(at + 1)
            && (clause_start(tokens, at)..at)
                .rev()
                .find(|&i| is(tokens, i, "target") || is(tokens, i, "among"))
                .is_some_and(|i| is(tokens, i, "target"))
        {
            found.push(("target kind union", SOFTLY, at));
        }
        // A numeric amount takes the short life phrase: `twice X life`.
        // Nominal expressions still need `life equal to twice the number ...`.
        if word == "life"
            && is(tokens, at + 1, "equal")
            && is(tokens, at + 2, "to")
            && is(tokens, at + 3, "twice")
            && (is(tokens, at + 4, "X") || matches!(tokens.get(at + 4), Some(Token::Number(_))))
        {
            found.push(("numeric life phrase", SOFTLY, at));
        }
        // Introduce a target before using its stat as a filter's operand. This
        // keeps `that creature's power` distinct from the candidate's `its power`.
        if word == "the"
            && (is(tokens, at + 1, "power") || is(tokens, at + 1, "toughness"))
            && is(tokens, at + 2, "of")
            && is(tokens, at + 3, "target")
            && (clause_start(tokens, at)..at).any(|i| {
                is(tokens, i, "with")
                    && (is(tokens, i + 1, "power") || is(tokens, i + 1, "toughness"))
            })
        {
            found.push(("target stat in filter", SOFTLY, at));
        }
        if word == "and" && is(tokens, at + 1, "whenever") && names_self(tokens, at + 2) {
            found.push(("and whenever self", BROKEN, at));
        }
        if word == "sacrifices"
            && is(tokens, at + 1, "a")
            && is(tokens, at + 2, "creature")
            && matches!(tokens.get(at + 3), Some(Token::Mark('.')))
        {
            found.push(("of their choice", BROKEN, at));
        }
    }
    for (start, end) in sentences(tokens) {
        let says = |text: &str| (start..end).any(|at| is(tokens, at, text));
        let lasting = [
            "until",
            "turn",
            "perpetually",
            "as",
            "if",
            "when",
            "whenever",
            "at",
        ];
        if says("gains") && !lasting.iter().any(|word| says(word)) {
            found.push(("gains for good", SOFTLY, start));
        }
    }
}

#[derive(Default)]
pub struct Model {
    ids: HashMap<String, u32>,
    three: HashMap<(u32, u32, u32), u32>,
    /// Trigrams by their first two tokens.
    leading: HashMap<(u32, u32), u32>,
    /// Trigrams by their last two tokens.
    pairs: HashMap<(u32, u32), u32>,
    /// Trigrams by their middle token.
    middle: HashMap<u32, u32>,
    /// Trigrams by their last token.
    last: HashMap<u32, u32>,
    total: f32,
    /// What printing Oracle's cards taught about each trigram, on top of its likelihood.
    learned: HashMap<(u32, u32, u32), f32>,
}

impl Model {
    pub fn parse(table: &str) -> Self {
        let mut model = Self::default();
        for line in table.lines() {
            let mut fields = line.split(' ');
            let (Some(a), Some(b), Some(c), Some(count)) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let Ok(count) = count.parse::<u32>() else {
                continue;
            };
            let (a, b, c) = (model.id(a), model.id(b), model.id(c));
            model.three.insert((a, b, c), count);
            *model.leading.entry((a, b)).or_default() += count;
            *model.pairs.entry((b, c)).or_default() += count;
            *model.middle.entry(b).or_default() += count;
            *model.last.entry(c).or_default() += count;
            model.total += count as f32;
        }
        model
    }

    /// Replace what was learned with a table of `a b c weight` lines.
    pub fn learn(&mut self, table: &str) {
        self.learned.clear();
        for line in table.lines() {
            let mut fields = line.split(' ');
            let (Some(a), Some(b), Some(c), Some(weight)) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let Ok(weight) = weight.parse::<f32>() else {
                continue;
            };
            let gram = (self.id(a), self.id(b), self.id(c));
            self.learned.insert(gram, weight);
        }
    }

    fn correction(&self, a: Option<u32>, b: Option<u32>, c: Option<u32>) -> f32 {
        let (Some(a), Some(b), Some(c)) = (a, b, c) else {
            return 0.0;
        };
        self.learned.get(&(a, b, c)).copied().unwrap_or(0.0)
    }

    /// What [`key_of`] gives, looked up without building it.
    fn token_id(&self, token: &Token) -> Option<u32> {
        let mut mark = [0; 4];
        let key = match token {
            Token::Word(word) => word.as_str(),
            Token::Number(_) | Token::Grouped(_) => "#",
            Token::Mark(symbol) => symbol.encode_utf8(&mut mark),
            Token::SelfName(_) => "@",
            Token::Newline => LINE,
        };
        self.ids.get(key).copied()
    }

    fn id(&mut self, key: &str) -> u32 {
        let next = u32::try_from(self.ids.len()).unwrap_or(u32::MAX);
        *self.ids.entry(key.to_string()).or_insert(next)
    }

    fn step(&self, a: Option<u32>, b: Option<u32>, c: Option<u32>) -> f32 {
        let Some(c) = c else {
            return UNSEEN;
        };
        if let (Some(a), Some(b)) = (a, b)
            && let (Some(seen), Some(before)) =
                (self.three.get(&(a, b, c)), self.leading.get(&(a, b)))
        {
            return (*seen as f32 / *before as f32).ln();
        }
        if let Some(b) = b
            && let (Some(seen), Some(before)) = (self.pairs.get(&(b, c)), self.middle.get(&b))
        {
            return BACKOFF.ln() + (*seen as f32 / *before as f32).ln();
        }
        match self.last.get(&c) {
            Some(count) => 2.0 * BACKOFF.ln() + (*count as f32 / self.total).ln(),
            None => UNSEEN,
        }
    }

    /// The log likelihood of a run of tokens, given the two before it when there are any.
    pub fn score(&self, tokens: &[Token], at_start: bool) -> f32 {
        let start = at_start.then(|| self.ids.get(LINE).copied()).flatten();
        let (mut a, mut b) = (start, start);
        let mut total = 0.0;
        for token in tokens {
            let c = self.token_id(token);
            total += self.step(a, b, c) + self.correction(a, b, c);
            a = b;
            b = c;
        }
        total
    }
}
