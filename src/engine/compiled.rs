//! Build and load a bounded subset of the elaborated bidirectional grammar.
use super::{Engine, LEXICAL};
use crate::grammar::{self, Grammar, Rule, RuleId, Sym};
use crate::model::ir::SCHEMA_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const FORMAT: u16 = 1;
const BYTES: usize = 8 * 1024 * 1024;
const RULES: usize = 4096;
const ALTERNATIVES: usize = 32768;
const WIDTH: usize = 256;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Table {
    format: u16,
    schema: u32,
    lexical: String,
    fingerprint: String,
    rules: Vec<Rule>,
}

type Selection = BTreeMap<RuleId, BTreeSet<usize>>;

impl Engine {
    /// Serialize the selected productions, with their already elaborated
    /// meanings. `complete_rules` keeps every alternative of its named rules.
    /// Each remaining multi-alternative dependency must be selected explicitly.
    pub fn compile_slice(
        &self,
        steps: &[String],
        complete_rules: &[&str],
    ) -> Result<Vec<u8>, String> {
        if steps.len() > ALTERNATIVES || complete_rules.len() > RULES {
            return Err("compiled grammar selection exceeds limit".into());
        }
        let mut selected = Selection::new();
        for step in steps {
            if step.starts_with('@') {
                continue;
            }
            let (name, offset) = step.rsplit_once('.').ok_or("invalid production step")?;
            let rule = self.compiled_rule(name)?;
            let offset = offset.parse::<usize>().map_err(|error| error.to_string())?;
            selected.entry(rule).or_default().insert(offset);
        }
        for name in complete_rules {
            let id = self.compiled_rule(name)?;
            selected
                .entry(id)
                .or_default()
                .extend(0..self.grammar.rules[id as usize].alternatives.len());
        }
        self.close_selection(&mut selected)?;
        let index = selected
            .keys()
            .copied()
            .enumerate()
            .map(|(next, old)| {
                Ok((
                    old,
                    RuleId::try_from(next).map_err(|error| error.to_string())?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        let mut rules = Vec::with_capacity(index.len());
        for (id, chosen) in selected {
            let rule = &self.grammar.rules[id as usize];
            let alternatives = chosen
                .into_iter()
                .map(|offset| {
                    let mut alternative = rule.alternatives[offset].clone();
                    for sym in &mut alternative.syms {
                        if let Sym::Rule(old) = sym {
                            *old = index[old];
                        }
                    }
                    alternative
                })
                .collect();
            rules.push(Rule {
                name: rule.name.clone(),
                alternatives,
                sem: rule.sem.clone(),
                ty: rule.ty.clone(),
                prefer: rule.prefer.clone(),
            });
        }
        Table {
            format: FORMAT,
            schema: SCHEMA_VERSION,
            lexical: grammar::fingerprint(LEXICAL),
            fingerprint: self.grammar.fingerprint.clone(),
            rules,
        }
        .encode()
    }

    fn compiled_rule(&self, name: &str) -> Result<RuleId, String> {
        self.grammar
            .names
            .get(name)
            .copied()
            .ok_or_else(|| format!("unknown rule {name}"))
    }

    fn close_selection(&self, selected: &mut Selection) -> Result<(), String> {
        let mut pending: Vec<_> = selected.keys().copied().collect();
        let mut alternatives = 0;
        while let Some(id) = pending.pop() {
            let rule = &self.grammar.rules[id as usize];
            let offsets = &selected[&id];
            alternatives += offsets.len();
            if selected.len() > RULES || alternatives > ALTERNATIVES {
                return Err("compiled grammar exceeds rule or alternative limit".into());
            }
            let mut children = Vec::new();
            for offset in offsets {
                let alternative = rule
                    .alternatives
                    .get(*offset)
                    .ok_or_else(|| format!("unknown alternative {}.{offset}", rule.name))?;
                for sym in &alternative.syms {
                    let Sym::Rule(child) = sym else { continue };
                    if !selected.contains_key(child) {
                        children.push(*child);
                    }
                }
            }
            for child in children {
                if selected.contains_key(&child) {
                    continue;
                }
                let dependency = &self.grammar.rules[child as usize];
                let alternatives = match dependency.alternatives.len() {
                    0 => BTreeSet::new(),
                    1 => BTreeSet::from([0]),
                    _ => return Err(format!("select alternatives of {}", dependency.name)),
                };
                selected.insert(child, alternatives);
                pending.push(child);
            }
        }
        if !selected.contains_key(&self.root) {
            return Err("select the card rule".into());
        }
        Ok(())
    }

    /// Load a compiler-produced table without reading EBNF. The Wasm build
    /// uses frozen schema descriptors. Native validation checks constructed IR.
    pub fn from_compiled(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > BYTES {
            return Err("compiled grammar exceeds byte limit".into());
        }
        let table: Table = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if table.format != FORMAT
            || table.schema != SCHEMA_VERSION
            || table.lexical != grammar::fingerprint(LEXICAL)
        {
            return Err("compiled grammar version, schema or lexical mismatch".into());
        }
        table.validate()?;
        let source = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
        let mut grammar = Grammar {
            rules: table.rules,
            fingerprint: format!("compiled-{}", grammar::fingerprint(source)),
            ..Grammar::default()
        };
        for (id, rule) in grammar.rules.iter().enumerate() {
            let id = RuleId::try_from(id).map_err(|error| error.to_string())?;
            if grammar.names.insert(rule.name.clone(), id).is_some() {
                return Err(format!("duplicate compiled rule {}", rule.name));
            }
        }
        grammar::initialize(&mut grammar)?;
        crate::model::canonical::schema()?;
        Self::from_grammar(grammar)
    }
}

impl Table {
    fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let encoded = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        if encoded.len() > BYTES {
            return Err("compiled grammar exceeds byte limit".into());
        }
        Ok(encoded)
    }

    fn validate(&self) -> Result<(), String> {
        if self.rules.len() > RULES
            || self
                .rules
                .iter()
                .map(|rule| rule.alternatives.len())
                .sum::<usize>()
                > ALTERNATIVES
        {
            return Err("compiled grammar exceeds rule or alternative limit".into());
        }
        for rule in &self.rules {
            for alternative in &rule.alternatives {
                if alternative.syms.len() > WIDTH
                    || alternative.syms.len() != alternative.holes.len()
                {
                    return Err(format!("invalid compiled alternative in {}", rule.name));
                }
                if alternative
                    .syms
                    .iter()
                    .any(|sym| matches!(sym, Sym::Rule(id) if *id as usize >= self.rules.len()))
                {
                    return Err(format!("invalid compiled rule reference in {}", rule.name));
                }
            }
        }
        Ok(())
    }
}
