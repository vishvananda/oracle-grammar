//! Printing: from abilities to the texts that read back as them.
use super::{Engine, bind_card, canonical_abilities};
use crate::bind;
use crate::derivation::Node;
use crate::model::canonical;
use crate::render::{Matcher, Renderer, Work};
use crate::replay::Replay;
use crate::tokens::{self, Names};
use serde_json::Value;
use std::rc::Rc;

/// Marks the part of an ordered key that says how the card's name is spelled.
const SPELLING: &str = "@name.";
/// Beam widths to try in turn, as multiples of the cap, until a text reads back.
const WIDTHS: [usize; 2] = [4, 16];
/// How many failed texts of a card are read as text even when their trees say no.
const READ_ANYWAY: usize = 8;
/// The most abilities one line can hold when printing a card apart.
const LONGEST_STRETCH: usize = 4;

/// What one pass of printing found, before any text is checked.
#[derive(Debug, Clone, Default)]
pub struct Enumerated {
    pub texts: Vec<String>,
    /// For each text, the trees that print it.
    pub trees: Vec<Vec<Rc<Node>>>,
    /// Whether a beam dropped anything, so that a wider one could find more.
    pub cut: bool,
    pub work: Work,
}

impl Engine {
    /// Where a change to some abilities stops them printing: the deepest value on the way
    /// to the change that printed before it and does not now. Each is a rule that holds
    /// a literal where it should hold a hole.
    pub fn stopped_at(
        &self,
        before: &Value,
        after: &Value,
        types: &[String],
    ) -> Result<Option<(Value, Vec<String>)>, String> {
        let (old, new) = (Self::surfaces(before)?, Self::surfaces(after)?);
        let mut renderer = Renderer::new(&self.grammar, &self.style, 8, types.to_vec());
        match (old.first(), new.first()) {
            (Some(old), Some(new)) => Ok(stopped_in(&mut renderer, old, new).map(|(old, new)| {
                let rules = renderer.printers(&old);
                let names = rules
                    .iter()
                    .map(|r| self.grammar.rules[*r as usize].name.clone());
                (new, names.collect())
            })),
            _ => Ok(None),
        }
    }

    /// The surface forms of a card's abilities, in the IR's own order: canonical
    /// sorting would lose the order words were written in.
    pub(super) fn surfaces(abilities: &Value) -> Result<Vec<Value>, String> {
        let mut abilities = abilities.clone();
        canonical::reduced(&mut abilities)?;
        let linked_card_view = bind::linked_card_actions::projection(&abilities)?;
        let Some(items) = abilities.as_array() else {
            return Err("abilities must be a list".into());
        };
        let mut surfaces: Vec<Vec<Value>> = vec![Vec::new()];
        for (index, item) in items.iter().enumerate() {
            let mut forms = bind::unbind_ability(item);
            if let Some(view) = bind::target_contrast::projection(item)? {
                for form in bind::unbind_ability(&view) {
                    if forms.len() < 64 && !forms.contains(&form) {
                        forms.push(form);
                    }
                }
            }

            if let Some(view) = linked_card_view.as_ref().and_then(|v| v.get(index)) {
                for form in bind::unbind_ability(view) {
                    if forms.len() < 64 && !forms.contains(&form) {
                        forms.push(form);
                    }
                }
            }
            let mut next = Vec::new();
            for head in &surfaces {
                for form in &forms {
                    if next.len() < 64 {
                        let mut joined = head.clone();
                        joined.push(form.clone());
                        next.push(joined);
                    }
                }
            }
            surfaces = next;
        }
        Ok(surfaces.into_iter().map(Value::Array).collect())
    }

    /// Texts for abilities that reparse to the same IR, in preference order.
    pub fn render(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        cap: usize,
    ) -> Result<Vec<String>, String> {
        self.render_keyed(name, abilities, types, cap, &[])
    }

    /// Print with a card's key: the productions named there win every choice they are in.
    pub fn render_keyed(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        cap: usize,
        key: &[String],
    ) -> Result<Vec<String>, String> {
        self.verified(name, abilities, types, (cap, cap), key)
    }

    /// The text a card prints, which is the best one that reparses to the same IR. Nothing
    /// after it is checked, so this costs one parse when the first candidate is right.
    pub fn print_keyed(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        key: &[String],
    ) -> Result<Option<String>, String> {
        let mut texts = self.verified(name, abilities, types, (8, 1), key)?;
        // A schema's canonical operand order can change which equivalent
        // spellings crowd the beam. Keep the common search cheap, then use
        // the already bounded wider renderer only when no text verifies.
        if texts.is_empty() {
            texts = self.verified(name, abilities, types, (32, 1), key)?;
        }
        Ok(texts.into_iter().next())
    }

    /// Up to `limits.1` verified texts from a beam of `limits.0`.
    fn verified(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        limits: (usize, usize),
        key: &[String],
    ) -> Result<Vec<String>, String> {
        let texts = self.verified_whole(name, abilities, types, limits, key)?;
        if !texts.is_empty() {
            return Ok(texts);
        }
        let apart = self.verified_apart(name, abilities, types, limits.0, key)?;
        if let Some(text) = apart {
            return Ok(vec![text]);
        }
        // Equivalent commutative orders can crowd a beam differently. Preserve
        // the lowered order first, then try the canonical one only on failure.
        // Both attempts still require the printed text to reparse to this IR.
        let canonical = canonical_abilities(abilities)?;
        if canonical == *abilities {
            return Ok(Vec::new());
        }
        let texts = self.verified_whole(name, &canonical, types, limits, key)?;
        if !texts.is_empty() {
            return Ok(texts);
        }
        Ok(self
            .verified_apart(name, &canonical, types, limits.0, key)?
            .into_iter()
            .collect())
    }

    /// `verified` with one beam over the whole card.
    fn verified_whole(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        (cap, most): (usize, usize),
        key: &[String],
    ) -> Result<Vec<String>, String> {
        let wanted = canonical_abilities(abilities)?;
        // Wrong guesses at a reference can fill a narrow beam, so widen it on failure. A
        // wider beam prints most of the same texts, and a text that failed still fails.
        let mut failed = rustc_hash::FxHashSet::default();
        for width in WIDTHS {
            let printed = self.enumerated(name, abilities, types, cap * width, key)?;
            let mut out = Vec::new();
            for (text, trees) in printed.texts.into_iter().zip(&printed.trees) {
                if failed.contains(&text) {
                    continue;
                }
                // A tree says what its text means with no search, so a wrong one costs
                // little. But a text can read some other way than the tree that printed
                // it, so the first few that fail are read anyway. Whatever passes has
                // been read back as a text, either way.
                let worth_reading = failed.len() < READ_ANYWAY || self.means(name, trees, &wanted);
                if worth_reading && self.reads_as(name, &text, &wanted) {
                    out.push(text);
                } else {
                    failed.insert(text);
                }
                if out.len() >= most {
                    break;
                }
            }
            // A beam that dropped nothing has nothing more to find when it is wider.
            if !out.is_empty() || !printed.cut {
                return Ok(out);
            }
        }
        Ok(Vec::new())
    }

    /// Print a card one stretch of abilities at a time. One beam over a whole card has to
    /// hold the product of every ability's spellings, and spellings that mean the same thing
    /// crowd out the one combination that reads back. Apart, each ability searches alone,
    /// and the joined text is still read back whole.
    fn verified_apart(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        cap: usize,
        key: &[String],
    ) -> Result<Option<String>, String> {
        let Some(items) = abilities.as_array().filter(|items| items.len() > 1) else {
            return Ok(None);
        };
        let mut lines = Vec::new();
        let mut at = 0;
        while at < items.len() {
            let longest = (at + LONGEST_STRETCH).min(items.len());
            let mut found = None;
            for end in at + 1..=longest {
                let stretch = Value::Array(items[at..end].to_vec());
                let texts = self.verified_whole(name, &stretch, types, (cap, 1), key)?;
                if let Some(text) = texts.into_iter().next() {
                    found = Some((text, end));
                    break;
                }
            }
            let Some((text, end)) = found else {
                return Ok(None);
            };
            lines.push(text);
            at = end;
        }
        let text = lines.join("\n");
        let wanted = canonical_abilities(abilities)?;
        Ok(self.reads_as(name, &text, &wanted).then_some(text))
    }

    /// The round-trip gate: how many enumerated texts there are, and those that do not
    /// reparse to the same IR. `render` drops those, so it cannot measure this.
    pub fn round_trip(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        cap: usize,
    ) -> Result<(u64, Vec<String>), String> {
        let wanted = canonical_abilities(abilities)?;
        let texts = self.enumerated(name, abilities, types, cap, &[])?.texts;
        let count = texts.len() as u64;
        let lost = texts
            .into_iter()
            .filter(|text| !self.reads_as(name, text, &wanted))
            .collect();
        Ok((count, lost))
    }

    /// Whether one of these trees builds the wanted abilities.
    fn means(&self, name: &str, trees: &[Rc<Node>], wanted: &Value) -> bool {
        trees.iter().any(|tree| {
            tree.fill(&self.grammar, 0)
                .and_then(|value| bind_card(&value, name))
                .is_ok_and(|bound| bound == *wanted)
        })
    }

    fn reads_as(&self, name: &str, text: &str, wanted: &Value) -> bool {
        self.parse(name, text)
            .is_ok_and(|parsed| parsed.readings.contains(wanted))
    }

    /// Every distinct text the grammar prints for these abilities, best first, unchecked.
    pub fn enumerated(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        cap: usize,
        key: &[String],
    ) -> Result<Enumerated, String> {
        self.enumerated_surfaces(self.root, Self::surfaces(abilities)?, name, types, cap, key)
    }

    pub(super) fn enumerated_surfaces(
        &self,
        root: u32,
        surfaces: Vec<Value>,
        name: &str,
        types: &[String],
        cap: usize,
        key: &[String],
    ) -> Result<Enumerated, String> {
        let names = Names::of(name);
        let named: rustc_hash::FxHashSet<(u32, usize)> = key
            .iter()
            .filter_map(|step| {
                let (rule, index) = step.rsplit_once('.')?;
                Some((*self.grammar.names.get(rule)?, index.parse().ok()?))
            })
            .collect();
        let mut printed: Vec<(f32, Vec<tokens::Token>, Option<Rc<Node>>)> = Vec::new();
        let mut renderer = Renderer::new(&self.grammar, &self.style, cap, types.to_vec())
            .with_key(named)
            .with_self_name(names.printed(types));
        for surface in surfaces {
            let found = renderer.render(root, &surface);
            for derivation in found.iter() {
                let tokens = derivation.tokens.items();
                let spelled = tokens::agree_articles(&tokens);
                let weight = renderer.weight(derivation, &tokens);
                let score = weight + self.style.score(&spelled, true);
                printed.push((score, spelled, derivation.node.clone()));
            }
        }
        printed.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut seen: rustc_hash::FxHashMap<String, usize> = rustc_hash::FxHashMap::default();
        let mut texts = Vec::new();
        let mut trees: Vec<Vec<Rc<Node>>> = Vec::new();
        for (_, spelled, tree) in printed {
            let text = tokens::detokenize(&spelled, &names);
            let at = *seen.entry(text).or_insert_with_key(|text| {
                texts.push(text.clone());
                trees.push(Vec::new());
                trees.len() - 1
            });
            trees[at].extend(tree);
        }
        Ok(Enumerated {
            texts,
            trees,
            cut: renderer.truncated,
            work: renderer.work,
        })
    }

    /// The alternatives of one derivation that reaches exactly `text`, or `None` when the
    /// grammar cannot reach it.
    pub fn derivation_of(
        &self,
        name: &str,
        abilities: &Value,
        text: &str,
    ) -> Result<Option<Vec<String>>, String> {
        self.surface_key(self.root, Self::surfaces(abilities)?, name, text, false)
    }

    /// Every production choice, including repeated choices and name spelling.
    pub fn trail_of(
        &self,
        name: &str,
        abilities: &Value,
        text: &str,
    ) -> Result<Option<Vec<String>>, String> {
        self.surface_key(self.root, Self::surfaces(abilities)?, name, text, true)
    }

    pub(super) fn surface_key(
        &self,
        root: u32,
        surfaces: Vec<Value>,
        name: &str,
        text: &str,
        ordered: bool,
    ) -> Result<Option<Vec<String>>, String> {
        let names = Names::of(name);
        let target = tokens::tokenize(text, &names, &self.grammar.vocabulary)?;
        if target.len() > self.limits.tokens {
            return Err("key target token limit".into());
        }
        let mut matcher = Matcher::new(&self.grammar, &target);
        for surface in surfaces {
            if matcher.reaches(root, &surface) {
                let steps = matcher.derivation(root, &surface);
                let mut key: Vec<String> = steps
                    .iter()
                    .map(|(rule, index)| self.step_name(*rule, *index))
                    .collect();
                if ordered {
                    key.extend(target.iter().filter_map(|token| match token {
                        tokens::Token::SelfName(index) => Some(format!("{SPELLING}{index}")),
                        _ => None,
                    }));
                } else {
                    key.sort();
                    key.dedup();
                }
                return Ok(Some(key));
            }
        }
        Ok(None)
    }

    /// The text an ordered key prints for these abilities, if it fits them and reads back.
    pub fn print_trail(
        &self,
        name: &str,
        abilities: &Value,
        trail: &[String],
    ) -> Result<Option<String>, String> {
        let wanted = canonical_abilities(abilities)?;
        for text in self.replayed_surfaces(self.root, Self::surfaces(abilities)?, name, trail)? {
            if self.reads_as(name, &text, &wanted) {
                return Ok(Some(text));
            }
        }
        Ok(None)
    }

    pub(super) fn replayed_surfaces(
        &self,
        root: u32,
        surfaces: Vec<Value>,
        name: &str,
        trail: &[String],
    ) -> Result<Vec<String>, String> {
        let (spelled, choices): (Vec<&String>, Vec<&String>) =
            trail.iter().partition(|step| step.starts_with(SPELLING));
        let steps: Option<Vec<(u32, usize)>> = choices
            .iter()
            .map(|step| {
                let (rule, index) = step.rsplit_once('.')?;
                Some((*self.grammar.names.get(rule)?, index.parse().ok()?))
            })
            .collect();
        let spellings: Option<Vec<u8>> = spelled
            .iter()
            .map(|step| step[SPELLING.len()..].parse().ok())
            .collect();
        let (Some(steps), Some(spellings)) = (steps, spellings) else {
            return Ok(Vec::new());
        };
        let names = Names::of(name);
        let mut texts = Vec::new();
        for surface in surfaces {
            let printed = Replay::run(&self.grammar, root, &surface, &steps, &spellings);
            if let Some(printed) = printed {
                let text = tokens::detokenize(&tokens::agree_articles(&printed), &names);
                if !texts.contains(&text) {
                    texts.push(text);
                }
            }
        }
        Ok(texts)
    }

    /// A derivation's alternatives, only for the rules where it leaves the default print.
    pub fn trim_key(steps: &[String], default: &[String]) -> Vec<String> {
        let rule_of = |step: &String| step.rsplit_once('.').map_or(step.clone(), |p| p.0.into());
        let differs: rustc_hash::FxHashSet<String> = steps
            .iter()
            .filter(|step| !default.contains(step))
            .map(rule_of)
            .collect();
        steps
            .iter()
            .filter(|step| differs.contains(&rule_of(step)))
            .cloned()
            .collect()
    }

    fn step_name(&self, rule: u32, index: usize) -> String {
        format!("{}.{index}", self.grammar.rules[rule as usize].name)
    }

    /// The alternatives a card's default print takes.
    pub fn productions_of(
        &self,
        abilities: &Value,
        types: &[String],
    ) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        let mut renderer = Renderer::new(&self.grammar, &self.style, 8, types.to_vec());
        for surface in Self::surfaces(abilities)? {
            if let Some(best) = renderer.render(self.root, &surface).first() {
                out.extend(
                    best.productions
                        .items()
                        .iter()
                        .map(|(rule, index)| self.step_name(*rule, *index)),
                );
                break;
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    pub fn renders_to(&self, name: &str, abilities: &Value, text: &str) -> Result<bool, String> {
        let names = Names::of(name);
        let target = tokens::tokenize(text, &names, &self.grammar.vocabulary)?;
        let mut matcher = Matcher::new(&self.grammar, &target);
        for surface in Self::surfaces(abilities)? {
            if matcher.reaches(self.root, &surface) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn stopped_in(renderer: &mut Renderer, before: &Value, after: &Value) -> Option<(Value, Value)> {
    if before == after {
        return None;
    }
    let deeper = match (before, after) {
        (Value::Object(old), Value::Object(new)) => old
            .iter()
            .find_map(|(key, part)| stopped_in(renderer, part, new.get(key)?)),
        (Value::Array(old), Value::Array(new)) => old
            .iter()
            .zip(new)
            .find_map(|(old, new)| stopped_in(renderer, old, new)),
        _ => None,
    };
    deeper.or_else(|| {
        (renderer.printable(before) && !renderer.printable(after))
            .then(|| (before.clone(), after.clone()))
    })
}
