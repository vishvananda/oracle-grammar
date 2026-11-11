//! Corruption candidates and their actual parser outcomes. A word swap can
//! remain legal; accepting only the gold reading is not a strict refusal.
use oracle_ir_grammar::engine::{Engine, Parsed, Refusal};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const JUNK: &str = "florp";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Outcome {
    Lexical,
    NoParse,
    Binding,
    Limit,
    AcceptedGoldOnly,
    AcceptedOther,
}

impl Outcome {
    pub(super) fn refusal(error: &Refusal) -> Self {
        match error {
            Refusal::Lexical(_) => Self::Lexical,
            Refusal::NoParse { .. } => Self::NoParse,
            Refusal::Binding(_) => Self::Binding,
            Refusal::Limit(_) => Self::Limit,
        }
    }
}

fn classify(result: &Result<Parsed, Refusal>, gold: &Value) -> Outcome {
    match result {
        Err(error) => Outcome::refusal(error),
        Ok(parsed) if !parsed.readings.is_empty() && parsed.readings.iter().all(|r| r == gold) => {
            Outcome::AcceptedGoldOnly
        }
        Ok(_) => Outcome::AcceptedOther,
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Tally {
    lexical: u64,
    no_parse: u64,
    binding: u64,
    limit: u64,
    accepted_gold_only: u64,
    accepted_other: u64,
}

impl Tally {
    pub(super) fn record(&mut self, outcome: Outcome) {
        *match outcome {
            Outcome::Lexical => &mut self.lexical,
            Outcome::NoParse => &mut self.no_parse,
            Outcome::Binding => &mut self.binding,
            Outcome::Limit => &mut self.limit,
            Outcome::AcceptedGoldOnly => &mut self.accepted_gold_only,
            Outcome::AcceptedOther => &mut self.accepted_other,
        } += 1;
    }

    fn add(&mut self, other: &Self) {
        self.lexical += other.lexical;
        self.no_parse += other.no_parse;
        self.binding += other.binding;
        self.limit += other.limit;
        self.accepted_gold_only += other.accepted_gold_only;
        self.accepted_other += other.accepted_other;
    }

    fn refused(&self) -> u64 {
        self.lexical + self.no_parse + self.binding + self.limit
    }

    fn tried(&self) -> u64 {
        self.refused() + self.accepted_gold_only + self.accepted_other
    }

    pub(super) fn json(&self) -> Value {
        json!({"tried":self.tried(), "strict_refused":self.refused(),
            "nonlimit_refused":self.refused() - self.limit,
            "no_other_reading":self.refused() + self.accepted_gold_only,
            "lexical":self.lexical, "no_parse":self.no_parse, "binding":self.binding,
            "limit":self.limit, "accepted_gold_only":self.accepted_gold_only,
            "accepted_other":self.accepted_other})
    }
}

pub(super) type Families = BTreeMap<String, Tally>;

pub(super) fn merge(total: &mut Families, part: &Families) {
    for (family, entry) in part {
        total.entry(family.clone()).or_default().add(entry);
    }
}

pub(super) fn summary(families: &Families) -> Value {
    let mut total = Tally::default();
    let rows: BTreeMap<_, _> = families
        .iter()
        .map(|(family, entry)| {
            total.add(entry);
            (family, entry.json())
        })
        .collect();
    json!({"families":rows,"total":total.json()})
}

pub(super) fn report(title: &str, families: &Families, proxy: bool) {
    println!("{title}");
    let mut total = Tally::default();
    for (family, entry) in families {
        total.add(entry);
        report_row(family, entry, proxy);
    }
    report_row("all", &total, proxy);
}

fn report_row(family: &str, entry: &Tally, proxy: bool) {
    let passed = entry.refused() + if proxy { entry.accepted_gold_only } else { 0 };
    println!(
        "  {family:10} {:6} tried {passed:6} passed {:5.1}%  accepted-gold {} accepted-other {} limit {}",
        entry.tried(),
        100.0 * passed as f64 / entry.tried().max(1) as f64,
        entry.accepted_gold_only,
        entry.accepted_other,
        entry.limit
    );
}

/// Preserve the historical generator and indices for comparison. These are
/// candidate negatives, not adjudicated invalid Oracle sentences.
fn candidates(text: &str, index: usize) -> Vec<(&'static str, String)> {
    let words: Vec<&str> = text.split(' ').collect();
    let mut out = Vec::new();
    if words.len() < 3 {
        return out;
    }
    let at = index % (words.len() - 1);
    let mut junk = words.clone();
    junk[at] = JUNK;
    out.push(("junk", junk.join(" ")));
    let mut inserted = words.clone();
    inserted.insert(at, JUNK);
    out.push(("inserted", inserted.join(" ")));
    let mut swapped = words.clone();
    swapped.swap(at, at + 1);
    out.push(("swapped", swapped.join(" ")));
    let cut = words[..words.len() / 2 + 1].join(" ");
    let cut = cut.trim_end_matches(['.', ',']).to_string();
    out.push(("truncated", cut + " the"));
    out
}

#[derive(Serialize)]
pub(super) struct Accepted {
    family: &'static str,
    text: String,
    outcome: Outcome,
    readings: usize,
}

#[derive(Default)]
pub(super) struct Checked {
    pub families: Families,
    pub accepted: Vec<Accepted>,
}

pub(super) fn check(
    engine: &Engine,
    name: &str,
    text: &str,
    gold: &Value,
    index: usize,
    keep: bool,
) -> Checked {
    let mut checked = Checked::default();
    for (family, bad) in candidates(text, index) {
        if bad == text {
            continue;
        }
        let result = engine.parse(name, &bad);
        let outcome = classify(&result, gold);
        checked
            .families
            .entry(family.into())
            .or_default()
            .record(outcome);
        if keep && let Ok(parsed) = result {
            checked.accepted.push(Accepted {
                family,
                text: bad,
                outcome,
                readings: parsed.readings.len(),
            });
        }
    }
    checked
}

#[cfg(test)]
mod tests {
    use super::*;
    use oracle_ir_grammar::parse::Counters;
    use std::sync::OnceLock;

    fn engine() -> &'static Engine {
        static ENGINE: OnceLock<Engine> = OnceLock::new();
        ENGINE.get_or_init(|| Engine::load().unwrap())
    }

    #[test]
    fn every_refusal_kind_is_counted_and_limits_remain_identifiable() {
        let mut tally = Tally::default();
        for error in [
            Refusal::Lexical("word".into()),
            Refusal::NoParse {
                furthest: 1,
                tokens: 2,
            },
            Refusal::Binding("reference".into()),
            Refusal::Limit("work".into()),
        ] {
            tally.record(classify(&Err(error), &Value::Null));
        }
        let report = tally.json();
        assert_eq!(report["strict_refused"], 4);
        assert_eq!(report["nonlimit_refused"], 3);
        assert_eq!(report["limit"], 1);
    }

    #[test]
    fn acceptance_never_counts_as_refusal_even_when_it_preserves_gold() {
        let gold = engine()
            .parse("Test", "Draw a card.")
            .unwrap()
            .readings
            .remove(0);
        let mut tally = Tally::default();
        tally.record(classify(&engine().parse("Test", "Draw one card."), &gold));
        tally.record(classify(&engine().parse("Test", "Draw two cards."), &gold));
        let report = tally.json();
        assert_eq!(report["strict_refused"], 0);
        assert_eq!(report["no_other_reading"], 1);
        assert_eq!(report["accepted_gold_only"], 1);
        assert_eq!(report["accepted_other"], 1);
        let empty = Ok(Parsed {
            readings: vec![],
            counters: Counters::default(),
        });
        assert_eq!(classify(&empty, &gold), Outcome::AcceptedOther);
    }

    #[test]
    fn an_extra_reading_fails_the_proxy_even_when_gold_is_also_present() {
        let text = "Destroy target creature with power less than or equal to the number of artifacts you control.";
        let parsed = engine().parse("Test", text).unwrap();
        assert!(parsed.readings.len() > 1);
        let gold = parsed.readings[0].clone();
        assert_eq!(classify(&Ok(parsed), &gold), Outcome::AcceptedOther);
    }

    #[test]
    fn swapping_adjectives_can_produce_a_valid_neighbor() {
        let text = "Destroy target tapped attacking creature.";
        let gold = engine().parse("Test", text).unwrap().readings.remove(0);
        let bad = candidates(text, 2)
            .into_iter()
            .find(|(family, _)| *family == "swapped")
            .unwrap()
            .1;
        assert_ne!(bad, text);
        assert_eq!(
            classify(&engine().parse("Test", &bad), &gold),
            Outcome::AcceptedGoldOnly
        );
    }
}
