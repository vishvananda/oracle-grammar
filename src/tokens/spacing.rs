//! The two noncanonical Oracle gaps accepted by the lexical frontend.

struct Spacing {
    canonical: &'static str,
    written: &'static str,
}

fn site(prefix: &[u8], suffix: &[u8]) -> Option<Spacing> {
    if suffix.starts_with(b" (") && (prefix.ends_with(b".") || prefix.ends_with(b".\"")) {
        Some(Spacing {
            canonical: " (",
            written: "(",
        })
    } else if suffix.starts_with(b"...") && prefix.last().is_some_and(u8::is_ascii_alphanumeric) {
        Some(Spacing {
            canonical: "...",
            written: " ...",
        })
    } else {
        None
    }
}

/// Report the ordinal of each accepted spacing adjustment without copying text.
pub(super) fn matches_sentence_spacing(
    text: &str,
    rendered: &str,
    mut changed: impl FnMut(usize),
) -> bool {
    let mut text = text.as_bytes();
    let rendered = rendered.as_bytes();
    let mut at = 0;
    let mut ordinal = 0;
    while let (Some(&actual), Some(&expected)) = (text.first(), rendered.get(at)) {
        if let Some(gap) = site(&rendered[..at], &rendered[at..]) {
            if let Some(suffix) = text.strip_prefix(gap.written.as_bytes()) {
                changed(ordinal);
                text = suffix;
            } else if let Some(suffix) = text.strip_prefix(gap.canonical.as_bytes()) {
                text = suffix;
            } else {
                return false;
            }
            at += gap.canonical.len();
            ordinal += 1;
        } else if actual == expected {
            text = &text[1..];
            at += 1;
        } else {
            return false;
        }
    }
    text.is_empty() && at == rendered.len()
}

pub(crate) fn spacing_changes(text: &str, rendered: &str) -> Option<Vec<usize>> {
    let mut edits = Vec::new();
    (text == rendered || matches_sentence_spacing(text, rendered, |ordinal| edits.push(ordinal)))
        .then_some(edits)
}

/// Edit only the grammar-approved gap sites of a newly rendered string.
pub(crate) fn restore_spacing(text: String, changes: &[usize]) -> Option<String> {
    if changes.is_empty() {
        return Some(text);
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut rest = changes.iter().copied().peekable();
    let mut previous = 0;
    let sites =
        (0..bytes.len()).filter_map(|at| site(&bytes[..at], &bytes[at..]).map(|gap| (at, gap)));
    for (ordinal, (at, gap)) in sites.enumerate() {
        if rest.next_if_eq(&ordinal).is_none() {
            continue;
        }
        out.push_str(&text[previous..at]);
        out.push_str(gap.written);
        previous = at + gap.canonical.len();
    }
    out.push_str(&text[previous..]);
    rest.next().is_none().then_some(out)
}
