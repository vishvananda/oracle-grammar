//! The Oracle representation of a sticker sheet has no ordinary face.
use super::{Engine, Refusal};
use crate::model::{canonical, ir};

const MAX_STICKERS: usize = 16;
const MAX_TICKETS: u32 = 64;
const MAX_TEXT_BYTES: usize = 65_536;

impl Engine {
    /// Parse the source name stickers and all rules-bearing lines.
    pub fn parse_sticker_sheet(
        &self,
        name: &str,
        text: &str,
    ) -> Result<Vec<ir::UnifiedCard>, Refusal> {
        if text.len() > MAX_TEXT_BYTES || name.len() > 4096 {
            return Err(Refusal::Limit("sticker sheet byte limit".into()));
        }
        let names = sticker_names(name)?;
        let mut selections = vec![
            names
                .into_iter()
                .map(|word| ir::Sticker::Name(word.to_owned()))
                .collect::<Vec<_>>(),
        ];
        if text.is_empty() {
            return Err(Refusal::Lexical("missing sticker rules".into()));
        }
        for (index, line) in text.split('\n').enumerate() {
            if index >= MAX_STICKERS - 3 {
                return Err(Refusal::Limit("sticker sheet row limit".into()));
            }
            let (cost, body) = line.split_once(" — ").ok_or_else(|| {
                Refusal::Lexical(format!(
                    "sticker row {} has no ticket cost delimiter",
                    index + 1
                ))
            })?;
            let tickets = ticket_cost(cost)?;
            let options = self.read_sticker(tickets, body)?;
            if selections.len().saturating_mul(options.len()) > self.reading_limit {
                return Err(Refusal::Limit("sticker sheet reading limit".into()));
            }
            selections = selections
                .into_iter()
                .flat_map(|stickers| {
                    options.iter().map(move |option| {
                        let mut next = stickers.clone();
                        next.push(option.clone());
                        next
                    })
                })
                .collect();
        }
        selections
            .into_iter()
            .map(|stickers| {
                let card = ir::UnifiedCard {
                    schema_version: ir::SCHEMA_VERSION,
                    layout: ir::Layout::StickerSheet,
                    faces: Vec::new(),
                    data: vec![ir::Field {
                        name: "Stickers".into(),
                        value: ir::Operand::Stickers(stickers),
                    }],
                };
                canonical::unified(&card).map_err(Refusal::Binding)
            })
            .collect()
    }

    fn read_sticker(&self, tickets: u32, text: &str) -> Result<Vec<ir::Sticker>, Refusal> {
        if let Some((power, toughness)) = text.split_once('/')
            && let (Ok(power), Ok(toughness)) = (power.parse(), toughness.parse())
        {
            return Ok(vec![ir::Sticker::PowerToughness {
                tickets,
                power,
                toughness,
            }]);
        }
        if text.is_empty() {
            return Err(Refusal::Lexical("empty sticker ability".into()));
        }
        // The sticker gives its ability to the object carrying it.
        self.parse("", text)?
            .readings
            .into_iter()
            .map(|reading| {
                let abilities: Vec<ir::Ability> = serde_json::from_value(reading)
                    .map_err(|error| Refusal::Binding(error.to_string()))?;
                if abilities.is_empty() {
                    return Err(Refusal::Binding("empty sticker ability reading".into()));
                }
                Ok(ir::Sticker::Abilities { tickets, abilities })
            })
            .collect()
    }

    /// Print current sticker operands and verify the full sheet reads back.
    pub fn print_sticker_sheet(
        &self,
        card: &ir::UnifiedCard,
    ) -> Result<Option<(String, String)>, String> {
        self.print_sticker_sheet_with_trail(card, None)
    }

    /// Save grammatical choices for each rules row of the given Oracle sheet.
    /// The result contains no sticker values or printed source text.
    pub fn sticker_sheet_trail_of(
        &self,
        card: &ir::UnifiedCard,
        name: &str,
        text: &str,
    ) -> Result<Option<Vec<Vec<String>>>, String> {
        if text.len() > MAX_TEXT_BYTES || name.len() > 4096 {
            return Err("sticker sheet byte limit".into());
        }
        let card = canonical::unified(card)?;
        let (printed_name, rules) = sheet_parts(&card)?;
        let lines: Vec<_> = text.split('\n').take(MAX_STICKERS + 1).collect();
        if printed_name != name || lines.len() != rules.len() {
            return Ok(None);
        }
        let mut trail = Vec::with_capacity(rules.len());
        for (sticker, line) in rules.iter().zip(lines) {
            let Some((cost, body)) = line.split_once(" — ") else {
                return Ok(None);
            };
            let cost = ticket_cost(cost).ok();
            match sticker {
                ir::Sticker::Abilities { tickets, abilities } if cost == Some(*tickets) => {
                    let abilities = serde_json::to_value(abilities).map_err(|e| e.to_string())?;
                    let Some(row) = self.trail_of("", &abilities, body)? else {
                        return Ok(None);
                    };
                    trail.push(row);
                }
                ir::Sticker::PowerToughness {
                    tickets,
                    power,
                    toughness,
                } if cost == Some(*tickets) && body == format!("{power}/{toughness}") => {
                    trail.push(Vec::new());
                }
                _ => return Ok(None),
            }
        }
        let replayed = self.print_sticker_sheet_trail(&card, &trail)?;
        Ok(replayed
            .is_some_and(|(written_name, written)| written_name == name && written == text)
            .then_some(trail))
    }

    /// Replay row choices using the current sticker operands, then verify the sheet.
    pub fn print_sticker_sheet_trail(
        &self,
        card: &ir::UnifiedCard,
        trail: &[Vec<String>],
    ) -> Result<Option<(String, String)>, String> {
        self.print_sticker_sheet_with_trail(card, Some(trail))
    }

    fn print_sticker_sheet_with_trail(
        &self,
        card: &ir::UnifiedCard,
        trail: Option<&[Vec<String>]>,
    ) -> Result<Option<(String, String)>, String> {
        let card = canonical::unified(card)?;
        let (name, rules) = sheet_parts(&card)?;
        if trail.is_some_and(|rows| rows.len() != rules.len()) {
            return Ok(None);
        }
        let mut rows = Vec::new();
        for (index, sticker) in rules.iter().enumerate() {
            let row_trail = trail.map(|rows| rows[index].as_slice());
            let (tickets, text) = match sticker {
                ir::Sticker::PowerToughness {
                    tickets,
                    power,
                    toughness,
                } => {
                    if row_trail.is_some_and(|row| !row.is_empty()) {
                        return Ok(None);
                    }
                    (*tickets, format!("{power}/{toughness}"))
                }
                ir::Sticker::Abilities { tickets, abilities } => {
                    let text = if let Some(row) = row_trail {
                        let value = serde_json::to_value(abilities).map_err(|e| e.to_string())?;
                        self.print_trail("", &value, row)?
                            .filter(|text| !text.contains('\n'))
                    } else {
                        self.print_sticker_abilities(abilities)?
                    };
                    let Some(text) = text else {
                        return Ok(None);
                    };
                    (*tickets, text)
                }
                ir::Sticker::Name(_) => return Err("name sticker after rules stickers".into()),
            };
            if tickets == 0 || tickets > MAX_TICKETS {
                return Err("sticker ticket limit".into());
            }
            rows.push(format!("{} — {text}", "{TK}".repeat(tickets as usize)));
        }
        let text = rows.join("\n");
        let wanted = serde_json::to_value(&card).map_err(|e| e.to_string())?;
        let Ok(readings) = self.parse_sticker_sheet(&name, &text) else {
            return Ok(None);
        };
        if readings
            .iter()
            .all(|reading| serde_json::to_value(reading).ok().as_ref() == Some(&wanted))
        {
            return Ok(Some((name, text)));
        }
        Ok(None)
    }

    fn print_sticker_abilities(&self, abilities: &[ir::Ability]) -> Result<Option<String>, String> {
        let value = serde_json::to_value(abilities).map_err(|error| error.to_string())?;
        let wanted = super::canonical_abilities(&value)?;
        let verifies = |text: &str| {
            !text.contains('\n')
                && self
                    .parse("", text)
                    .is_ok_and(|parsed| parsed.readings.as_slice() == std::slice::from_ref(&wanted))
        };
        if let Some(text) = self.print_keyed("", &wanted, &[], &[])?
            && verifies(&text)
        {
            return Ok(Some(text));
        }
        Ok(self
            .enumerated("", &wanted, &[], 64, &[])?
            .texts
            .into_iter()
            .find(|text| verifies(text)))
    }
}

fn sheet_parts(card: &ir::UnifiedCard) -> Result<(String, &[ir::Sticker]), String> {
    let [
        ir::Field {
            value: ir::Operand::Stickers(stickers),
            ..
        },
    ] = card.data.as_slice()
    else {
        return Err("expected sticker sheet metadata".into());
    };
    if card.layout != ir::Layout::StickerSheet || stickers.len() > MAX_STICKERS {
        return Err("invalid sticker sheet layout".into());
    }
    let [
        ir::Sticker::Name(first),
        ir::Sticker::Name(second),
        ir::Sticker::Name(third),
        rules @ ..,
    ] = stickers.as_slice()
    else {
        return Err("expected three name stickers".into());
    };
    if ![first, second, third]
        .iter()
        .all(|word| valid_name_sticker(word))
    {
        return Err("invalid printed name sticker".into());
    }
    Ok((format!("{first} {second} {third}"), rules))
}

fn valid_name_sticker(word: &str) -> bool {
    word == "Hot Dog"
        || (word.chars().any(char::is_alphabetic)
            && word
                .chars()
                .all(|ch| ch.is_alphabetic() || matches!(ch, '-' | '\'' | '’')))
}

fn sticker_names(name: &str) -> Result<[&str; 3], Refusal> {
    let words: Vec<_> = name.split(' ').collect();
    let names = match words.as_slice() {
        [first, second, third] => [*first, *second, *third],
        [first, "Hot", "Dog", last] => [*first, "Hot Dog", *last],
        _ => return Err(Refusal::Lexical("expected three name stickers".into())),
    };
    if !names.iter().all(|word| valid_name_sticker(word)) {
        return Err(Refusal::Lexical("invalid name sticker".into()));
    }
    Ok(names)
}

fn ticket_cost(mut text: &str) -> Result<u32, Refusal> {
    let mut tickets = 0;
    while let Some(rest) = text.strip_prefix("{TK}") {
        tickets += 1;
        text = rest;
        if tickets > MAX_TICKETS {
            return Err(Refusal::Limit("sticker ticket limit".into()));
        }
    }
    if !text.is_empty() || tickets == 0 {
        return Err(Refusal::Lexical("expected a nonempty ticket cost".into()));
    }
    Ok(tickets)
}
