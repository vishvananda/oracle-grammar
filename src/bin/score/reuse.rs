//! Carrying rows over from the last full print of a population, card by card.
//!
//! `--trace-out=FILE` writes, beside the rows, what each card's print consulted.
//! `--reuse-rows=ROWS --reuse-traces=FILE --reuse-grammar=DIR` read a previous run
//! with its grammar; a card whose trace no difference between the two grammars can
//! reach keeps its old row byte for byte. `--reuse-mode=shadow` prints every card and
//! only reports which predictions a full print contradicts. `--reuse-report=FILE`
//! writes the counts.
use super::{Judged, Options, Score};
use oracle_ir_grammar::engine::Engine;
use oracle_ir_grammar::print_reuse::{Changes, Saved};
use oracle_ir_grammar::print_trace;
use rustc_hash::FxHashMap as HashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::hash::Hasher;
use std::io::{BufRead, Write};
use std::sync::Mutex;

const FORMAT: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Mode {
    On,
    Shadow,
}

pub(super) struct Spec {
    rows: String,
    traces: String,
    pub(super) grammar: String,
    mode: Mode,
}

pub(super) struct Flags {
    pub(super) trace_out: Option<String>,
    pub(super) spec: Option<Spec>,
    pub(super) report: Option<String>,
}

pub(super) fn flags(args: &[String]) -> Result<Flags, String> {
    let value = |flag: &str| {
        args.iter()
            .find_map(|a| a.strip_prefix(flag).map(str::to_string))
    };
    let mode = match value("--reuse-mode=").as_deref() {
        None | Some("on") => Mode::On,
        Some("shadow") => Mode::Shadow,
        Some(other) => return Err(format!("unknown --reuse-mode {other}")),
    };
    let spec = match (
        value("--reuse-rows="),
        value("--reuse-traces="),
        value("--reuse-grammar="),
    ) {
        (Some(rows), Some(traces), Some(grammar)) => Some(Spec {
            rows,
            traces,
            grammar,
            mode,
        }),
        (None, None, None) => None,
        _ => return Err("--reuse-rows, --reuse-traces and --reuse-grammar go together".into()),
    };
    Ok(Flags {
        trace_out: value("--trace-out="),
        spec,
        report: value("--reuse-report="),
    })
}

/// What a trace file says about the run that wrote it.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Header {
    format: u32,
    fingerprint: String,
    binary: String,
    options: String,
    /// Rules at least half the cards consulted; each card's line lists how its own
    /// rules differ from these.
    core: String,
}

/// One card's line in a trace file, in row order. Id lists are [`pack`]ed.
#[derive(Debug, Serialize, Deserialize)]
struct Line {
    name: String,
    key: String,
    rules: String,
    prefixes: String,
    features: String,
    words: String,
    pairs: String,
    vocabulary: String,
    steps: u64,
    depth: usize,
    score: Score,
    stuck: Option<String>,
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const SYMBOLS: [u8; 256] = {
    let mut table = [u8::MAX; 256];
    let mut at = 0;
    while at < 64 {
        table[ALPHABET[at] as usize] = at as u8;
        at += 1;
    }
    table
};

/// Sorted ids as base64 of their LEB128 gaps.
fn pack(ids: &[u32]) -> String {
    let mut bytes = Vec::with_capacity(ids.len() + 8);
    let mut previous = 0u32;
    for (index, id) in ids.iter().enumerate() {
        let mut gap = if index == 0 { *id } else { id - previous - 1 };
        previous = *id;
        loop {
            let low = (gap & 0x7f) as u8;
            gap >>= 7;
            bytes.push(if gap == 0 { low } else { low | 0x80 });
            if gap == 0 {
                break;
            }
        }
    }
    let mut text = String::with_capacity(bytes.len() * 4 / 3 + 4);
    for chunk in bytes.chunks(3) {
        let word = chunk
            .iter()
            .enumerate()
            .fold(0u32, |w, (i, b)| w | u32::from(*b) << (16 - 8 * i));
        for at in 0..=chunk.len() {
            text.push(char::from(
                ALPHABET[(word >> (18 - 6 * at)) as usize & 0x3f],
            ));
        }
    }
    text
}

fn unpack(text: &str) -> Result<Vec<u32>, String> {
    let mut bytes = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.as_bytes().chunks(4) {
        let mut word = 0u32;
        for (at, symbol) in chunk.iter().enumerate() {
            let value = SYMBOLS[usize::from(*symbol)];
            if value == u8::MAX {
                return Err("bad trace encoding".into());
            }
            word |= u32::from(value) << (18 - 6 * at);
        }
        bytes.extend((0..chunk.len().saturating_sub(1)).map(|i| (word >> (16 - 8 * i)) as u8));
    }
    let mut ids = Vec::new();
    let (mut gap, mut shift, mut previous) = (0u32, 0u32, None::<u32>);
    for byte in bytes {
        gap |= u32::from(byte & 0x7f)
            .checked_shl(shift)
            .ok_or("bad trace gap")?;
        shift += 7;
        if byte & 0x80 == 0 {
            let id = match previous {
                None => gap,
                Some(before) => before
                    .checked_add(gap)
                    .and_then(|v| v.checked_add(1))
                    .ok_or("bad trace id")?,
            };
            ids.push(id);
            previous = Some(id);
            (gap, shift) = (0, 0);
        }
    }
    Ok(ids)
}

/// The ids in exactly one of two sorted lists.
fn toggle(a: &[u32], b: &[u32]) -> Vec<u32> {
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() || j < b.len() {
        match (a.get(i), b.get(j)) {
            (Some(x), Some(y)) if x == y => (i, j) = (i + 1, j + 1),
            (Some(x), Some(y)) if x < y => {
                out.push(*x);
                i += 1;
            }
            (Some(_), Some(y)) | (None, Some(y)) => {
                out.push(*y);
                j += 1;
            }
            (Some(x), None) => {
                out.push(*x);
                i += 1;
            }
            (None, None) => break,
        }
    }
    out
}

/// One card's trace, held packed while the population prints.
#[derive(Debug, Default)]
pub(super) struct Packed {
    rules: String,
    prefixes: String,
    features: String,
    words: String,
    pairs: String,
    vocabulary: String,
    steps: u64,
    depth: usize,
}

impl Packed {
    pub(super) fn new(saved: &Saved) -> Self {
        Self {
            rules: pack(&saved.rules),
            prefixes: pack(&saved.prefixes),
            features: pack(&saved.features),
            words: pack(&saved.words),
            pairs: pack(&saved.pairs),
            vocabulary: pack(&saved.vocabulary),
            steps: saved.steps,
            depth: saved.depth,
        }
    }
}

impl Line {
    fn saved(&self, core: &[u32]) -> Result<Saved, String> {
        Ok(Saved {
            rules: toggle(core, &unpack(&self.rules)?),
            prefixes: unpack(&self.prefixes)?,
            features: unpack(&self.features)?,
            words: unpack(&self.words)?,
            pairs: unpack(&self.pairs)?,
            vocabulary: unpack(&self.vocabulary)?,
            steps: self.steps,
            depth: self.depth,
        })
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn fx(bytes: &[u8]) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write(bytes);
    hasher.finish()
}

fn wide(bytes: &[u8]) -> String {
    format!("{:016x}{:016x}", fnv(bytes), fx(bytes))
}

/// The running scoreboard, so a run of another build never lends its rows.
fn binary() -> Result<String, String> {
    let path = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(wide(&std::fs::read(path).map_err(|e| e.to_string())?))
}

fn options_key(options: &Options) -> String {
    format!(
        "render={} raw={} source_context={} round_trip={} without={:?}",
        options.render, options.raw, options.source_context, options.round_trip, options.without
    )
}

/// A card's identity: everything the scoreboard reads of it before the grammar does.
pub(super) fn face_key(face: &Value, text: &str, context: &str) -> String {
    let mut bytes = face.to_string().into_bytes();
    for part in [text, context] {
        bytes.push(0);
        bytes.extend_from_slice(part.as_bytes());
    }
    wide(&bytes)
}

struct Entry {
    /// The row as this scoreboard writes it, whoever wrote the file it came from.
    row: String,
    line: Line,
}

/// The previous run and what changed since.
pub(super) struct Previous<'a> {
    changes: Changes<'a>,
    core: Vec<u32>,
    entries: HashMap<String, Entry>,
    mode: Mode,
    tally: Mutex<Tally>,
}

#[derive(Default, Serialize)]
struct Tally {
    reused: u64,
    printed: u64,
    unseen: u64,
    reasons: std::collections::BTreeMap<String, u64>,
    predicted: u64,
    contradicted: Vec<String>,
    /// The names whose rows carried over, or would have in shadow mode.
    carried: Vec<String>,
    /// Per changed rule: the cards it was the first requirement met for, the cards
    /// it alone could read, and all the cards it could read.
    required: std::collections::BTreeMap<String, u64>,
    alone: std::collections::BTreeMap<String, u64>,
    reads: std::collections::BTreeMap<String, u64>,
    /// Per consulted rule and changed child, the cards the print-side check reprints.
    zone: std::collections::BTreeMap<String, u64>,
    /// Per rule a fresh print consulted that a predicted row's carried trace lacks,
    /// the cards: each could hide the rule's next change from the next comparison.
    uncovered: std::collections::BTreeMap<String, u64>,
}

fn read_lines(path: &str) -> Result<Vec<String>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    std::io::BufReader::new(file)
        .lines()
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())
}

impl<'a> Previous<'a> {
    pub(super) fn load(
        spec: &Spec,
        old: &'a Engine,
        new: &'a Engine,
        options: &Options,
    ) -> Result<Self, String> {
        let rows = read_lines(&spec.rows)?;
        let traces = read_lines(&spec.traces)?;
        let (head, lines) = traces.split_first().ok_or("empty trace file")?;
        let header: Header = serde_json::from_str(head).map_err(|e| e.to_string())?;
        let expected = Header {
            format: FORMAT,
            fingerprint: old.grammar.fingerprint.clone(),
            binary: binary()?,
            options: options_key(options),
            core: header.core.clone(),
        };
        if header != expected || lines.len() != rows.len() {
            return Err(
                "the trace file belongs to another grammar, build, mode or row file".into(),
            );
        }
        // Switched-off rules are named by id, which another grammar renumbers.
        if !options.without.is_empty() {
            return Err("rows printed without some rules do not carry over".into());
        }
        let mut entries = HashMap::default();
        for (row, line) in rows.into_iter().zip(lines) {
            let line: Line = serde_json::from_str(line).map_err(|e| e.to_string())?;
            let named: Value = serde_json::from_str(&row).map_err(|e| e.to_string())?;
            if named["name"].as_str() != Some(line.name.as_str()) {
                return Err("trace and row files are out of step".into());
            }
            let row = named.to_string();
            entries
                .entry(line.key.clone())
                .or_insert(Entry { row, line });
        }
        Ok(Self {
            changes: Changes::between(&old.grammar, &new.grammar, old.limits),
            core: unpack(&header.core)?,
            entries,
            mode: spec.mode.clone(),
            tally: Mutex::new(Tally::default()),
        })
    }

    /// Whether a row may carry over at all: a complete exact print with no search
    /// diagnostics, whose alternatives it names by index have not moved.
    /// The row as the new grammar would spell it, if it may carry over at all: a
    /// complete exact print with no search diagnostics and the same text.
    fn eligible(&self, entry: &Entry, text: &str) -> Result<String, &'static str> {
        let mut row: Value = serde_json::from_str(&entry.row).map_err(|_| "unreadable row")?;
        let shown = [
            "printing",
            "print_frontier",
            "source_context",
            "syntax",
            "rendered",
        ];
        if row["status"] != "equal"
            || row["exact"] != true
            || shown.iter().any(|k| row.get(*k).is_some())
        {
            return Err("only exact equal rows carry over");
        }
        if row["text"] != text {
            return Err("the text changed");
        }
        self.changes.rekey(&mut row)?;
        Ok(row.to_string())
    }

    fn note(&self, outcome: Result<(), &'static str>) {
        let mut tally = self
            .tally
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match outcome {
            Ok(()) => tally.reused += 1,
            Err(why) => {
                tally.printed += 1;
                *tally.reasons.entry(why.to_string()).or_default() += 1;
            }
        }
    }

    /// Print a card, or carry its row over.
    pub(super) fn judge(
        &self,
        key: &str,
        name: &str,
        text: &str,
        print: impl FnOnce() -> Result<Judged, String>,
    ) -> Result<(Judged, Saved), String> {
        let Some(entry) = self.entries.get(key).filter(|e| e.line.name == name) else {
            self.tally
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .unseen += 1;
            return traced(print);
        };
        let saved = entry.line.saved(&self.core)?;
        let verdict = match self.changes.verdict(&saved) {
            None => self.eligible(entry, text),
            Some(why) => Err(why),
        };
        let cause = self.changes.zone_cause(&saved);
        let hits: Vec<&str> = self.changes.required_hits(&saved).collect();
        if let Some(cause) = cause {
            *self
                .tally
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .zone
                .entry(cause)
                .or_default() += 1;
        }
        if !hits.is_empty() {
            let mut tally = self
                .tally
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *tally.required.entry(hits[0].to_string()).or_default() += 1;
            if hits.len() == 1 {
                *tally.alone.entry(hits[0].to_string()).or_default() += 1;
            }
            for rule in hits {
                *tally.reads.entry(rule.to_string()).or_default() += 1;
            }
        }
        let renumbered = verdict.and_then(|row| {
            self.changes
                .renumber(&saved)
                .map(|saved| (row, saved))
                .ok_or("a consulted rule has no new id")
        });
        if self.mode == Mode::On
            && let Ok((row, saved)) = renumbered
        {
            self.note(Ok(()));
            self.tally
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .carried
                .push(name.to_string());
            let judged = Judged {
                row: Value::Null,
                raw: Some(row),
                score: entry.line.score.clone(),
                stuck: entry.line.stuck.clone(),
            };
            return Ok((judged, saved));
        }
        let (judged, saved) = traced(print)?;
        match renumbered {
            Ok((row, carried)) => {
                let mut tally = self
                    .tally
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                tally.predicted += 1;
                tally.carried.push(name.to_string());
                for rule in &saved.rules {
                    if carried.rules.binary_search(rule).is_err() {
                        let key = self.changes.new_key(*rule).to_string();
                        *tally.uncovered.entry(key).or_default() += 1;
                    }
                }
                // Compare the bytes the scoreboard writes, not the JSON value.
                let fresh = judged.row.to_string();
                if fresh != row {
                    tally.contradicted.push(name.to_string());
                }
                drop(tally);
                self.note(Err("shadow"));
            }
            Err(why) => self.note(Err(why)),
        }
        Ok((judged, saved))
    }

    pub(super) fn report(&self, path: &str) -> Result<(), String> {
        let tally = self
            .tally
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut carried = tally.carried.clone();
        carried.sort_unstable();
        let report = json!({
            "mode": if self.mode == Mode::On { "on" } else { "shadow" },
            "changes": self.changes.summary,
            "reused": tally.reused,
            "printed": tally.printed,
            "unseen": tally.unseen,
            "reasons": tally.reasons,
            "predicted": tally.predicted,
            "carried": carried,
            "uncovered": tally.uncovered,
            "contradicted": tally.contradicted,
            "required": tally.required,
            "required_alone": tally.alone,
            "required_reads": tally.reads,
            "zone_causes": tally.zone,
        });
        std::fs::write(path, format!("{report}\n")).map_err(|e| e.to_string())
    }
}

/// Report a previous run that cannot lend rows; every card is printed.
pub(super) fn unavailable(path: &str, why: &str) -> Result<(), String> {
    let report = json!({"unavailable": why, "reused": 0});
    std::fs::write(path, format!("{report}\n")).map_err(|e| e.to_string())
}

/// Print a card and keep what its print consulted.
pub(super) fn traced(
    print: impl FnOnce() -> Result<Judged, String>,
) -> Result<(Judged, Saved), String> {
    let (judged, trace) = print_trace::record(print);
    Ok((judged?, Saved::from_trace(&trace)))
}

/// Write the trace file for rows in the order they were written.
pub(super) fn write_traces(
    path: &str,
    engine: &Engine,
    options: &Options,
    cards: &[(String, String, Packed, Score, Option<String>)],
) -> Result<(), String> {
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    let mut counts: HashMap<u32, usize> = HashMap::default();
    for (_, _, packed, _, _) in cards {
        for id in unpack(&packed.rules)? {
            *counts.entry(id).or_default() += 1;
        }
    }
    let mut core: Vec<u32> = counts
        .into_iter()
        .filter(|(_, count)| count * 2 >= cards.len())
        .map(|(id, _)| id)
        .collect();
    core.sort_unstable();
    let header = Header {
        format: FORMAT,
        fingerprint: engine.grammar.fingerprint.clone(),
        binary: binary()?,
        options: options_key(options),
        core: pack(&core),
    };
    let write = |out: &mut std::io::BufWriter<std::fs::File>, value: String| {
        writeln!(out, "{value}").map_err(|e| e.to_string())
    };
    write(
        &mut out,
        serde_json::to_string(&header).map_err(|e| e.to_string())?,
    )?;
    for (name, key, packed, score, stuck) in cards {
        let line = Line {
            name: name.clone(),
            key: key.clone(),
            rules: pack(&toggle(&core, &unpack(&packed.rules)?)),
            prefixes: packed.prefixes.clone(),
            features: packed.features.clone(),
            words: packed.words.clone(),
            pairs: packed.pairs.clone(),
            vocabulary: packed.vocabulary.clone(),
            steps: packed.steps,
            depth: packed.depth,
            score: score.clone(),
            stuck: stuck.clone(),
        };
        write(
            &mut out,
            serde_json::to_string(&line).map_err(|e| e.to_string())?,
        )?;
    }
    out.flush().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{pack, toggle, unpack};

    #[test]
    fn packed_ids_read_back() {
        for ids in [
            vec![],
            vec![0],
            vec![5],
            vec![0, 1, 2, 200, 70_000, u32::MAX - 1, u32::MAX],
        ] {
            assert_eq!(unpack(&pack(&ids)).unwrap(), ids);
        }
        let dense: Vec<u32> = (0..3_700).map(|i| i * 6).collect();
        assert!(pack(&dense).len() < 4 * dense.len() / 3 + 4);
    }

    #[test]
    fn toggling_twice_restores_a_list() {
        let (core, card) = (vec![1, 3, 5, 9], vec![2, 3, 9, 11]);
        let differs = toggle(&core, &card);
        assert_eq!(differs, vec![1, 2, 5, 11]);
        assert_eq!(toggle(&core, &differs), card);
    }
}
