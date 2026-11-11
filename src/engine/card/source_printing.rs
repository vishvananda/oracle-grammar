//! Card surfaces use the shared enumerator, matcher and derivation replay.
use super::{CardInputContext, CardReading, Engine};
use crate::engine::Enumerated;

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
            self.card_surfaces_with_source(reading, context)?,
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
        self.card_context(&context.characteristics)?;

        for cap in [8, 32, 128] {
            let found = self.enumerate_card_with_source(name, &wanted, context, cap, key)?;
            for text in found.texts {
                if self.reads_as_card_with_source(name, &text, &wanted, context) {
                    return Ok(Some(text));
                }
            }
            if !found.cut {
                break;
            }
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
        self.card_context(&context.characteristics)?;
        let (root, surfaces) = (
            self.card_root()?,
            self.card_surfaces_with_source(reading, context)?,
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
            self.card_surfaces_with_source(reading, context)?,
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
