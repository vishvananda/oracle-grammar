//! The lexical level: text to tokens and back. `grammar/lexical.ebnf` declares the
//! same language. Spacing and sentence capitals are not grammar: a text is
//! accepted only when detokenizing its tokens reproduces it byte for byte.
use rustc_hash::FxHashSet as HashSet;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Token {
    /// A word, in the case a mid-sentence occurrence has.
    Word(String),
    Number(u32),
    /// A number written with thousands separators, as in `1,000 life`.
    Grouped(u32),
    /// One punctuation character, including the braces of a `{T}` symbol.
    Mark(char),
    /// This card's name, as a spelling index into the context's names.
    SelfName(u8),
    Newline,
}

/// What the tokenizer needs from outside the text: the card's names, longest first.
#[derive(Debug, Clone, Default)]
pub struct Names {
    pub names: Vec<String>,
}

impl Names {
    pub fn of(full: &str) -> Self {
        let mut names = vec![full.to_string()];
        if let Some((short, _)) = full.split_once(", ") {
            names.push(short.to_string());
        }
        if let Some((short, _)) = full.split_once(" of ") {
            names.push(short.to_string());
        }
        if let Some((short, _)) = full.split_once(" the ") {
            names.push(short.to_string());
        }
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));
        names.dedup();
        Self { names }
    }

    /// Which spelling a card prints: a legend goes by the part before its comma.
    pub fn printed(&self, context: &[String]) -> u8 {
        let full = self.names.first().map_or("", String::as_str);
        let short = full.split_once(", ").map(|(short, _)| short);
        let legend = context.iter().any(|name| name == "Legendary");
        let index = self
            .names
            .iter()
            .position(|name| legend && Some(name.as_str()) == short);
        index.and_then(|at| u8::try_from(at).ok()).unwrap_or(0)
    }
}

/// An em dash written tight against a word, as in `Ward\u{2014}Pay 2 life.`
pub const TIGHT_DASH: char = '\u{2015}';
/// Before a word capitalized where no sentence begins, as in `{T}, Sacrifice`.
/// It is a token, so the grammar says where this spelling is allowed.
pub const CAPITAL: char = '\u{2191}';
/// Before a word in lower case where a sentence would begin, as in `"bands with other`.
pub const LOWER: char = '\u{2193}';
/// A plus sign tight against the number before it, as in `Station 7+`.
pub const TIGHT_PLUS: char = '\u{207A}';
/// A hyphen tight against the number before it, as in `LEVEL 1-3` or `20-sided`.
pub const TIGHT_HYPHEN: char = '\u{2011}';

const MARKS: &str = ".,:;\"'()+-\u{2212}/\u{2022}\u{2014}{}[]|!?&∞";

/// The possessive of a card's own name, a token apart from the name.
pub const POSSESSIVE: &str = "'s";

fn is_word_byte(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '-' || c == '\u{2019}'
}

/// Is the quote mark that ends `before` an opening one?
fn opens(before: &[Token], mark: char) -> bool {
    before
        .iter()
        .filter(|token| **token == Token::Mark(mark))
        .count()
        % 2
        == 1
}

/// A sentence begins after `before`, so a capital is spelling, not vocabulary.
/// A closing quote or reminder parenthesis ends a sentence when its last
/// token inside is a full stop. Inline parentheticals without one do not.
fn starts_sentence(before: &[Token]) -> bool {
    match before.last() {
        None
        | Some(Token::Newline)
        | Some(Token::Mark('.' | ':' | '\u{2022}' | '\u{2014}' | '\u{2015}' | '(' | '|')) => true,
        Some(Token::Mark(')')) => matches!(
            before.len().checked_sub(2).map(|i| &before[i]),
            Some(Token::Mark('.'))
        ),
        Some(Token::Mark(mark @ ('"' | '\''))) => {
            opens(before, *mark)
                || matches!(
                    before.len().checked_sub(2).map(|i| &before[i]),
                    Some(Token::Mark('.'))
                )
        }
        _ => false,
    }
}

pub(crate) fn lower_first(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect()
    })
}

fn upper_first(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// A card's own name takes no article and no `this`, so `a Wall` on `Wall of Junk` and
/// `this Case` on `Case of the Lost Witness` are types.
fn after_article(previous: Option<&Token>) -> bool {
    matches!(previous, Some(Token::Word(word)) if matches!(word.as_str(), "a" | "an" | "another" | "this"))
}

fn name_at(text: &str, at: usize, names: &Names) -> Option<(u8, usize)> {
    let rest = &text[at..];
    for (index, name) in names.names.iter().enumerate() {
        if !name.is_empty()
            && rest.starts_with(name.as_str())
            && !rest[name.len()..]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric)
        {
            return Some((u8::try_from(index).ok()?, name.len()));
        }
    }
    None
}

/// A short name alone on its line, as `Storm` on `Storm of Memories`, is the
/// keyword it spells: a reference to the card is never a whole line.
fn keyword_line(
    text: &str,
    at: usize,
    length: usize,
    index: u8,
    vocabulary: &HashSet<String>,
) -> bool {
    let alone = (at == 0 || text[..at].ends_with('\n'))
        && (text.len() == at + length || text[at + length..].starts_with('\n'));
    index > 0 && alone && vocabulary.contains(lower_first(&text[at..at + length]).as_str())
}

/// Digits in groups of three after a comma, as in `1,000`, and their length.
/// A comma not followed by exactly three digits is punctuation.
fn grouped(rest: &str) -> Option<(u32, usize)> {
    let bytes = rest.as_bytes();
    let lead = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if !(1..=3).contains(&lead) {
        return None;
    }
    let mut end = lead;
    while bytes.get(end) == Some(&b',')
        && bytes
            .get(end + 1..end + 4)
            .is_some_and(|g| g.iter().all(u8::is_ascii_digit))
        && !bytes.get(end + 4).is_some_and(u8::is_ascii_digit)
    {
        end += 4;
    }
    if end == lead {
        return None;
    }
    let digits: String = rest[..end].chars().filter(char::is_ascii_digit).collect();
    Some((digits.parse().ok()?, end))
}

fn with_separators(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Split text into tokens, or refuse it. `vocabulary` holds the grammar's words,
/// so a sentence-initial `Goblins` keeps its capital and `Draw` loses it.
pub fn tokenize(
    text: &str,
    names: &Names,
    vocabulary: &HashSet<String>,
) -> Result<Vec<Token>, String> {
    let tokens = scan(text, names, vocabulary)?;
    let rendered = detokenize(&tokens, names);
    if rendered == text {
        Ok(tokens)
    } else {
        Err(format!("noncanonical spacing or case: {rendered:?}"))
    }
}

fn scan(text: &str, names: &Names, vocabulary: &HashSet<String>) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::with_capacity(text.len() / 4);
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let c = rest.chars().next().ok_or("empty character")?;
        let word_start = at == 0 || !text[..at].ends_with(char::is_alphanumeric);
        if word_start
            && !after_article(tokens.last())
            && let Some((index, length)) = name_at(text, at, names)
            && !keyword_line(text, at, length, index, vocabulary)
        {
            tokens.push(Token::SelfName(index));
            at += length;
            if text[at..].starts_with(POSSESSIVE) {
                tokens.push(Token::Word(POSSESSIVE.to_string()));
                at += POSSESSIVE.len();
            }
            continue;
        }
        match c {
            ' ' => at += 1,
            '\n' => {
                tokens.push(Token::Newline);
                at += 1;
            }
            _ if c.is_ascii_digit() && grouped(rest).is_some() => {
                let (value, end) = grouped(rest).ok_or("grouped number")?;
                tokens.push(Token::Grouped(value));
                at += end;
            }
            _ if c.is_ascii_digit() => {
                let end = rest
                    .find(|d: char| !d.is_ascii_digit())
                    .unwrap_or(rest.len());
                let tail = rest[end..].chars().next();
                if tail.is_some_and(char::is_alphabetic) {
                    return Err(format!("number glued to a word at {at}"));
                }
                let value = rest[..end].parse().map_err(|_| "number too large")?;
                tokens.push(Token::Number(value));
                at += end;
            }
            _ if c.is_alphabetic() => {
                let end = rest.find(|d| !is_word_byte(d)).unwrap_or(rest.len());
                let word = &rest[..end];
                let lower = lower_first(word);
                let initial = starts_sentence(&tokens);
                let folded = initial && word != lower && vocabulary.contains(lower.as_str());
                let inner = matches!(tokens.last(), Some(Token::Mark(',')))
                    && word != lower
                    && vocabulary.contains(lower.as_str());
                if inner {
                    tokens.push(Token::Mark(CAPITAL));
                } else if initial && word == lower && word != upper_first(word) {
                    tokens.push(Token::Mark(LOWER));
                }
                tokens.push(Token::Word(if folded || inner {
                    lower
                } else {
                    word.to_string()
                }));
                at += end;
            }
            '\u{2014}' if at > 0 && !text[..at].ends_with([' ', '\n']) => {
                tokens.push(Token::Mark(TIGHT_DASH));
                at += c.len_utf8();
            }
            '+' if text[..at].ends_with(|d: char| d.is_ascii_digit()) => {
                tokens.push(Token::Mark(TIGHT_PLUS));
                at += c.len_utf8();
            }
            '-' if text[..at].ends_with(|d: char| d.is_ascii_digit()) => {
                tokens.push(Token::Mark(TIGHT_HYPHEN));
                at += c.len_utf8();
            }
            _ if MARKS.contains(c) => {
                tokens.push(Token::Mark(c));
                at += c.len_utf8();
            }
            _ => return Err(format!("unexpected character {c:?} at {at}")),
        }
    }
    Ok(tokens)
}

/// English spelling the grammar leaves open: `a` agrees with the next word's sound.
pub fn agree_articles(tokens: &[Token]) -> Vec<Token> {
    let vowel_sound = |word: &str| {
        word.starts_with(['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U'])
            && !word.starts_with("one")
            && !word.starts_with("Uni")
    };
    tokens
        .iter()
        .enumerate()
        .map(|(index, token)| match (token, tokens.get(index + 1)) {
            (Token::Word(word), Some(Token::Word(next))) if word == "a" && vowel_sound(next) => {
                Token::Word("an".to_string())
            }
            _ => token.clone(),
        })
        .collect()
}

fn glue_left(token: &Token) -> bool {
    matches!(
        token,
        Token::Mark('.' | ',' | ':' | ';' | ')' | '!' | '?' | TIGHT_PLUS | TIGHT_HYPHEN)
    ) || matches!(token, Token::Word(word) if word == POSSESSIVE)
}

/// Writes tokens with Oracle's spacing and sentence capitals.
pub fn detokenize(tokens: &[Token], names: &Names) -> String {
    let mut out = String::new();
    let mut quotes = Quotes::default();
    for (index, token) in tokens.iter().enumerate() {
        let previous = index.checked_sub(1).map(|i| &tokens[i]);
        if needs_space(previous, token, quotes) {
            out.push(' ');
        }
        match token {
            Token::Mark(CAPITAL | LOWER) => {}
            Token::Word(word)
                if starts_sentence(&tokens[..index])
                    || matches!(previous, Some(Token::Mark(CAPITAL))) =>
            {
                out.push_str(&upper_first(word));
            }
            Token::Word(word) => out.push_str(word),
            Token::Number(value) => out.push_str(&value.to_string()),
            Token::Grouped(value) => out.push_str(&with_separators(*value)),
            Token::Mark(mark) => {
                quotes.pass(*mark);
                out.push(spelled(*mark));
            }
            Token::SelfName(index) => {
                if let Some(name) = names.names.get(usize::from(*index)) {
                    out.push_str(name);
                }
            }
            Token::Newline => out.push('\n'),
        }
    }
    out
}

/// The character a mark prints as.
fn spelled(mark: char) -> char {
    match mark {
        TIGHT_DASH => '\u{2014}',
        TIGHT_PLUS => '+',
        TIGHT_HYPHEN => '-',
        other => other,
    }
}

/// Which kinds of quotation are open.
#[derive(Clone, Copy, Default)]
struct Quotes {
    double: bool,
    single: bool,
}

impl Quotes {
    fn pass(&mut self, mark: char) {
        match mark {
            '"' => self.double = !self.double,
            '\'' => self.single = !self.single,
            _ => {}
        }
    }
}

fn needs_space(previous: Option<&Token>, token: &Token, quotes: Quotes) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    if matches!(previous, Token::Newline) || matches!(token, Token::Newline) || glue_left(token) {
        return false;
    }
    match (previous, token) {
        (Token::Mark(CAPITAL | LOWER), _) => false,
        (Token::Mark('}'), Token::Mark('{')) => false,
        // `Spree` modes: `+ {1} \u{2014}`.
        (Token::Mark('+'), Token::Mark('{')) => true,
        (
            Token::Mark('(' | '+' | '-' | '\u{2212}' | '/' | '{' | '[' | TIGHT_DASH | TIGHT_HYPHEN),
            _,
        ) => false,
        (_, Token::Mark('/' | '}' | ']' | TIGHT_DASH)) => false,
        // An opening quote takes the next token; a closing one follows the last.
        (Token::Mark('"'), _) => !quotes.double,
        (_, Token::Mark('"')) => !quotes.double,
        (Token::Mark('\''), _) => !quotes.single,
        (_, Token::Mark('\'')) => !quotes.single,
        _ => true,
    }
}
