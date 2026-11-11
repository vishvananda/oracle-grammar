//! A fast path past `tabnas-ebnf` for the plainest productions: a name, then alternatives
//! of quoted literals and rule names. Nearly every rule is one, and the general reader
//! spends seconds on them at every start. This accepts nothing else, and hands anything
//! else back, so `tabnas-ebnf` still reads all the syntax that needs reading. A test holds
//! this to `tabnas-ebnf`'s reading of every production in the grammar.
use tabnas_ebnf::{Element, Kind, Production};

fn term(literal: &str) -> Element {
    Element {
        kind: Kind::Term {
            literal: literal.to_string(),
            case_sensitive: None,
            token_name: None,
        },
        sp: None,
    }
}

fn reference(name: &str) -> Element {
    Element {
        kind: Kind::Ref {
            name: name.to_string(),
            debt: None,
        },
        sp: None,
    }
}

fn name_end(bytes: &[u8], from: usize) -> usize {
    let word = |at: &usize| bytes[*at].is_ascii_alphanumeric() || bytes[*at] == b'_';
    (from..bytes.len())
        .find(|at| !word(at))
        .unwrap_or(bytes.len())
}

/// The literal at `from`, and where it ends. A quote doubled to escape it is not plain.
fn literal(text: &str, from: usize) -> Option<(&str, usize)> {
    let quote = text[from..].chars().next()?;
    let close = text[from + 1..].find(quote)? + from + 1;
    let body = &text[from + 1..close];
    let plain = !body.is_empty() && !body.contains(['\\', '\n']);
    let doubled = text[close + 1..].starts_with(quote);
    (plain && !doubled).then_some((body, close + 1))
}

/// The production `text` holds, when it is this plain.
pub fn plain_production(text: &str) -> Option<Production> {
    let bytes = text.as_bytes();
    let end = name_end(bytes, 0);
    let rest = text[end..].trim_start();
    let mut at = text.len() - rest.strip_prefix("::=")?.len();
    let mut alts: Vec<Vec<Element>> = vec![Vec::new()];
    while at < bytes.len() {
        match bytes[at] {
            byte if byte.is_ascii_whitespace() => at += 1,
            b'|' => {
                alts.push(Vec::new());
                at += 1;
            }
            b'"' | b'\'' => {
                let (body, after) = literal(text, at)?;
                alts.last_mut()?.push(term(body));
                at = after;
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let after = name_end(bytes, at);
                alts.last_mut()?.push(reference(&text[at..after]));
                at = after;
            }
            _ => return None,
        }
    }
    if end == 0 || alts.iter().any(Vec::is_empty) {
        return None;
    }
    Some(Production {
        name: text[..end].to_string(),
        alts,
        ..Production::default()
    })
}
