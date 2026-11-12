//! Words for the card: the types its words prefer, and the seeded alternatives that call
//! it by its own name.
use crate::grammar::{Alternative, Rule, RuleId, Sym, seeded};
use crate::pattern::{Item, Pattern};
use crate::tokens::lower_first;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use serde_json::Value;

#[derive(Debug, Default)]
pub struct Words {
    /// The types the words for the card prefer, by `@prefer type(X)`.
    types: Vec<String>,
    /// Each word's own types.
    own: HashMap<RuleId, Vec<String>>,
    /// Seeded alternatives that refer to the card and say a name no hand-written rule
    /// says and the value does not carry, with its words: the card is called by its own
    /// name. `Whenever Edgar attacks` names Edgar Markov and no other card.
    named: HashMap<(RuleId, usize), Vec<String>>,
    /// Seeded alternatives that call the card by a pronoun of one character: `on her`
    /// passes over the card's own word, as `it` does.
    pronouns: HashSet<(RuleId, usize)>,
}

impl Words {
    pub fn build(rules: &[Rule]) -> Self {
        let own: HashMap<RuleId, Vec<String>> = (0..)
            .zip(rules)
            .map(|(id, rule)| (id, types(&rule.prefer).map(str::to_string).collect()))
            .filter(|(_, types): &(RuleId, Vec<String>)| !types.is_empty())
            .collect();
        let mut types: Vec<String> = own.values().flatten().cloned().collect();
        types.sort();
        types.dedup();
        let known: HashSet<&str> = rules
            .iter()
            .filter(|rule| !seeded(&rule.name))
            .flat_map(literals)
            .collect();
        let mut named = HashMap::default();
        let mut pronouns = HashSet::default();
        for (id, rule) in (0..).zip(rules) {
            let Some(pattern) = rule.sem.as_ref().filter(|_| seeded(&rule.name)) else {
                continue;
            };
            if !refers_to_card(pattern) {
                continue;
            }
            let mut carried = Vec::new();
            strings(pattern, &mut carried);
            for (index, alternative) in rule.alternatives.iter().enumerate() {
                let words = names(alternative, &known, &carried);
                if !words.is_empty() {
                    named.insert((id, index), words);
                }
                if alternative.syms.iter().any(is_pronoun) {
                    pronouns.insert((id, index));
                }
            }
        }
        Self {
            types,
            own,
            named,
            pronouns,
        }
    }

    /// The card's best word for itself in its context.
    pub fn best(&self, context: &[String]) -> u32 {
        weigh(context, &self.types)
    }

    /// How well a rule names the card in its context, if it is a word for the card.
    pub fn of(&self, rule: RuleId, context: &[String]) -> Option<u32> {
        self.own.get(&rule).map(|types| weigh(context, types))
    }

    /// Whether an alternative calls the card by a pronoun of its own, which names no type.
    pub fn by_pronoun(&self, step: (RuleId, usize)) -> bool {
        self.pronouns.contains(&step)
    }

    /// Whether every name an alternative says is a word of the card's `name`.
    pub fn fit(&self, step: (RuleId, usize), name: &HashSet<String>) -> bool {
        self.named
            .get(&step)
            .is_none_or(|words| words.iter().all(|word| name.contains(word)))
    }
}

/// How well words of these types name the card: the sooner a type in its context, the
/// better.
fn weigh(context: &[String], types: &[String]) -> u32 {
    types
        .iter()
        .filter_map(|name| context.iter().position(|kind| kind == name))
        .map(|at| u32::try_from(context.len() - at).unwrap_or(1))
        .max()
        .unwrap_or(0)
}

/// A card's own types, then what every card is: a spell while it is cast, and a card.
/// A card of unknown types has no best word for itself.
pub fn card_context(mut context: Vec<String>) -> Vec<String> {
    if context.is_empty() {
        return context;
    }
    for kind in ["Spell", "Card"] {
        if !context.iter().any(|name| name == kind) {
            context.push(kind.to_string());
        }
    }
    context
}

/// Whether a value is the card itself: a reference to it, or a group or filter of it alone.
pub fn is_the_card(value: &Value) -> bool {
    match value["kind"].as_str() {
        Some("Source") => true,
        Some("One" | "SameObject") => is_the_card(&value["args"]),
        _ => false,
    }
}

/// The types `@prefer type(...)` names.
fn types(prefer: &[String]) -> impl Iterator<Item = &str> {
    prefer
        .iter()
        .filter_map(|prefer| prefer.strip_prefix("type(")?.strip_suffix(')'))
}

/// The words of a card's name, as a seeded rule spells them.
pub fn name_words(name: &str) -> HashSet<String> {
    name.split([' ', ','])
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// The names an alternative says: capitalized words that no hand-written rule says and
/// the value does not carry. A single capital, like `Y`, is a variable.
fn names(alternative: &Alternative, known: &HashSet<&str>, carried: &[&str]) -> Vec<String> {
    alternative
        .syms
        .iter()
        .filter_map(|sym| match sym {
            Sym::Word(literal) if !known.contains(literal.as_str()) => Some(literal),
            _ => None,
        })
        .flat_map(|literal| literal.split(' '))
        .map(|word| word.strip_suffix("'s").unwrap_or(word))
        .filter(|word| word.starts_with(char::is_uppercase) && word.chars().count() > 1)
        .filter(|word| !known.contains(word) && !known.contains(lower_first(word).as_str()))
        .filter(|word| !carried.iter().any(|text| text.contains(word)))
        .map(str::to_string)
        .collect()
}

/// A pronoun for a character: what a seeded rule calls its own card, and no other.
fn is_pronoun(sym: &Sym) -> bool {
    matches!(sym, Sym::Word(word) if matches!(word.as_str(), "she" | "he" | "her" | "him" | "his" | "herself" | "himself"))
}

fn literals(rule: &Rule) -> impl Iterator<Item = &str> {
    rule.alternatives
        .iter()
        .flat_map(|alternative| &alternative.syms)
        .filter_map(|sym| match sym {
            Sym::Word(word) => Some(word.as_str()),
            _ => None,
        })
}

/// Whether a pattern says the card itself, other than to exclude it as `another` does.
fn refers_to_card(pattern: &Pattern) -> bool {
    match pattern {
        Pattern::Value(value) => names_card(value),
        Pattern::Hole(..) => false,
        Pattern::Object(fields) => {
            let kind = fields.iter().find_map(|(field, pattern)| match pattern {
                Pattern::Value(Value::String(kind)) if field == "kind" => Some(kind.as_str()),
                _ => None,
            });
            match kind {
                Some("Source") => true,
                Some("Not") => false,
                _ => fields.iter().any(|(_, pattern)| refers_to_card(pattern)),
            }
        }
        Pattern::List(items) => items
            .iter()
            .any(|item| matches!(item, Item::One(pattern) if refers_to_card(pattern))),
        Pattern::Symbol(_, args) => args.iter().any(refers_to_card),
    }
}

fn names_card(value: &Value) -> bool {
    match value {
        Value::Object(map) => match map.get("kind").and_then(Value::as_str) {
            Some("Source") => true,
            Some("Not") => false,
            _ => map.values().any(names_card),
        },
        Value::Array(items) => items.iter().any(names_card),
        _ => false,
    }
}

/// The text a pattern's value carries, like a token's name.
fn strings<'p>(pattern: &'p Pattern, found: &mut Vec<&'p str>) {
    fn values<'v>(value: &'v Value, found: &mut Vec<&'v str>) {
        match value {
            Value::String(text) => found.push(text),
            Value::Array(items) => items.iter().for_each(|item| values(item, found)),
            Value::Object(map) => map.values().for_each(|item| values(item, found)),
            _ => {}
        }
    }
    match pattern {
        Pattern::Value(value) => values(value, found),
        Pattern::Hole(..) => {}
        Pattern::Object(fields) => fields.iter().for_each(|(_, field)| strings(field, found)),
        Pattern::List(items) => {
            for item in items {
                if let Item::One(pattern) = item {
                    strings(pattern, found);
                }
            }
        }
        Pattern::Symbol(_, args) => args.iter().for_each(|arg| strings(arg, found)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rule(name: &str, words: &[&str], sem: Value) -> Rule {
        Rule {
            name: name.into(),
            alternatives: vec![Alternative {
                syms: words.iter().map(|word| Sym::Word((*word).into())).collect(),
                holes: vec![None; words.len()],
            }],
            sem: Some(Pattern::Value(sem)),
            ty: None,
            prefer: vec![],
        }
    }

    #[test]
    fn a_seeded_name_for_the_card_fits_only_the_card_it_names() {
        let card = json!({"kind": "SameObject", "args": {"kind": "Source"}});
        let another = json!({"kind": "Not", "args": card});
        let conjure = json!({"kind": "Conjure", "args": {"name": "Mitotic Slime", "to": card}});
        let words = Words::build(&[
            rule("vampire", &["Vampire"], json!({"kind": "Subtype"})),
            rule(
                "words",
                &["whenever", "attacks", "gets", "conjure", "another"],
                json!(null),
            ),
            rule("t_edgar", &["whenever", "Edgar", "attacks"], card.clone()),
            rule("t_zurgo", &["Zurgo", "attacks"], card.clone()),
            rule("t_variable", &["gets", "-", "Y", "Vampire's"], card.clone()),
            rule(
                "t_slime",
                &["conjure", "Mitotic", "Slime's", "Vampire"],
                conjure,
            ),
            rule("t_another", &["another", "Edgar"], another),
        ]);
        let ghitu = name_words("Ghitu Fire-Eater");
        assert!(!words.fit((2, 0), &ghitu));
        assert!(words.fit((2, 0), &name_words("Edgar Markov")));
        assert!(!words.fit((3, 0), &ghitu));
        assert!(words.fit((3, 0), &name_words("Zurgo Bellstriker")));
        for step in [(4, 0), (5, 0), (6, 0)] {
            assert!(words.fit(step, &ghitu), "{step:?}");
        }
    }
}
