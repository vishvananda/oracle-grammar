//! Card surfaces use the shared enumerator, matcher and derivation replay.
use super::{CardInputContext, CardNote, CardReading, Engine, LayoutReminder};
use crate::engine::Enumerated;
use crate::engine::ability_joins::has_joined_events;
use crate::engine::spell_paragraphs::can_split_reminded_spell;

impl Engine {
    pub fn enumerate_card_with_source(
        &self,
        name: &str,
        reading: &CardReading,
        context: &CardInputContext,
        cap: usize,
        key: &[String],
    ) -> Result<Enumerated, String> {
        if cap == 0 || cap > 4096 {
            return Err("card print candidate limit".into());
        }
        if name.len() > 4096 {
            return Err("card name byte limit".into());
        }
        let types = self.card_context(&context.characteristics)?;

        self.enumerated_surfaces(
            self.card_root()?,
            &self.card_surfaces_with_source(reading, context, !key.is_empty())?,
            name,
            &types,
            cap,
            key,
        )
    }

    pub fn print_card_keyed_with_source(
        &self,
        name: &str,
        reading: &CardReading,
        context: &CardInputContext,
        key: &[String],
    ) -> Result<Option<String>, String> {
        let wanted = reading.canonical()?;
        if name.len() > 4096 {
            return Err("card name byte limit".into());
        }
        let types = self.card_context(&context.characteristics)?;
        let verify = |text: &str| self.reads_as_card_with_source(name, text, &wanted, context);
        let independent = wanted.notes.is_empty()
            && wanted
                .abilities
                .as_array()
                .is_some_and(|items| (2..=8).contains(&items.len()));
        let tried_apart = independent
            && key.is_empty()
            && types.iter().any(|ty| ty == "Planeswalker")
            && wanted
                .abilities
                .as_array()
                .is_some_and(|items| items.iter().all(|ability| ability["kind"] == "Activated"));
        if tried_apart
            && let Some(text) =
                self.verified_apart(name, &wanted.abilities, &types, 8, key, None)?
            && verify(&text)
        {
            return Ok(Some(text));
        }
        let surfaces = self.card_surfaces_with_source(&wanted, context, !key.is_empty())?;
        let printed = self.verify_card_surfaces(name, &types, &surfaces, key, verify)?;
        if printed.is_some() || !key.is_empty() {
            return Ok(printed);
        }
        if can_split_reminded_spell(&wanted.abilities) || has_joined_events(&wanted.abilities) {
            // Merged abilities can need their separate printed clauses.
            let surfaces = self.card_surfaces_with_source(&wanted, context, true)?;
            if let Some(text) = self.verify_card_surfaces(name, &types, &surfaces, key, verify)? {
                return Ok(Some(text));
            }
        }
        // Compose the independent layout prefix with a verified ability body.
        if let [CardNote::LayoutReminder(LayoutReminder::OngoingScheme)] = wanted.notes.as_slice()
            && let Some(&root) = self.grammar.names.get("ongoing_scheme_header")
        {
            let layout = serde_json::to_value(LayoutReminder::OngoingScheme)
                .map_err(|error| error.to_string())?;
            let headers = self.enumerated_surfaces(root, &[layout], name, &types, 1, &[])?;
            let body = CardReading {
                abilities: wanted.abilities.clone(),
                notes: Vec::new(),
            };
            if let Some(header) = headers.texts.into_iter().next()
                && let Some(body) = self.print_card_keyed_with_source(name, &body, context, &[])?
            {
                let text = format!("{header}\n{body}");
                if verify(&text) {
                    return Ok(Some(text));
                }
            }
        }
        // Independently printable abilities can exhaust a shared candidate
        // beam. Recheck the combined text with the original source context.
        if independent && !tried_apart {
            let text = self.verified_apart(name, &wanted.abilities, &types, 8, key, None)?;
            return Ok(text.filter(|text| verify(text)));
        }
        Ok(None)
    }

    pub fn card_key_of_with_source(
        &self,
        name: &str,
        reading: &CardReading,
        context: &CardInputContext,
        text: &str,
        ordered: bool,
    ) -> Result<Option<Vec<String>>, String> {
        if name.len() > 4096 || text.len() > 65_536 {
            return Err("card text byte limit".into());
        }
        reading.canonical()?;
        let types = self.card_context(&context.characteristics)?;
        let (root, surfaces) = (
            self.card_root()?,
            self.card_surfaces_with_source(reading, context, true)?,
        );
        let key = self.surface_key_types(root, surfaces, (name, &types), text, ordered)?;
        if key.is_some() || !ordered {
            return Ok(key);
        }
        // Exact default wording needs no derivation choices. The default
        // printer has already verified this text against the current reading.
        let default = self.print_card_keyed_with_source(name, reading, context, &[])?;
        Ok((default.as_deref() == Some(text)).then(Vec::new))
    }

    pub fn print_card_trail_with_source(
        &self,
        name: &str,
        reading: &CardReading,
        context: &CardInputContext,
        trail: &[String],
    ) -> Result<Option<String>, String> {
        if name.len() > 4096 {
            return Err("card name byte limit".into());
        }
        if trail.is_empty() {
            return self.print_card_keyed_with_source(name, reading, context, &[]);
        }
        let wanted = reading.canonical()?;
        let types = self.card_context(&context.characteristics)?;

        Ok(self.replay_first_verified(
            self.card_root()?,
            self.card_surfaces_with_source(reading, context, true)?,
            (name, &types),
            trail,
            |text| self.reads_as_card_with_source(name, text, &wanted, context),
        ))
    }

    fn reads_as_card_with_source(
        &self,
        name: &str,
        text: &str,
        wanted: &CardReading,
        context: &CardInputContext,
    ) -> bool {
        self.parse_card_with_source(name, text, context)
            .is_ok_and(|parsed| parsed.readings.contains(wanted))
    }
}
