//! Presentation removal is allowlisted by the grammar, never guessed from a dash.
use super::Engine;

pub(super) fn builtin(text: &str) -> String {
    let labels = super::read_module("syntax/labels.ebnf")
        .and_then(|text| crate::grammar::presentation_labels(&text))
        .unwrap_or_default();
    strip(text, &labels)
}

impl Engine {
    /// Rules text according to this engine's current, possibly hot-loaded grammar.
    pub fn presentation_free(&self, text: &str) -> String {
        strip(text, &self.grammar.presentation_labels)
    }
}

fn strip(text: &str, labels: &[String]) -> String {
    let mut lines = Vec::new();
    for line in text.split('\n') {
        let mut kept = String::new();
        let mut depth = 0usize;
        for c in line.chars() {
            match c {
                '(' => depth += 1,
                ')' if depth > 0 => depth -= 1,
                _ if depth == 0 => kept.push(c),
                _ => {}
            }
        }
        let kept = kept.trim_end();
        let kept = match kept.split_once(" — ") {
            Some((label, rest))
                if !rest.is_empty()
                    && labels.iter().any(|known| {
                        // Removing a parenthetical before the dash can leave
                        // a gap after the label, as in "Raid (the Fridge)".
                        let label = label.trim_end();
                        label.eq_ignore_ascii_case(known)
                            || (!label.is_ascii()
                                && !known.is_ascii()
                                && label.to_lowercase() == known.to_lowercase())
                    }) =>
            {
                rest
            }
            _ => kept,
        };
        // A label such as "Throw ..." owns its punctuation. Remove the
        // classified heading before closing gaps left by reminder text.
        let kept = if line.contains('(') {
            kept.replace("  ", " ")
                .replace(" .", ".")
                .replace(" ,", ",")
        } else {
            kept.to_string()
        };
        if !kept.trim().is_empty() {
            lines.push(kept.trim_start().to_string());
        }
    }
    lines.join("\n")
}
