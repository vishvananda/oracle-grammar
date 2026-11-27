//! Card surfaces use the shared enumerator, matcher and derivation replay.
use super::{CardInputContext, CardReading, Engine};
use crate::engine::Enumerated;
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
        let surfaces = self.card_surfaces_with_source(&wanted, context, !key.is_empty())?;
        let printed = self.verify_card_surfaces(name, &types, &surfaces, key, verify)?;
        if printed.is_some() || !key.is_empty() || !can_split_reminded_spell(&wanted.abilities) {
            return Ok(printed);
        }
        // A merged spell can need a paragraph break to place its reminder.
        let surfaces = self.card_surfaces_with_source(&wanted, context, true)?;
        self.verify_card_surfaces(name, &types, &surfaces, key, verify)
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
        self.card_context(&context.characteristics)?;
        let (root, surfaces) = (
            self.card_root()?,
            self.card_surfaces_with_source(reading, context, true)?,
        );
        self.surface_key(root, surfaces, name, text, ordered)
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
        let wanted = reading.canonical()?;
        self.card_context(&context.characteristics)?;

        for text in self.replayed_surfaces(
            self.card_root()?,
            self.card_surfaces_with_source(reading, context, true)?,
            name,
            trail,
        )? {
            if self.reads_as_card_with_source(name, &text, &wanted, context) {
                return Ok(Some(text));
            }
        }
        Ok(None)
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
