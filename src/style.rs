//! Printing style: how likely a run of tokens is, by the trigrams of Oracle text. One
//! meaning has many texts in the grammar, and this ranks them. It never adds or removes
//! a text, so what prints still reads back through the same rules.
use crate::rtoken::{RToken, WordTable};
use crate::tokens::Token;
use rustc_hash::FxHashMap as HashMap;

mod casting;
mod conditions;
mod mana;
mod owners;
mod quotes;
mod unions;
pub(crate) mod view;
mod where_x;

use view::{
    ALL_FLAGS, BreachSink, CostSum, F_ACTIVATE, F_AND, F_ARTICLE, F_ATTACH, F_CAST, F_COMBAT,
    F_COUNTED_NOUN, F_DAMAGE, F_DRAW, F_EQUAL, F_GAINS, F_HAS_HAVE, F_JOIN_VERB, F_KICKED,
    F_LBRACE, F_LPAREN, F_MAY, F_OWNER, F_PERMANENT, F_PLAIN, F_QUOTE, F_SACRIFICE, F_SLASH,
    F_SPELL, F_SPELLS, F_SPEND, F_SUBJECT_VERB, F_THIS, F_THOUGH, F_UNTIL, F_VERB_AGREE, F_WHERE,
    F_WITH, RTokenSlice, TokenSlice, slice_flags,
};
pub(crate) use view::{F_SELF_NAME, mark_flags, word_flags};

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
    let mut sum = CostSum::default();
    let flags = slice_flags(tokens);
    scan(tokens, flags, context, &mut sum);
    sum.0
}

pub(crate) fn broken_rules_rtokens(
    tokens: &[RToken],
    flags: u64,
    context: &[String],
    table: &WordTable,
    dyn_words: &[String],
) -> f32 {
    if flags == 0 {
        return 0.0;
    }
    let slice = RTokenSlice {
        tokens,
        table,
        dyn_words,
    };
    let mut sum = CostSum::default();
    scan(&slice, flags, context, &mut sum);
    sum.0
}

/// Each rule `tokens` breaks: its name, its cost and where.
pub fn breaches(tokens: &[Token], context: &[String]) -> Vec<(&'static str, f32, usize)> {
    let mut found = Vec::new();
    scan(tokens, ALL_FLAGS, context, &mut found);
    found
}

fn scan<T: TokenSlice + ?Sized, S: BreachSink>(
    tokens: &T,
    flags: u64,
    context: &[String],
    found: &mut S,
) {
    if flags & (F_DRAW | F_DAMAGE | F_SACRIFICE) != 0 {
        sacrificed_and_drew(tokens, found);
    }
    let word_loop_mask = F_COUNTED_NOUN | F_VERB_AGREE | F_MAY | F_THIS | F_ATTACH;
    if flags & word_loop_mask != 0 {
        let do_counted = flags & F_COUNTED_NOUN != 0;
        let do_verb = flags & F_VERB_AGREE != 0;
        let do_may = flags & F_MAY != 0;
        let do_this = flags & F_THIS != 0;
        let do_attach = flags & F_ATTACH != 0;
        let token_mask = word_loop_mask | if do_may { F_HAS_HAVE } else { 0 };
        for at in 0..tokens.len() {
            let tf = tokens.token_flags(at);
            if tf & token_mask == 0 {
                continue;
            }
            let Some(word) = tokens.word(at) else {
                continue;
            };
            if do_counted && tf & F_COUNTED_NOUN != 0 {
                counted_noun(tokens, at, word, found);
            }
            if do_verb && tf & F_VERB_AGREE != 0 {
                verb_agrees(tokens, at, word, found);
            }
            if do_may && tf & (F_MAY | F_HAS_HAVE) != 0 {
                may_takes_a_verb(tokens, at, word, found);
            }
            if do_this && tf & F_THIS != 0 {
                self_noun(tokens, at, word, found);
            }
            if do_attach && tf & F_ATTACH != 0 {
                attachment(tokens, at, word, context, found);
            }
        }
    }
    if flags & F_SLASH != 0 {
        signs_match(tokens, found);
    }
    if (flags & F_UNTIL != 0) && (flags & F_HAS_HAVE != 0) {
        has_for_a_while(tokens, found);
    }
    if flags & (F_THIS | F_SELF_NAME) != 0 {
        self_named_again(tokens, found);
    }
    if (flags & F_AND != 0) && (flags & F_JOIN_VERB != 0) {
        joined_verbs(tokens, found);
    }
    if flags & F_PLAIN != 0 {
        plain_phrasing(tokens, flags, found);
    }
    if flags & F_SUBJECT_VERB != 0 {
        subject_named(tokens, found);
    }
    if (flags & F_WITH != 0) && (flags & F_EQUAL != 0) {
        numeric_stat_phrasing(tokens, found);
    }
    if flags & F_THOUGH != 0 {
        shared_defender_condition(tokens, found);
    }
    if flags & F_LPAREN != 0 {
        reminder_wording(tokens, found);
    }
    if flags & F_KICKED != 0 {
        casting::wording(tokens, context, found);
    }
    if flags & (F_LBRACE | F_SPEND) != 0 || ((flags & F_CAST != 0) && (flags & F_ACTIVATE != 0)) {
        mana::wording(tokens, flags, found);
    }
    if flags & F_OWNER != 0 {
        owners::agreement(tokens, found);
    }
    if flags & F_WHERE != 0 {
        where_x::defined_after_use(tokens, found);
    }
    if flags & F_QUOTE != 0 {
        quotes::one_period(tokens, found);
    }
    if (flags & F_AND != 0) && (flags & F_ARTICLE != 0) {
        unions::singular_or(tokens, found);
    }
    if (flags & F_SPELL != 0) && (flags & F_PERMANENT != 0) {
        unions::spell_kinds(tokens, found);
    }
    if flags & F_COMBAT != 0 {
        conditions::combat_condition_leads(tokens, found);
    }
    if flags & F_SPELLS != 0 {
        conditions::trigger_asks_its_cost(tokens, found);
        conditions::behold_names_its_kind(tokens, found);
    }
}

/// Prefer the common keyword templates; a key retains other printed variants.
fn reminder_wording<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    let len = tokens.len();
    for open in 0..len {
        if !tokens.is_mark(open, '(') || open == 0 {
            continue;
        }
        let bare_source = |at| tokens.is_word(at, "this") && tokens.is_word(at + 1, "deals");
        let deathtouch = tokens.is_word(open - 1, "deathtouch")
            && tokens.is_word(open + 1, "any")
            && tokens.is_word(open + 4, "damage")
            && !bare_source(open + 5);
        let wither = tokens.is_word(open - 1, "wither") && !bare_source(open + 1);
        if deathtouch || wither {
            found.push("reminder source", BROKEN, open);
        }
        if tokens.is_word(open - 1, "haste") {
            let end = (open + 1..len)
                .find(|&at| tokens.is_mark(at, ')'))
                .unwrap_or(len);
            if (open + 1..end).any(|at| {
                tokens.is_word(at, "can")
                    && tokens.is_word(at + 1, "attack")
                    && tokens.is_word(at + 2, "as")
            }) {
                found.push("haste reminder without tap", BROKEN, open);
            }
        }
    }
}

/// A condition shared by characteristic and defender changes precedes their
/// conjunction, keeping its scope clear across both parts of the sentence.
fn shared_defender_condition<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    let len = tokens.len();
    for at in 0..len {
        if !tokens.is_word(at, "can")
            || !tokens.is_word(at + 1, "attack")
            || !tokens.is_word(at + 2, "as")
            || !tokens.is_word(at + 3, "though")
        {
            continue;
        }
        let start = clause_start(tokens, at);
        let joined = (start..at).any(|i| tokens.is_word(i, "and"));
        let characteristic = (start..at).any(|i| {
            ["get", "gets", "gain", "gains", "has", "have"]
                .iter()
                .any(|word| tokens.is_word(i, word))
        });
        let end = (at..len)
            .find(|&i| tokens.is_newline(i) || matches!(tokens.mark(i), Some('.' | '"')))
            .unwrap_or(len);
        if joined
            && characteristic
            && (at + 4..end).any(|i| {
                tokens.is_word(i, "as")
                    && tokens.is_word(i + 1, "long")
                    && tokens.is_word(i + 2, "as")
            })
        {
            found.push("shared defender condition", SOFTLY, at);
        }
    }
}

/// A single numeric bound takes `power 3 or less`, while a compound expression
/// keeps `equal to 1 plus ...`. Both wordings remain available to the grammar.
fn numeric_stat_phrasing<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    for at in 0..tokens.len() {
        if !tokens.is_word(at, "with") {
            continue;
        }
        let start = if tokens.is_word(at + 1, "mana") && tokens.is_word(at + 2, "value") {
            at + 3
        } else if tokens.is_word(at + 1, "power") || tokens.is_word(at + 1, "toughness") {
            at + 2
        } else {
            continue;
        };
        let number = if tokens.is_word(start, "equal") && tokens.is_word(start + 1, "to") {
            start + 2
        } else if (tokens.is_word(start, "less") || tokens.is_word(start, "greater"))
            && tokens.is_word(start + 1, "than")
            && tokens.is_word(start + 2, "or")
            && tokens.is_word(start + 3, "equal")
            && tokens.is_word(start + 4, "to")
        {
            start + 5
        } else {
            continue;
        };
        let numeric = tokens.is_word(number, "X") || tokens.number(number).is_some();
        let compound = tokens.is_word(number + 1, "plus") || tokens.is_word(number + 1, "minus");
        if numeric && !compound {
            found.push("numeric stat phrase", SOFTLY, start);
        }
    }
}

fn names_self<T: TokenSlice + ?Sized>(tokens: &T, at: usize) -> bool {
    tokens.is_word(at, "this") || tokens.is_self_name(at)
}

/// Where a run of words ends going back from `at`: a sentence, line or clause boundary.
pub(super) fn clause_start<T: TokenSlice + ?Sized>(tokens: &T, at: usize) -> usize {
    (0..at)
        .rev()
        .find(|&i| {
            tokens.is_newline(i)
                || matches!(tokens.mark(i), Some('.' | ',' | ':' | '"' | '\u{2022}'))
        })
        .map_or(0, |i| i + 1)
}

/// A cost that sacrifices the card leaves it gone, so the effect calls it `It`. One who
/// draws and loses life is named twice: `You draw a card and you lose 1 life`.
fn sacrificed_and_drew<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    let (mut sacrificed, mut drew) = (false, false);
    for at in 0..tokens.len() {
        if tokens.is_newline(at) {
            (sacrificed, drew) = (false, false);
        } else if tokens.is_mark(at, '.') {
            if drew && tokens.is_word(at + 1, "you") && tokens.is_word(at + 2, "lose") {
                found.push("draw and lose", BROKEN, at);
            }
            drew = false;
        } else if tokens.is_mark(at, ':') {
            if sacrificed && names_self(tokens, at + 1) {
                found.push("sacrificed self", SECONDLY, at);
            }
            sacrificed = false;
        } else if let Some(w) = tokens.word(at) {
            match w {
                "sacrifice" if names_self(tokens, at + 1) => sacrificed = true,
                "draw" => drew = true,
                "and" if drew && tokens.is_word(at + 1, "lose") => {
                    found.push("draw and lose", BROKEN, at);
                }
                "to" if tokens.is_word(at + 1, "them")
                    && at > 0
                    && tokens.is_word(at - 1, "damage") =>
                {
                    found.push("damage to them", SOFTLY, at);
                }
                _ => {}
            }
        }
    }
}

/// Whether a token counts more than one thing, when it counts at all.
fn count_of<T: TokenSlice + ?Sized>(tokens: &T, at: usize) -> Option<bool> {
    // The sides of a `+1/+1` or an `X/X` count nothing.
    let side = at
        .checked_sub(1)
        .is_some_and(|i| matches!(tokens.mark(i), Some('/' | '+' | '-' | '\u{2212}')))
        || tokens.is_mark(at + 1, '/');
    let number = tokens.number(at);
    let sized = number.is_some() || tokens.is_word(at, "X");
    if sized && side {
        return None;
    }
    if let Some(n) = number {
        return Some(n != 1);
    }
    match tokens.word(at)? {
        "a" | "an" | "one" | "another" => Some(false),
        "two" | "three" | "four" | "five" | "six" | "seven" | "eight" | "nine" | "ten" | "X"
        | "many" | "more" | "those" | "these" => Some(true),
        _ => None,
    }
}

/// `two +1/+1 counters`, `a Treasure token`: the noun takes the number of its count.
fn counted_noun<T: TokenSlice + ?Sized, S: BreachSink>(
    tokens: &T,
    at: usize,
    word: &str,
    found: &mut S,
) {
    let plural = match word {
        "counter" | "token" => false,
        "counters" | "tokens" => true,
        _ => return,
    };
    // `counters target spell` is the verb.
    let verb = ["a", "an", "target", "that", "this", "it", "the"];
    if word == "counters" && verb.iter().any(|w| tokens.is_word(at + 1, w)) {
        return;
    }
    for index in (clause_start(tokens, at)..at).rev() {
        if tokens.is_word(index, "of") {
            return;
        }
        if let Some(many) = count_of(tokens, index) {
            if many != plural {
                found.push("counted noun", BROKEN, at);
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
fn plural_subject<T: TokenSlice + ?Sized>(tokens: &T, start: usize, end: usize) -> Option<bool> {
    // A joined subject has no single number, but `base power and toughness 1/1` only
    // qualifies its head.
    let stats = |i: usize| {
        i > start
            && i + 1 < end
            && tokens.is_word(i - 1, "power")
            && tokens.is_word(i + 1, "toughness")
    };
    if (start..end).any(|i| {
        tokens
            .word(i)
            .is_some_and(|w| matches!(w, "and" | "or" | "as" | "if"))
            && !stats(i)
    }) {
        return None;
    }
    let mut head = None;
    for i in start..end {
        if tokens.is_self_name(i) {
            return Some(false);
        }
        if let Some(word) = tokens.word(i) {
            if head.is_none() && word == "that" {
                return None;
            }
            // `Creature you control`, `creature they control`: a new subject ends the head.
            if word == "you" || word == "they" && head.is_some() {
                return head;
            }
            if SINGULAR.contains(&word) {
                head = Some(false);
            } else if PLURAL.contains(&word) {
                head = Some(true);
            } else if head.is_some() {
                return head;
            }
        } else if head.is_some() {
            return head;
        }
    }
    head
}

/// `Creatures you control get`, `Target creature gets`, `up to one target Vehicle becomes`.
fn verb_agrees<T: TokenSlice + ?Sized, S: BreachSink>(
    tokens: &T,
    at: usize,
    word: &str,
    found: &mut S,
) {
    let plural = match word {
        "get" | "gain" | "have" | "lose" | "become" => true,
        "gets" | "gains" | "has" | "loses" | "becomes" => false,
        _ => return,
    };
    // In `causes you to gain` and `as though it didn't have`, the infinitive has no
    // subject agreement.
    let bare = [
        "to", "didn't", "doesn't", "don't", "can", "can't", "may", "must", "would", "will",
    ];
    if plural && at > 0 && bare.iter().any(|word| tokens.is_word(at - 1, word)) {
        return;
    }
    let start = clause_start(tokens, at);
    // `you may have it become …`: a causative `have` takes the bare verb.
    if (start + 1..at).any(|i| tokens.is_word(i - 1, "may") && tokens.is_word(i, "have")) {
        if !plural {
            found.push("verb agrees", BROKEN, at);
        }
        return;
    }
    // `counter you have`: the subject is `you`, whatever came before it.
    let you = at > 0 && (tokens.is_word(at - 1, "you") || tokens.is_word(at - 1, "they"));
    if !you && plural_subject(tokens, start, at).is_some_and(|many| many != plural) {
        found.push("verb agrees", BROKEN, at);
    }
}

/// `May` takes a verb: a subject after it needs a causative `have` (`you may have
/// target creature get`, never `you may target creature gets` or `you may this turn`),
/// and a duration never comes between them (`you may have this turn, …`).
fn may_takes_a_verb<T: TokenSlice + ?Sized, S: BreachSink>(
    tokens: &T,
    at: usize,
    word: &str,
    found: &mut S,
) {
    let openers = [
        "target",
        "another",
        "up",
        "that",
        "it",
        "this",
        "enchanted",
        "equipped",
        "until",
        "the",
        "a",
        "an",
        "its",
        "their",
        "your",
        "other",
    ];
    let subject = word == "may" && openers.iter().any(|next| tokens.is_word(at + 1, next));
    let duration = word == "have"
        && at > 0
        && tokens.is_word(at - 1, "may")
        && (tokens.is_word(at + 1, "until")
            || tokens.is_word(at + 1, "this") && tokens.is_word(at + 2, "turn"));
    if subject || duration {
        found.push("may takes a verb", BROKEN, at);
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
fn self_noun<T: TokenSlice + ?Sized, S: BreachSink>(
    tokens: &T,
    at: usize,
    word: &str,
    found: &mut S,
) {
    if word != "this" || !tokens.word(at + 1).is_some_and(|w| CARD_NOUNS.contains(&w)) {
        return;
    }
    let spent = at >= 3 && tokens.is_word(at - 3, "spent");
    let cast =
        at > 0 && tokens.is_word(at - 1, "cast") && !spent || tokens.is_word(at + 2, "costs");
    let from = tokens.is_word(at + 2, "from") || tokens.is_word(at + 2, "in");
    let zone = from
        && tokens.is_word(at + 3, "your")
        && (tokens.is_word(at + 4, "hand") || tokens.is_word(at + 4, "graveyard"));
    if cast || zone {
        found.push("self off the battlefield", BROKEN, at);
    }
}

/// An Equipment says `equipped creature`, an Aura `enchanted creature`.
fn attachment<T: TokenSlice + ?Sized, S: BreachSink>(
    tokens: &T,
    at: usize,
    word: &str,
    context: &[String],
    found: &mut S,
) {
    let held = |name: &str| context.iter().any(|c| c == name);
    let wrong = (word == "enchanted" && held("Equipment")) || (word == "equipped" && held("Aura"));
    let noun = tokens
        .word(at + 1)
        .is_some_and(|w| w == "creature" || w == "permanent");
    if wrong && noun {
        found.push("attachment", BROKEN, at);
    }
}

/// `-2/-0`: a zero takes the sign beside it.
fn signs_match<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    let sign = |at: usize| match tokens.mark(at) {
        Some(mark @ ('+' | '-' | '\u{2212}')) => Some(mark == '+'),
        _ => None,
    };
    for at in 0..tokens.len() {
        if !tokens.is_mark(at + 2, '/') || tokens.number(at + 1).is_none() {
            continue;
        }
        if let (Some(left), Some(right), Some(0)) = (sign(at), sign(at + 3), tokens.number(at + 4))
            && left != right
        {
            found.push("sign of zero", BROKEN, at);
        }
    }
}

/// `Until end of turn` goes with `gains`: `has` is for what lasts. `May have it gain`
/// and `if it has` are other verbs, and `has base power` is a set value.
fn has_for_a_while<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    for_each_sentence(tokens, |start, end| {
        let until = (start..end).any(|i| {
            i + 3 < end
                && tokens.is_word(i, "until")
                && tokens.is_word(i + 1, "end")
                && tokens.is_word(i + 3, "turn")
        });
        if !until {
            return;
        }
        if (start..end).any(|i| tokens.is_word(i, "if")) {
            return;
        }
        let has = (start..end).find(|&i| {
            (tokens.is_word(i, "has") || tokens.is_word(i, "have"))
                && !(i + 1 < end && tokens.is_word(i + 1, "base"))
                && !(i > start
                    && (tokens.is_word(i - 1, "may")
                        || tokens.is_word(i - 1, "you")
                        || tokens.is_word(i - 1, "it")))
        });
        if let Some(index) = has {
            found.push("has until", BROKEN, index);
        }
    });
}

/// Visit each sentence range `[start..end)`, ending at a period, quote, or line break.
fn for_each_sentence<T: TokenSlice + ?Sized>(tokens: &T, mut visit: impl FnMut(usize, usize)) {
    let len = tokens.len();
    let mut start = 0;
    for at in 0..=len {
        if at == len || tokens.is_newline(at) || matches!(tokens.mark(at), Some('.' | '"')) {
            visit(start, at);
            start = at + 1;
        }
    }
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
fn self_named_again<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    let names = |at: usize| {
        let this = tokens.is_word(at, "this")
            && tokens.word(at + 1).is_some_and(|w| CARD_NOUNS.contains(&w));
        this || tokens.is_self_name(at)
    };
    let between = |at: usize, start: usize| {
        let plain = tokens.word(at).is_some_and(|w| RIVALS.contains(&w));
        let another = tokens.is_word(at, "another") && tokens.word(at + 1).is_some();
        (plain || another) && !(at > start && names(at - 1))
    };
    let subject = |at: usize, start: usize| {
        at == start
            || clause_start(tokens, at) == at
            || tokens.word(at - 1).is_some_and(|w| OPENERS.contains(&w))
    };
    for_each_sentence(tokens, |start, end| {
        let mut named = false;
        let mut rival = false;
        let mut pronoun = ["when", "whenever", "as"]
            .iter()
            .any(|opener| tokens.is_word(start, opener))
            && tokens.is_word(start + 1, "it");
        for at in start..end {
            if names(at) {
                let blocked = at > start
                    && tokens
                        .word(at - 1)
                        .is_some_and(|w| NAMES_AGAIN.contains(&w));
                if named && !rival && !blocked {
                    found.push("self named again", SECONDLY, at);
                }
                if pronoun {
                    found.push("pronoun before name", BROKEN, start + 1);
                    pronoun = false;
                }
                named |= subject(at, start);
                rival = false;
            } else if between(at, start) {
                rival = true;
            }
        }
    });
}

/// `get +1/+0 and gain first strike`: verbs joined by `and` share a subject.
fn joined_verbs<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    let number = |at: usize| match tokens.word(at)? {
        "get" | "gain" | "have" | "lose" => Some(true),
        "gets" | "gains" | "has" | "loses" => Some(false),
        _ => None,
    };
    for_each_sentence(tokens, |start, end| {
        let mut seen = None;
        for at in start..end {
            let Some(many) = number(at) else {
                continue;
            };
            if at > start
                && tokens.is_word(at - 1, "and")
                && seen.is_some_and(|first| first != many)
            {
                found.push("joined verbs", BROKEN, at);
            }
            seen = Some(many);
        }
    });
}

/// A verb of an object names its subject: `This land deals 1 damage to you`. Only a list
/// of what one subject does goes on without it: `gets +1/+1, gains flying`. A phrase
/// ranked alone has its subject outside it, so only a boundary before the verb counts.
fn subject_named<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, found: &mut S) {
    for at in 1..tokens.len() {
        let Some(word) = tokens.word(at) else {
            continue;
        };
        let sentence = tokens.is_newline(at - 1)
            || matches!(tokens.mark(at - 1), Some('.' | ':' | '\u{2022}' | '"'));
        let clause = sentence || tokens.is_mark(at - 1, ',');
        let listed = matches!(word, "gets" | "gains" | "has" | "loses");
        if (word == "deals" && clause) || (listed && sentence) {
            found.push("verb without subject", BROKEN, at);
        }
    }
}

/// Short wordings Oracle always takes: `you gain 3 life`, `enters or attacks`,
/// `a creature of their choice`, `has` where nothing wears off.
fn plain_phrasing<T: TokenSlice + ?Sized, S: BreachSink>(tokens: &T, flags: u64, found: &mut S) {
    let permanent = |at| {
        tokens.word(at).is_some_and(|w| {
            matches!(
                w,
                "artifacts"
                    | "creatures"
                    | "enchantments"
                    | "lands"
                    | "planeswalkers"
                    | "permanents"
            )
        })
    };
    for at in 0..tokens.len() {
        let Some(word) = tokens.word(at) else {
            continue;
        };
        match word {
            "gain" | "lose" => {
                let opens = at == 0 || clause_start(tokens, at) == at;
                if opens && tokens.number(at + 1).is_some() && tokens.is_word(at + 2, "life") {
                    found.push("life without you", BROKEN, at);
                }
            }
            "have" => {
                if at == 0 || clause_start(tokens, at) == at {
                    found.push("instruction opens with have", BROKEN, at);
                }
            }
            "and" => {
                if at > 0
                    && permanent(at - 1)
                    && permanent(at + 1)
                    && (clause_start(tokens, at)..at)
                        .rev()
                        .find(|&i| tokens.is_word(i, "target") || tokens.is_word(i, "among"))
                        .is_some_and(|i| tokens.is_word(i, "target"))
                {
                    found.push("target kind union", SOFTLY, at);
                }
                if tokens.is_word(at + 1, "whenever") && names_self(tokens, at + 2) {
                    found.push("and whenever self", BROKEN, at);
                }
            }
            "life" => {
                if tokens.is_word(at + 1, "equal")
                    && tokens.is_word(at + 2, "to")
                    && tokens.is_word(at + 3, "twice")
                    && (tokens.is_word(at + 4, "X") || tokens.number(at + 4).is_some())
                {
                    found.push("numeric life phrase", SOFTLY, at);
                }
            }
            "the" => {
                if (tokens.is_word(at + 1, "power") || tokens.is_word(at + 1, "toughness"))
                    && tokens.is_word(at + 2, "of")
                    && tokens.is_word(at + 3, "target")
                    && (clause_start(tokens, at)..at).any(|i| {
                        tokens.is_word(i, "with")
                            && (tokens.is_word(i + 1, "power")
                                || tokens.is_word(i + 1, "toughness"))
                    })
                {
                    found.push("target stat in filter", SOFTLY, at);
                }
            }
            "sacrifices"
                if tokens.is_word(at + 1, "a")
                    && tokens.is_word(at + 2, "creature")
                    && tokens.is_mark(at + 3, '.') =>
            {
                found.push("of their choice", BROKEN, at);
            }
            _ => {}
        }
    }
    if flags & F_GAINS != 0 {
        for_each_sentence(tokens, |start, end| {
            let says = |text: &str| (start..end).any(|at| tokens.is_word(at, text));
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
                found.push("gains for good", SOFTLY, start);
            }
        });
    }
}

#[inline]
fn pack_tri(a: u32, b: u32, c: u32) -> u64 {
    ((a as u64) << 42) | ((b as u64) << 21) | (c as u64)
}

#[inline]
fn pack_pair(b: u32, c: u32) -> u64 {
    ((b as u64) << 32) | (c as u64)
}

#[derive(Default)]
pub struct Model {
    ids: HashMap<String, u32>,
    three: HashMap<u64, f32>,
    pairs: HashMap<u64, f32>,
    last: Vec<f32>,
    /// What printing Oracle's cards taught about each trigram, on top of its likelihood.
    learned: HashMap<u64, f32>,
}

impl Model {
    pub fn parse(table: &str) -> Self {
        let mut model = Self::default();
        let (mut three, mut leading, mut pairs, mut middle, mut last) = (
            HashMap::default(),
            HashMap::<_, u32>::default(),
            HashMap::<_, u32>::default(),
            HashMap::<_, u32>::default(),
            HashMap::<_, u32>::default(),
        );
        let mut total = 0.0;
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
            three.insert((a, b, c), count);
            *leading.entry((a, b)).or_default() += count;
            *pairs.entry((b, c)).or_default() += count;
            *middle.entry(b).or_default() += count;
            *last.entry(c).or_default() += count;
            total += count as f32;
        }
        // Freeze the same backoff weights once. Ranking repeatedly looks up these
        // probabilities while exploring alternatives for a partial derivation.
        model.three = three
            .into_iter()
            .map(|((a, b, c), count)| {
                (
                    pack_tri(a, b, c),
                    (count as f32 / leading[&(a, b)] as f32).ln(),
                )
            })
            .collect();
        model.pairs = pairs
            .into_iter()
            .map(|((b, c), count)| {
                (
                    pack_pair(b, c),
                    BACKOFF.ln() + (count as f32 / middle[&b] as f32).ln(),
                )
            })
            .collect();
        model.last = vec![UNSEEN; model.ids.len()];
        for (c, count) in last {
            if let Some(slot) = model.last.get_mut(c as usize) {
                *slot = 2.0 * BACKOFF.ln() + (count as f32 / total).ln();
            }
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
            let gram = pack_tri(self.id(a), self.id(b), self.id(c));
            self.learned.insert(gram, weight);
        }
    }

    #[inline]
    pub(crate) fn id_of_key(&self, key: &str) -> Option<u32> {
        self.ids.get(key).copied()
    }

    #[inline]
    fn correction(&self, a: Option<u32>, b: Option<u32>, c: Option<u32>) -> f32 {
        if self.learned.is_empty() {
            return 0.0;
        }
        let (Some(a), Some(b), Some(c)) = (a, b, c) else {
            return 0.0;
        };
        self.learned.get(&pack_tri(a, b, c)).copied().unwrap_or(0.0)
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

    #[inline]
    fn step(&self, a: Option<u32>, b: Option<u32>, c: Option<u32>) -> f32 {
        let Some(c) = c else {
            return UNSEEN;
        };
        if let (Some(a), Some(b)) = (a, b)
            && let Some(weight) = self.three.get(&pack_tri(a, b, c))
        {
            return *weight;
        }
        if let Some(b) = b
            && let Some(weight) = self.pairs.get(&pack_pair(b, c))
        {
            return *weight;
        }
        self.last.get(c as usize).copied().unwrap_or(UNSEEN)
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

    /// Zero-allocation trigram score over interned [`RToken`]s.
    pub(crate) fn score_rtokens(
        &self,
        tokens: &[RToken],
        table: &WordTable,
        dyn_words: &[String],
        at_start: bool,
    ) -> f32 {
        let start = if at_start {
            if table.style_bound {
                table.line_style_id
            } else {
                self.ids.get(LINE).copied()
            }
        } else {
            None
        };
        let (mut a, mut b) = (start, start);
        let mut total = 0.0;
        for &token in tokens {
            let c = table.style_id(token, dyn_words, self);
            total += self.step(a, b, c) + self.correction(a, b, c);
            a = b;
            b = c;
        }
        total
    }
}
