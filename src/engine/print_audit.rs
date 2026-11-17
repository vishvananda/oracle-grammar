//! Audit the same enumerated texts while sharing an already verified default print.
use super::{Engine, canonical_abilities};
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
        self.check_round_trip(name, abilities, types, cap, None)
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
        let default = self.print_keyed(name, abilities, types, &[])?;
        let round_trip = self.check_round_trip(name, abilities, types, cap, default.as_deref())?;
        Ok((default, round_trip))
    }

    fn check_round_trip(
        &self,
        name: &str,
        abilities: &Value,
        types: &[String],
        cap: usize,
        verified: Option<&str>,
    ) -> Result<RoundTrip, String> {
        let wanted = canonical_abilities(abilities)?;
        let texts = self.enumerated(name, abilities, types, cap, &[])?.texts;
        let count = texts.len() as u64;
        let lost = texts
            .into_iter()
            .filter(|text| Some(text.as_str()) != verified && !self.reads_as(name, text, &wanted))
            .collect();
        Ok((count, lost))
    }
}
