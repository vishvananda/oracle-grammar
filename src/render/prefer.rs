//! `@prefer` between alternatives: by the value's kind, or by whether an event happens to
//! the card once, or by a spelling view proved from current operands.
use crate::grammar::{Alternative, Grammar, Sym};
use serde_json::Value;

/// What `@prefer` gives an alternative: `value(X)` reads the value's kind. `always`
/// prefers a neutral wording when the value cannot name a retained trait.
/// `symbol(X)` prefers a spelling view supplied by the inverse binder. A word for
/// the card is weighed where it refers to the card, by [`super::words::Words`].
pub(super) fn preferred(grammar: &Grammar, alternative: &Alternative, value: &Value) -> u32 {
    let [Sym::Rule(rule)] = alternative.syms.as_slice() else {
        return 0;
    };
    crate::print_trace::rule(*rule);
    if let Some(compiled) = grammar.word_table.compiled_prefer.get(*rule as usize) {
        if compiled.is_empty() {
            return 0;
        }
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let sym = value.get("$sym").and_then(Value::as_str);
        let mut once: Option<bool> = None;
        for prefer in compiled {
            let matched = match prefer {
                crate::rtoken::CompiledPrefer::Always => true,
                crate::rtoken::CompiledPrefer::Value(want) => want == kind,
                crate::rtoken::CompiledPrefer::Symbol(want) => sym == Some(want.as_str()),
                crate::rtoken::CompiledPrefer::Once => {
                    *once.get_or_insert_with(|| happens_once(value))
                }
                crate::rtoken::CompiledPrefer::Recurs => {
                    !*once.get_or_insert_with(|| happens_once(value))
                }
            };
            if matched {
                return 1;
            }
        }
        return 0;
    }
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut most = 0;
    for prefer in &grammar.rules[*rule as usize].prefer {
        if prefer == "always"
            || prefer == &format!("value({kind})")
            || value["$sym"]
                .as_str()
                .is_some_and(|name| prefer == &format!("symbol({name})"))
            || (prefer == "once" && happens_once(value))
            || (prefer == "recurs" && !happens_once(value))
        {
            most = most.max(1);
        }
    }
    most
}

/// Events Oracle opens with `When`, because they happen to this object once; the rest
/// take `Whenever`.
fn happens_once(event: &Value) -> bool {
    let Some((kind, inner)) = event.as_object().and_then(|o| o.iter().next()) else {
        return false;
    };
    if kind == "AnyOf" {
        return inner
            .as_array()
            .is_some_and(|cases| cases.iter().all(happens_once));
    }
    let subject = if inner["objects"].is_null() {
        &inner["spells"]
    } else {
        &inner["objects"]
    };
    let source = is_this_object(subject) || inner["players"].is_null() && subject.is_null();
    let attached = subject["kind"] == "SameObject" && subject["args"]["kind"] == "AttachedTo";
    // One object already named, such as a target, leaves its zone once: it becomes a
    // new object (CR 400.7). `When that creature dies this turn`.
    let named = subject["kind"] == "SameObject" && !attached;
    let name = inner["event"].as_str().unwrap_or_default();
    match kind.as_str() {
        "State" => true,
        "ZoneChange" => source || named || attached && inner["to"] == "Graveyard",
        "Spell" | "Object" | "Pair" | "Player" => {
            (source || named && name == "LeavesBattlefield") && ONCE.contains(&name)
                || ONCE_ANYONE.contains(&name)
        }
        _ => false,
    }
}

/// Whether a set of objects is the card itself, however narrowly it is filtered.
fn is_this_object(objects: &Value) -> bool {
    match objects["kind"].as_str() {
        Some("SameObject") => objects["args"]["kind"] == "Source",
        Some("AllOf") => objects["args"]
            .as_array()
            .is_some_and(|parts| parts.iter().any(is_this_object)),
        _ => false,
    }
}

/// Object events that happen to this object once.
const ONCE: [&str; 7] = [
    "Cast",
    "TurnsFaceUp",
    "LeavesBattlefield",
    "Specializes",
    "Exploits",
    "BecomesMonstrous",
    "Transforms",
];
/// Player events worded with the object they belong to.
const ONCE_ANYONE: [&str; 4] = [
    "SetsSchemeInMotion",
    "EncountersPhenomenon",
    "UnlocksDoor",
    "Planeswalks",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::Rule;
    use serde_json::json;

    #[test]
    fn symbolic_preference_requires_the_current_spelling_view() {
        let mut grammar = Grammar::default();
        grammar.rules.push(Rule {
            name: "qualified".into(),
            prefer: vec!["symbol(qualified_object)".into()],
            alternatives: vec![],
            sem: None,
            ty: None,
        });
        let alternative = Alternative {
            syms: vec![Sym::Rule(0)],
            holes: vec![None],
        };
        assert_eq!(
            preferred(
                &grammar,
                &alternative,
                &json!({"$sym":"qualified_object","args":[]})
            ),
            1
        );
        for value in [
            json!({"kind":"Target","args":{"slot":0,"index":0}}),
            json!({"kind":"Query"}),
            json!({"$sym":"it","args":[]}),
        ] {
            assert_eq!(preferred(&grammar, &alternative, &value), 0);
        }
    }
}
