//! Card surfaces use the shared enumerator, matcher and derivation replay.
use super::{CardReading, Characteristics, Engine};
use crate::engine::Enumerated;

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
            self.card_surfaces(reading, context)?,
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
        if reading.notes.is_empty() {
            return self.print_keyed(name, &reading.abilities, &types, key);
        }
        for cap in [8, 32, 128] {
            let found = self.enumerate_card(name, &wanted, context, cap, key)?;
            for text in found.texts {
                if self.reads_as_card(name, &text, &wanted, context) {
                    return Ok(Some(text));
                }
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
        self.card_context(context)?;
        let (root, surfaces) = if reading.notes.is_empty() {
            (self.root, Self::surfaces(&reading.abilities)?)
        } else {
            (self.card_root()?, self.card_surfaces(reading, context)?)
        };
        self.surface_key(root, surfaces, name, text, ordered)
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
        let wanted = reading.canonical()?;
        self.card_context(context)?;
        if reading.notes.is_empty() {
            return self.print_trail(name, &reading.abilities, trail);
        }
        for text in self.replayed_surfaces(
            self.card_root()?,
            self.card_surfaces(reading, context)?,
            name,
            trail,
        )? {
            if self.reads_as_card(name, &text, &wanted, context) {
                return Ok(Some(text));
            }
        }
        Ok(None)
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
