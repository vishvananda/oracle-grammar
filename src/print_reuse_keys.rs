//! Carried-over rows and traces, spelled in the new grammar's ids and indices.
use super::{Changes, Children, Saved, Side};
use crate::grammar::{RuleId, Sym};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

/// New ids of the rules a print under the new grammar may consult where the old print
/// could not: added rules, the new ids of strict and zone rules, and every rule beneath
/// them. The index offers and the printer asks a rule before it knows whether the rule
/// prints anything, and a rule it tries asks its children: a splice unifies with any
/// list. A rule outside them is asked as before, with the same values.
pub(super) fn fresh(
    old: &Side<'_>,
    new: &Side<'_>,
    strict: &HashSet<RuleId>,
    zone: &HashMap<RuleId, Children>,
) -> Vec<RuleId> {
    let mut stack: Vec<RuleId> = (0..new.keys.len())
        .filter_map(|id| RuleId::try_from(id).ok())
        .filter(|id| old.id(new.key(*id)).is_none())
        .collect();
    stack.extend(
        strict
            .iter()
            .chain(zone.keys())
            .filter_map(|before| new.id(old.key(*before))),
    );
    let mut seen = vec![false; new.keys.len()];
    let mut out = Vec::new();
    while let Some(rule) = stack.pop() {
        if std::mem::replace(&mut seen[rule as usize], true) {
            continue;
        }
        out.push(rule);
        for alternative in &new.grammar.rules[rule as usize].alternatives {
            for sym in &alternative.syms {
                if let Sym::Rule(child) = sym
                    && !seen[*child as usize]
                {
                    stack.push(*child);
                }
            }
        }
    }
    out.sort_unstable();
    out
}

impl Changes<'_> {
    /// A rule's name in the new grammar, `name#n` for duplicates.
    pub fn new_key(&self, rule: RuleId) -> &str {
        self.new.key(rule)
    }

    /// The trace in the new grammar's ids, for a card whose row carries over. A removed
    /// rule cannot be consulted any more. The rules the change introduced join, so the
    /// trace holds every rule a fresh print may consult and the next comparison checks
    /// them too; their checks ask what a value needs to fit them, so this costs little.
    pub fn renumber(&self, saved: &Saved) -> Option<Saved> {
        let mut rules = Vec::with_capacity(saved.rules.len() + self.fresh.len());
        for rule in &saved.rules {
            let key = self.old.keys.get(*rule as usize)?;
            rules.extend(self.new.id(key));
        }
        rules.extend(&self.fresh);
        rules.sort_unstable();
        rules.dedup();
        Some(Saved {
            rules,
            ..saved.clone()
        })
    }

    /// Spell a carried-over row's derivation key in the new grammar's alternative
    /// indices, as the scoreboard would: sorted unless the key is an ordered trail.
    /// A step through a rule that changed some other way, or any step outside the key,
    /// means the row must be printed again.
    pub fn rekey(&self, row: &mut serde_json::Value) -> Result<(), &'static str> {
        use serde_json::Value;
        let ordered = row.get("ordered") == Some(&Value::Bool(true));
        let object = row.as_object_mut().ok_or("unreadable row")?;
        for (field, value) in object.iter() {
            if field != "key" && self.names_a_changed_step(value) {
                return Err("the row names a changed rule outside its key");
            }
        }
        let Some(Value::Array(steps)) = object.get_mut("key") else {
            return Ok(());
        };
        let mut out = Vec::with_capacity(steps.len());
        for step in steps.iter() {
            let step = step.as_str().ok_or("unreadable key")?;
            let Some((name, index)) = step.rsplit_once('.').filter(|_| !step.starts_with('@'))
            else {
                out.push(step.to_string());
                continue;
            };
            if let Some(map) = self.renumbered.get(name) {
                let index: usize = index.parse().map_err(|_| "unreadable key")?;
                let moved = map.get(index).copied().flatten();
                out.push(format!(
                    "{name}.{}",
                    moved.ok_or("the key names a removed alternative")?
                ));
            } else if self.changed_names.contains(name) {
                return Err("the key names a changed rule");
            } else {
                out.push(step.to_string());
            }
        }
        if !ordered {
            out.sort();
            out.dedup();
        }
        *steps = out.into_iter().map(Value::String).collect();
        Ok(())
    }

    fn names_a_changed_step(&self, value: &serde_json::Value) -> bool {
        use serde_json::Value;
        match value {
            Value::String(text) => text.rsplit_once('.').is_some_and(|(name, index)| {
                !index.is_empty()
                    && index.bytes().all(|b| b.is_ascii_digit())
                    && (self.renumbered.contains_key(name) || self.changed_names.contains(name))
            }),
            Value::Array(items) => items.iter().any(|item| self.names_a_changed_step(item)),
            Value::Object(fields) => fields.values().any(|item| self.names_a_changed_step(item)),
            _ => false,
        }
    }
}
