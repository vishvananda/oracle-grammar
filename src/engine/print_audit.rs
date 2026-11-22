//! Audit the same enumerated texts while sharing an already verified default print.
use super::{Engine, canonical_abilities};
use crate::render::FaceMemo;
use serde_json::Value;

type RoundTrip = (u64, Vec<String>);

impl Engine {
    /// Count every enumerated text and report the ones that fail to reparse to the same IR.
    pub fn round_trip(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        cap: usize,
    ) -> Result<RoundTrip, String> {
        self.check_round_trip(None, name, abilities, types, cap, None)
    }

    /// Verify the default print and the round-trip candidates using the same engine and IR.
    #[cfg(feature = "cli")]
    pub fn audit_default_print(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        cap: usize,
    ) -> Result<(Option<String>, RoundTrip), String> {
        let mut memo = FaceMemo::new();
        self.audit_default_print_memo(&mut memo, name, abilities, types, cap)
    }

    /// [`Self::audit_default_print`] reusing a per-face memo across default,
    /// round-trip, and subsequent keyed prints of the same face.
    #[cfg(feature = "cli")]
    pub fn audit_default_print_memo(
        &self,
        memo: &mut FaceMemo,
        name: &str,
        abilities: &Value,
        types: &[String],
        cap: usize,
    ) -> Result<(Option<String>, RoundTrip), String> {
        let default = self.print_keyed_memo(memo, name, abilities, types, &[])?;
        let round_trip =
            self.check_round_trip(Some(memo), name, abilities, types, cap, default.as_deref())?;
        Ok((default, round_trip))
    }

    fn check_round_trip(
        &self,
        mut memo: Option<&mut FaceMemo>,
        name: &str,
        abilities: &Value,
        types: &[String],
        cap: usize,
        verified: Option<&str>,
    ) -> Result<RoundTrip, String> {
        let wanted = canonical_abilities(abilities)?;
        let texts = self
            .enumerated_memo(memo.as_deref_mut(), name, abilities, types, cap, &[])?
            .texts;
        let count = texts.len() as u64;
        let mut lost = Vec::new();
        for text in texts {
            if Some(text.as_str()) != verified
                && !self.reads_as_memo(memo.as_deref_mut(), (name, types), &text, &wanted)
            {
                lost.push(text);
            }
        }
        Ok((count, lost))
    }
}
