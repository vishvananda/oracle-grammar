//! Printing: from abilities to the texts that read back as them.
use super::{Engine, bind_card, canonical_abilities};
use crate::bind;
use crate::derivation::Node;
use crate::grammar::Sym;
use crate::model::canonical;
use crate::render::{Blame, FaceMemo, Matcher, Renderer, Work};
use crate::replay::Replay;
use crate::tokens::{self, Names};
use serde_json::Value;
use std::rc::Rc;

mod guarded;

/// Marks the part of an ordered key that says how the card's name is spelled.
const SPELLING: &str = "@name.";
/// Marks a number of an ordered key, by its place among the text's numbers, that is
/// written with thousands separators.
const GROUPED: &str = "@grouped.";
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

impl Enumerated {
    fn append(&mut self, other: Self) {
        self.cut |= other.cut;
        self.work.add(&other.work);
        let mut seen: rustc_hash::FxHashMap<_, _> = self
            .texts
            .iter()
            .cloned()
            .enumerate()
            .map(|(at, text)| (text, at))
            .collect();
        for (text, trees) in other.texts.into_iter().zip(other.trees) {
            if let Some(at) = seen.get(&text) {
                self.trees[*at].extend(trees);
            } else {
                seen.insert(text.clone(), self.texts.len());
                self.texts.push(text);
                self.trees.push(trees);
            }
        }
    }
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
        let (old, new) = (
            Self::surfaces(before, false)?,
            Self::surfaces(after, false)?,
        );
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
    /// The forms printing tries for `lowered`. A `written` text, one a key, a trail
    /// or a match follows, may also hold a trigger on either of its events apart,
    /// one trigger per event; the text printed for no written words keeps the
    /// trigger it means.
    pub(super) fn surfaces(lowered: &Value, written: bool) -> Result<Vec<Value>, String> {
        let mut abilities = lowered.clone();
        canonical::reduced(&mut abilities)?;
        let mut surfaces = Self::item_surfaces(&abilities)?;
        // A few joined and split forms follow the per-ability ones, beyond their cap.
        // Statics lowered apart split where they were lowered, then at each cut.
        let mut views: Vec<Value> = super::ability_joins::joined(&abilities)
            .into_iter()
            .collect();
        let subject_views = canonical::event_subject_print_views(&abilities)?;
        views.extend(subject_views.iter().cloned());
        let immediate_views = canonical::immediate_parameter_print_views(&abilities)?;
        views.extend(immediate_views.iter().cloned());
        views.extend(super::ability_joins::lowered_apart(lowered, &abilities));
        views.extend(super::ability_joins::split_statics(&abilities));
        if written {
            views.extend(super::ability_joins::split_events(&abilities));
        }
        for view in views {
            for surface in Self::item_surfaces(&view)?.into_iter().take(8) {
                if !surfaces.contains(&surface) {
                    surfaces.push(surface);
                }
            }
        }
        for view in subject_views.into_iter().chain(immediate_views) {
            if !surfaces.contains(&view) {
                surfaces.push(view);
            }
        }
        Ok(surfaces)
    }

    /// Every combination of each ability's unbound forms, at most 64.
    fn item_surfaces(abilities: &Value) -> Result<Vec<Value>, String> {
        let linked_card_view = bind::linked_card_actions::projection(abilities)?;
        let Some(items) = abilities.as_array() else {
            return Err("abilities must be a list".into());
        };
        let mut surfaces: Vec<Vec<Value>> = vec![Vec::new()];
        for (index, item) in items.iter().enumerate() {
            let mut forms = bind::unbind_ability(item);
            let views = [
                bind::target_contrast::projection(item)?,
                bind::group_contrast::projection(item),
                bind::spell_x::projection(item)?,
                canonical::observed_top::projection(item)?,
            ];
            for view in views.iter().flatten() {
                for form in bind::unbind_ability(view) {
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

    pub(super) fn native_surfaces(abilities: &Value, written: bool) -> Result<Vec<Value>, String> {
        let mut surfaces = Self::surfaces(abilities, written)?;
        // Closed declarations can already carry native slots and result IDs.
        // Their canonical inverse must also see the current native operands,
        // before lexical unbinding replaces those identities with references.
        let native = canonical_abilities(abilities)?;
        for view in canonical::immediate_parameter_print_views(&native)? {
            if !surfaces.contains(&view) {
                surfaces.push(view);
            }
        }
        for view in canonical::event_subject_print_views(&native)? {
            if !surfaces.contains(&view) {
                surfaces.push(view);
            }
        }
        if !surfaces.contains(&native) {
            surfaces.push(native);
        }
        Ok(surfaces)
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
        self.verified(name, abilities, types, (cap, cap), key, None)
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
        let mut memo = FaceMemo::new();
        self.print_keyed_memo(&mut memo, name, abilities, types, key)
    }

    /// [`Self::print_keyed`] reusing a per-face memo across calls for the same face.
    pub fn print_keyed_memo(
        &self,
        memo: &mut FaceMemo,
        name: &str,
        abilities: &Value,
        types: &[String],
        key: &[String],
    ) -> Result<Option<String>, String> {
        let mut texts = self.verified(name, abilities, types, (8, 1), key, Some(memo))?;
        // A schema's canonical operand order can change which equivalent
        // spellings crowd the beam. Keep the common search cheap, then use
        // the already bounded wider renderer only when no text verifies.
        if texts.is_empty() {
            texts = self.verified(name, abilities, types, (32, 1), key, Some(memo))?;
        }
        Ok(texts.into_iter().next())
    }

    /// `print_keyed` with one narrow beam and no fallback: no widening, no per-stretch
    /// search, no canonical retry. A key is a set of productions, so a short key can make
    /// every candidate wrong the same way; this bounds what such a key costs to fail.
    pub fn print_keyed_narrow(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        key: &[String],
    ) -> Result<Option<String>, String> {
        let mut memo = FaceMemo::new();
        self.print_keyed_narrow_memo(&mut memo, name, abilities, types, key)
    }

    /// [`Self::print_keyed_narrow`] reusing a per-face memo across calls for the same face.
    pub fn print_keyed_narrow_memo(
        &self,
        memo: &mut FaceMemo,
        name: &str,
        abilities: &Value,
        types: &[String],
        key: &[String],
    ) -> Result<Option<String>, String> {
        super::validate_abilities(abilities)?;
        let texts = self.verified_whole(
            name,
            abilities,
            types,
            (8, 1),
            (key, &WIDTHS[..1]),
            Some(memo),
        )?;
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
        mut memo: Option<&mut FaceMemo>,
    ) -> Result<Vec<String>, String> {
        super::validate_abilities(abilities)?;
        if limits.1 == 1
            && key.is_empty()
            && let Some(text) =
                self.print_guarded_trigger(abilities, (name, types), memo.as_deref_mut())
        {
            return Ok(vec![text]);
        }
        let texts = self.verified_whole(
            name,
            abilities,
            types,
            limits,
            (key, &WIDTHS),
            memo.as_deref_mut(),
        )?;
        if !texts.is_empty() {
            return Ok(texts);
        }
        let apart =
            self.verified_apart(name, abilities, types, limits.0, key, memo.as_deref_mut())?;
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
        let texts = self.verified_whole(
            name,
            &canonical,
            types,
            limits,
            (key, &WIDTHS),
            memo.as_deref_mut(),
        )?;
        if !texts.is_empty() {
            return Ok(texts);
        }
        Ok(self
            .verified_apart(name, &canonical, types, limits.0, key, memo)?
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
        (key, widths): (&[String], &[usize]),
        mut memo: Option<&mut FaceMemo>,
    ) -> Result<Vec<String>, String> {
        let wanted = canonical_abilities(abilities)?;
        let written = !key.is_empty();
        // Wrong guesses at a reference can fill a narrow beam, so widen it on failure. A
        // wider beam prints most of the same texts, and a text that failed still fails.
        // Keep native inverse views in their own pass: extra guesses must not
        // consume the lexical pass's reparse allowance or hide a verified text.
        for native in [true, false] {
            let mut failed = rustc_hash::FxHashSet::default();
            let surfaces = if native {
                Self::native_surfaces(abilities, written)?
            } else {
                Self::surfaces(abilities, written)?
            };
            for &width in widths {
                let printed = self.enumerated_settings(
                    self.root,
                    &surfaces,
                    (name, types),
                    (cap * width, key, native),
                    memo.as_deref_mut(),
                )?;
                let mut out = Vec::new();
                for (text, trees) in printed.texts.into_iter().zip(&printed.trees) {
                    if failed.contains(&text) {
                        continue;
                    }
                    // A tree says what its text means with no search, so a wrong one costs
                    // little. But a text can read some other way than the tree that printed
                    // it, so the first few that fail are read anyway. Whatever passes has
                    // been read back as a text, either way.
                    let worth_reading =
                        failed.len() < READ_ANYWAY || self.means(name, trees, &wanted);
                    if worth_reading
                        && self.reads_as_memo(memo.as_deref_mut(), (name, types), &text, &wanted)
                    {
                        out.push(text);
                    } else {
                        failed.insert(text);
                    }
                    if out.len() >= most {
                        break;
                    }
                }
                if !out.is_empty() {
                    return Ok(out);
                }
                // Widen only a truncated beam, then try the other view.
                if !printed.cut {
                    break;
                }
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
        mut memo: Option<&mut FaceMemo>,
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
                let texts = self.verified_whole(
                    name,
                    &stretch,
                    types,
                    (cap, 1),
                    (key, &WIDTHS),
                    memo.as_deref_mut(),
                )?;
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
        Ok(self
            .reads_as_memo(memo, (name, types), &text, &wanted)
            .then_some(text))
    }

    /// Whether one of these trees builds the wanted abilities.
    fn means(&self, name: &str, trees: &[Rc<Node>], wanted: &Value) -> bool {
        trees.iter().any(|tree| {
            tree.fill(&self.grammar, 0)
                .and_then(|value| bind_card(&value, name))
                .is_ok_and(|bound| bound == *wanted)
        })
    }

    pub(super) fn reads_as_memo(
        &self,
        memo: Option<&mut FaceMemo>,
        card: (&str, &[String]),
        text: &str,
        wanted: &Value,
    ) -> bool {
        super::diagnostics::parse_with_memo_types(self, memo.map(|m| &mut m.parse), card, text)
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
        self.enumerated_memo(None, name, abilities, types, cap, key)
    }

    pub(super) fn enumerated_memo(
        &self,
        mut memo: Option<&mut FaceMemo>,
        name: &str,
        abilities: &Value,
        types: &[String],
        cap: usize,
        key: &[String],
    ) -> Result<Enumerated, String> {
        super::validate_abilities(abilities)?;
        let mut printed = self.enumerated_view(
            name,
            abilities,
            types,
            (cap, key, false),
            memo.as_deref_mut(),
        )?;
        printed.append(self.enumerated_view(name, abilities, types, (cap, key, true), memo)?);
        Ok(printed)
    }

    fn enumerated_view(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        (cap, key, native): (usize, &[String], bool),
        memo: Option<&mut FaceMemo>,
    ) -> Result<Enumerated, String> {
        let written = !key.is_empty();
        let surfaces = if native {
            Self::native_surfaces(abilities, written)?
        } else {
            Self::surfaces(abilities, written)?
        };
        self.enumerated_settings(
            self.root,
            &surfaces,
            (name, types),
            (cap, key, native),
            memo,
        )
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
        self.enumerated_settings(root, &surfaces, (name, types), (cap, key, true), None)
    }

    fn enumerated_settings(
        &self,
        root: u32,
        surfaces: &[Value],
        (name, types): (&str, &[String]),
        (cap, key, native): (usize, &[String], bool),
        mut memo: Option<&mut FaceMemo>,
    ) -> Result<Enumerated, String> {
        let names = Names::for_card(name, types);
        let named: rustc_hash::FxHashSet<(u32, usize)> = key
            .iter()
            .filter_map(|step| {
                let (rule, index) = step.rsplit_once('.')?;
                Some((*self.grammar.names.get(rule)?, index.parse().ok()?))
            })
            .collect();
        let mut printed: Vec<(f32, Vec<tokens::Token>, Option<Rc<Node>>)> = Vec::new();
        let mut renderer = Renderer::new(&self.grammar, &self.style, cap, types.to_vec())
            .with_literal_inverses(native)
            .with_key(named)
            .with_name(name)
            .with_self_name(names.printed(types));
        let saved_parse = memo
            .as_deref_mut()
            .map(|face| std::mem::take(&mut face.parse));
        if let Some(face) = memo.as_deref_mut() {
            renderer = renderer.with_face_memo(std::mem::take(face));
        }
        for surface in surfaces {
            let found = renderer.render(root, surface);
            for derivation in found.iter() {
                let tokens = renderer.tokens_of(derivation);
                let spelled = tokens::agree_articles(&tokens);
                let weight = renderer.weight(derivation, &tokens);
                let score = weight + self.style.score(&spelled, true);
                printed.push((score, spelled, derivation.node.clone()));
            }
        }
        let cut = renderer.truncated;
        let work = renderer.work;
        if let Some(face) = memo {
            *face = renderer.into_face_memo(saved_parse.unwrap_or_default());
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
            cut,
            work,
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
        self.key_of((name, &[]), abilities, text, false)
    }

    pub fn derivation_of_types(
        &self,
        name: &str,
        types: &[String],
        abilities: &Value,
        text: &str,
    ) -> Result<Option<Vec<String>>, String> {
        self.key_of((name, types), abilities, text, false)
    }

    /// Every production choice, including repeated choices and name spelling.
    pub fn trail_of(
        &self,
        name: &str,
        abilities: &Value,
        text: &str,
    ) -> Result<Option<Vec<String>>, String> {
        self.key_of((name, &[]), abilities, text, true)
    }

    pub fn trail_of_types(
        &self,
        name: &str,
        types: &[String],
        abilities: &Value,
        text: &str,
    ) -> Result<Option<Vec<String>>, String> {
        self.key_of((name, types), abilities, text, true)
    }

    /// The key along which `text` prints from the abilities as given, else from
    /// their canonical form. An equivalent lowered form can unbind into
    /// surfaces that miss the written wording; printing makes the same retry.
    fn key_of(
        &self,
        card: (&str, &[String]),
        abilities: &Value,
        text: &str,
        ordered: bool,
    ) -> Result<Option<Vec<String>>, String> {
        super::validate_abilities(abilities)?;
        let surfaces = Self::native_surfaces(abilities, true)?;
        if let Some(key) = self.surface_key_types(self.root, surfaces, card, text, ordered)? {
            return Ok(Some(key));
        }
        let canonical = canonical_abilities(abilities)?;
        if canonical == *abilities {
            return Ok(None);
        }
        let surfaces = Self::native_surfaces(&canonical, true)?;
        self.surface_key_types(self.root, surfaces, card, text, ordered)
    }

    pub(super) fn surface_key(
        &self,
        root: u32,
        surfaces: Vec<Value>,
        name: &str,
        text: &str,
        ordered: bool,
    ) -> Result<Option<Vec<String>>, String> {
        self.surface_key_types(root, surfaces, (name, &[]), text, ordered)
    }

    pub(super) fn surface_key_types(
        &self,
        root: u32,
        surfaces: Vec<Value>,
        (name, types): (&str, &[String]),
        text: &str,
        ordered: bool,
    ) -> Result<Option<Vec<String>>, String> {
        let names = Names::for_card(name, types);
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
                    key.extend(
                        target
                            .iter()
                            .filter(|token| {
                                matches!(
                                    token,
                                    tokens::Token::Number(_) | tokens::Token::Grouped(_)
                                )
                            })
                            .enumerate()
                            .filter(|(_, token)| matches!(token, tokens::Token::Grouped(_)))
                            .map(|(place, _)| format!("{GROUPED}{place}")),
                    );
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
        self.print_trail_types(name, &[], abilities, trail)
    }

    pub fn print_trail_types(
        &self,
        name: &str,
        types: &[String],
        abilities: &Value,
        trail: &[String],
    ) -> Result<Option<String>, String> {
        super::validate_abilities(abilities)?;
        let wanted = canonical_abilities(abilities)?;
        let Some(trail) = PrintTrail::decode(self, trail) else {
            return Ok(None);
        };
        let names = Names::for_card(name, types);
        let mut checked = rustc_hash::FxHashSet::default();
        let mut check = |surface: &Value| {
            let text = trail.render(self, self.root, surface, &names)?;
            (checked.insert(text.clone())
                && self.reads_as_memo(None, (name, types), &text, &wanted))
            .then_some(text)
        };
        // A single ability's first unbound form is also the first full surface.
        // Try it before building alternate projections for the same trail.
        if let Some([ability]) = abilities.as_array().map(Vec::as_slice)
            && let Ok(surface) = Self::surface(ability)
            && let Some(text) = check(&Value::Array(vec![surface]))
        {
            return Ok(Some(text));
        }
        // A trail found on the canonical form replays on its surfaces.
        let views = if wanted == *abilities {
            vec![abilities]
        } else {
            vec![abilities, &wanted]
        };
        for view in views {
            if let Some(text) = Self::native_surfaces(view, true)?
                .iter()
                .find_map(&mut check)
            {
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
        self.replayed_surfaces_types(root, surfaces, (name, &[]), trail)
    }

    pub(super) fn replayed_surfaces_types(
        &self,
        root: u32,
        surfaces: Vec<Value>,
        (name, types): (&str, &[String]),
        trail: &[String],
    ) -> Result<Vec<String>, String> {
        let Some(trail) = PrintTrail::decode(self, trail) else {
            return Ok(Vec::new());
        };
        let names = Names::for_card(name, types);
        let mut texts = Vec::new();
        for text in surfaces
            .iter()
            .filter_map(|surface| trail.render(self, root, surface, &names))
        {
            if !texts.contains(&text) {
                texts.push(text);
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
        name: &str,
        abilities: &Value,
        types: &[String],
    ) -> Result<Vec<String>, String> {
        let mut memo = FaceMemo::new();
        self.productions_of_memo(&mut memo, name, abilities, types)
    }

    /// [`Self::productions_of`] reusing a per-face memo.
    pub fn productions_of_memo(
        &self,
        memo: &mut FaceMemo,
        name: &str,
        abilities: &Value,
        types: &[String],
    ) -> Result<Vec<String>, String> {
        super::validate_abilities(abilities)?;
        let mut out = Vec::new();
        let saved_parse = std::mem::take(&mut memo.parse);
        let mut renderer = Renderer::new(&self.grammar, &self.style, 8, types.to_vec())
            .with_name(name)
            .with_self_name(Names::for_card(name, types).printed(types))
            .with_face_memo(std::mem::take(memo));
        for surface in Self::native_surfaces(abilities, false)? {
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
        *memo = renderer.into_face_memo(saved_parse);
        out.sort();
        out.dedup();
        Ok(out)
    }

    /// Where the inverse of these abilities stops short of exactly `text`: the
    /// furthest token any partial derivation reached, the words around it and
    /// the rules that failed there. `None` when the text is reachable.
    pub fn print_frontier(
        &self,
        name: &str,
        abilities: &Value,
        text: &str,
    ) -> Result<Option<Value>, String> {
        super::validate_abilities(abilities)?;
        let names = Names::of(name);
        let target = tokens::tokenize(text, &names, &self.grammar.vocabulary)?;
        if target.len() > self.limits.tokens {
            return Err("frontier target token limit".into());
        }
        let mut matcher = Matcher::new(&self.grammar, &target).with_frontier();
        for surface in Self::native_surfaces(abilities, true)? {
            if matcher.reaches(self.root, &surface) {
                return Ok(None);
            }
        }
        let frontier = matcher.frontier();
        let at = frontier.at.min(target.len());
        let window = |from: usize, to: usize| tokens::detokenize(&target[from..to], &names);
        let blamed: Vec<String> = frontier
            .blamed
            .iter()
            .map(|(rule, blame)| {
                let rule = &self.grammar.rules[*rule as usize];
                match blame {
                    Blame::Unify => format!("{} unify", rule.name),
                    Blame::Symbol {
                        alternative,
                        symbol,
                    } => {
                        let sym = match rule.alternatives[*alternative].syms.get(*symbol) {
                            Some(Sym::Rule(child)) => {
                                self.grammar.rules[*child as usize].name.clone()
                            }
                            Some(other) => format!("{other:?}"),
                            None => "end".into(),
                        };
                        format!("{}.{alternative} {sym}", rule.name)
                    }
                }
            })
            .collect();
        Ok(Some(serde_json::json!({
            "at": at,
            "before": window(at.saturating_sub(8), at),
            "after": window(at, (at + 4).min(target.len())),
            "blamed": blamed,
        })))
    }

    pub fn renders_to(&self, name: &str, abilities: &Value, text: &str) -> Result<bool, String> {
        super::validate_abilities(abilities)?;
        let names = Names::of(name);
        let target = tokens::tokenize(text, &names, &self.grammar.vocabulary)?;
        let mut matcher = Matcher::new(&self.grammar, &target);
        for surface in Self::native_surfaces(abilities, true)? {
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

/// Writes the numbers at these places among the printed numbers with separators.
struct PrintTrail {
    steps: Vec<(u32, usize)>,
    spellings: Vec<u8>,
    grouped: Vec<usize>,
}

impl PrintTrail {
    fn decode(engine: &Engine, trail: &[String]) -> Option<Self> {
        let mut plan = Self {
            steps: Vec::new(),
            spellings: Vec::new(),
            grouped: Vec::new(),
        };
        for step in trail {
            if let Some(grouped) = step.strip_prefix(GROUPED) {
                plan.grouped.push(grouped.parse().ok()?);
            } else if let Some(spelling) = step.strip_prefix(SPELLING) {
                plan.spellings.push(spelling.parse().ok()?);
            } else {
                let (rule, index) = step.rsplit_once('.')?;
                plan.steps
                    .push((*engine.grammar.names.get(rule)?, index.parse().ok()?));
            }
        }
        Some(plan)
    }

    fn render(&self, engine: &Engine, root: u32, value: &Value, names: &Names) -> Option<String> {
        let mut printed = Replay::run(&engine.grammar, root, value, &self.steps, &self.spellings)?;
        group_numbers(&mut printed, &self.grouped);
        Some(tokens::detokenize(&tokens::agree_articles(&printed), names))
    }
}

fn group_numbers(printed: &mut [tokens::Token], grouped: &[usize]) {
    if grouped.is_empty() {
        return;
    }
    let numbers = printed
        .iter_mut()
        .filter(|token| matches!(token, tokens::Token::Number(_) | tokens::Token::Grouped(_)));
    for (place, token) in numbers.enumerate() {
        if let tokens::Token::Number(value) = *token
            && grouped.contains(&place)
        {
            *token = tokens::Token::Grouped(value);
        }
    }
}
