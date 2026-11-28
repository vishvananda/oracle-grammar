//! Card surfaces use the shared enumerator, matcher and derivation replay.
use super::{CardReading, Characteristics, Engine};
use crate::engine::Enumerated;
use crate::engine::spell_paragraphs::can_split_reminded_spell;
use serde_json::Value;

impl Engine {
    pub fn enumerate_card(
        &self,
        name: &str,
        reading: &CardReading,
        context: &Characteristics,
        cap: usize,
        key: &[String],
    ) -> Result<Enumerated, String> {
        if cap == 0 || cap > 4096 {
            return Err("card print candidate limit".into());
        }
        if name.len() > 4096 {
            return Err("card name byte limit".into());
        }
        let types = self.card_context(context)?;
        if reading.notes.is_empty() {
            return self.enumerated(name, &reading.abilities, &types, cap, key);
        }
        self.enumerated_surfaces(
            self.card_root()?,
            &self.card_surfaces(reading, context, !key.is_empty())?,
            name,
            &types,
            cap,
            key,
        )
    }

    pub fn print_card_keyed(
        &self,
        name: &str,
        reading: &CardReading,
        context: &Characteristics,
        key: &[String],
    ) -> Result<Option<String>, String> {
        let wanted = reading.canonical()?;
        if name.len() > 4096 {
            return Err("card name byte limit".into());
        }
        let types = self.card_context(context)?;
        let split_retry = key.is_empty() && can_split_reminded_spell(&wanted.abilities);
        let verify = |text: &str| self.reads_as_card(name, text, &wanted, context);
        if reading.notes.is_empty() {
            let printed = self.print_keyed(name, &reading.abilities, &types, key)?;
            if printed.is_some() || !split_retry {
                return Ok(printed);
            }
        } else {
            let surfaces = self.card_surfaces(&wanted, context, !key.is_empty())?;
            let printed = self.verify_card_surfaces(name, &types, &surfaces, key, verify)?;
            if printed.is_some() || !split_retry {
                return Ok(printed);
            }
        }
        let surfaces = self.card_surfaces(&wanted, context, true)?;
        self.verify_card_surfaces(name, &types, &surfaces, key, verify)
    }

    pub(super) fn verify_card_surfaces(
        &self,
        name: &str,
        types: &[String],
        surfaces: &[Value],
        key: &[String],
        mut verifies: impl FnMut(&str) -> bool,
    ) -> Result<Option<String>, String> {
        let root = self.card_root()?;
        for cap in [8, 32, 128] {
            let found = self.enumerated_surfaces(root, surfaces, name, types, cap, key)?;
            if let Some(text) = found.texts.into_iter().find(|text| verifies(text)) {
                return Ok(Some(text));
            }
            if !found.cut {
                break;
            }
        }
        Ok(None)
    }

    pub fn card_key_of(
        &self,
        name: &str,
        reading: &CardReading,
        context: &Characteristics,
        text: &str,
        ordered: bool,
    ) -> Result<Option<Vec<String>>, String> {
        if name.len() > 4096 || text.len() > 65_536 {
            return Err("card text byte limit".into());
        }
        reading.canonical()?;
        let types = self.card_context(context)?;
        let (root, surfaces) = if reading.notes.is_empty() {
            (self.root, Self::native_surfaces(&reading.abilities, true)?)
        } else {
            (
                self.card_root()?,
                self.card_surfaces(reading, context, true)?,
            )
        };
        let key = self.surface_key_types(root, surfaces, (name, &types), text, ordered)?;
        if key.is_some() || !ordered {
            return Ok(key);
        }
        // Exact default wording needs no derivation choices. The default
        // printer has already verified this text against the current reading.
        let default = self.print_card_keyed(name, reading, context, &[])?;
        Ok((default.as_deref() == Some(text)).then(Vec::new))
    }

    pub fn print_card_trail(
        &self,
        name: &str,
        reading: &CardReading,
        context: &Characteristics,
        trail: &[String],
    ) -> Result<Option<String>, String> {
        if name.len() > 4096 {
            return Err("card name byte limit".into());
        }
        if trail.is_empty() {
            return self.print_card_keyed(name, reading, context, &[]);
        }
        let wanted = reading.canonical()?;
        let types = self.card_context(context)?;
        if reading.notes.is_empty() {
            return self.print_trail_types(name, &types, &reading.abilities, trail);
        }
        Ok(self.replay_first_verified(
            self.card_root()?,
            self.card_surfaces(reading, context, true)?,
            (name, &types),
            trail,
            |text| self.reads_as_card(name, text, &wanted, context),
        ))
    }

    fn reads_as_card(
        &self,
        name: &str,
        text: &str,
        wanted: &CardReading,
        context: &Characteristics,
    ) -> bool {
        self.parse_card(name, text, context)
            .is_ok_and(|parsed| parsed.readings.contains(wanted))
    }
}
