//! Expand one written subject into the immediate affected slot of each static arm.
use crate::model::ir_schema::{Schema, Ty};
use crate::pattern::{Bindings, Pattern, Syntax, elaborate};
use serde_json::Value;

const MAX_CONJUNCTS: usize = 10;
pub(crate) const SUBJECT_SLOT: &str = "static_subject_slot";

pub(crate) fn checked_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Option<Pattern>, String> {
    if name == SUBJECT_SLOT {
        if !args.is_empty() || *ty != Ty::Named("ObjectSet".into()) {
            return Err(format!("@{name} takes no arguments and produces ObjectSet"));
        }
        return Ok(Some(Pattern::Symbol(name.into(), Vec::new())));
    }
    SharedStatic::named(name)
        .map(|shared| shared.elaborate(name, args, ty, schema))
        .transpose()
}

#[derive(Clone, Copy)]
pub(crate) enum SharedStatic {
    Subject,
    Filter,
}

impl SharedStatic {
    pub(crate) fn named(name: &str) -> Option<Self> {
        match name {
            "shared_static_subject" => Some(Self::Subject),
            "shared_static_filter" => Some(Self::Filter),
            _ => None,
        }
    }

    fn elaborate(
        self,
        name: &str,
        args: &[Syntax],
        ty: &Ty,
        schema: &Schema,
    ) -> Result<Pattern, String> {
        let [subject, arms] = args else {
            return Err(format!("@{name} needs a subject and a list of static arms"));
        };
        if *ty != Ty::Vec(Box::new(Ty::Named("Ability".into()))) {
            return Err(format!("@{name} produces Vec<Ability>"));
        }
        let subject_type = match self {
            Self::Subject => "ObjectSet",
            Self::Filter => "ObjectFilter",
        };
        Ok(Pattern::Symbol(
            name.into(),
            vec![
                elaborate(subject, &Ty::Named(subject_type.into()), schema)?,
                elaborate(arms, ty, schema)?,
            ],
        ))
    }

    fn path(self) -> &'static str {
        match self {
            Self::Subject => "/args/then/0/args/affected/Objects",
            Self::Filter => "/args/affected/Objects/args/filter",
        }
    }

    fn slot(self, arm: &Value) -> Option<&Value> {
        let static_ability = match self {
            Self::Subject => {
                if arm["kind"] != "Conditional" || !arm["args"]["otherwise"].as_array()?.is_empty()
                {
                    return None;
                }
                let [ability] = arm["args"]["then"].as_array()?.as_slice() else {
                    return None;
                };
                ability
            }
            Self::Filter => {
                let objects = arm.pointer("/args/affected/Objects")?;
                if objects["kind"] != "Zone"
                    || objects["args"]["scope"]["Shared"] != "Battlefield"
                    || objects["args"]["kind"] != "Permanent"
                {
                    return None;
                }
                arm
            }
        };
        if static_ability["kind"] != "Static" || static_ability["args"]["kind"] != "Ordinary" {
            return None;
        }
        arm.pointer(self.path())
    }

    fn slots(self, arms: &Value) -> Option<Vec<&Value>> {
        let entries = arms.as_array()?;
        if !(2..=5).contains(&entries.len()) {
            return None;
        }
        let slots = entries
            .iter()
            .map(|arm| self.slot(arm))
            .collect::<Option<_>>()?;
        super::branch_bounds(arms).ok()?;
        Some(slots)
    }

    pub(crate) fn fill(self, subject: &Value, arms: &Value) -> Result<Value, String> {
        super::branch_bounds(subject)?;
        let slots = self
            .slots(arms)
            .ok_or("shared static needs two to five ordinary arms")?;
        let mut replacements = Vec::with_capacity(slots.len());
        for slot in slots {
            let replacement = match self {
                Self::Subject if subject["kind"] == "One" && slot["$sym"] == SUBJECT_SLOT => {
                    subject.clone()
                }
                Self::Filter if atomic_quality(slot) => {
                    super::conjunction(vec![subject.clone(), slot.clone()])
                }
                _ => return Err("shared static arm has the wrong subject slot".into()),
            };
            replacements.push(replacement);
        }
        let output = self
            .replace(arms, replacements)
            .ok_or("missing static subject slot")?;
        super::branch_bounds(&output)?;
        Ok(output)
    }

    fn replace(self, arms: &Value, replacements: impl IntoIterator<Item = Value>) -> Option<Value> {
        let mut output = arms.clone();
        for (arm, replacement) in output.as_array_mut()?.iter_mut().zip(replacements) {
            *arm.pointer_mut(self.path())? = replacement;
        }
        Some(output)
    }

    pub(crate) fn unify<'p, 'v>(
        self,
        args: &'p [Pattern],
        value: &'v Value,
        bindings: Bindings<'p, 'v>,
    ) -> Vec<Bindings<'p, 'v>> {
        let [subject, arms] = args else {
            return Vec::new();
        };
        let Some(slots) = self.slots(value) else {
            return Vec::new();
        };
        if matches!(self, Self::Filter) {
            return self.unify_filter(subject, arms, value, &slots, &bindings);
        }
        let common = slots[0];
        if common["kind"] != "One" || slots.iter().any(|&slot| slot != common) {
            return Vec::new();
        }
        // Match the rows with their original affected objects so nested
        // grant printing still has the receiver's spelling context.
        subject
            .unify(common, bindings)
            .into_iter()
            .flat_map(|found| arms.unify(value, found))
            .collect()
    }

    fn unify_filter<'p, 'v>(
        self,
        subject: &'p Pattern,
        arms: &'p Pattern,
        value: &Value,
        slots: &[&Value],
        bindings: &Bindings<'p, 'v>,
    ) -> Vec<Bindings<'p, 'v>> {
        let Some(parts) = slots
            .iter()
            .map(|slot| filter_parts(slot))
            .collect::<Option<Vec<_>>>()
        else {
            return Vec::new();
        };
        let [first, rest @ ..] = parts.as_slice() else {
            return Vec::new();
        };
        let mut seen = rustc_hash::FxHashSet::default();
        let mut result = Vec::new();
        // Each quality is one conjunct. Try every possible deletion in the
        // first arm. In another arm, two deletions that leave the same
        // sequence must delete equal terms and produce the same binding.
        for index in (0..first.len()).rev() {
            let common: Vec<_> = omit(first, index).collect();
            if !seen.insert(common.clone()) {
                continue;
            }
            let Some(mut qualities) = rest
                .iter()
                .map(|parts| {
                    if parts.len() != first.len() {
                        return None;
                    }
                    parts.iter().enumerate().find_map(|(index, quality)| {
                        omit(parts, index)
                            .eq(common.iter().copied())
                            .then_some(*quality)
                    })
                })
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            qualities.insert(0, first[index]);
            let common = super::conjunction(common.into_iter().cloned().collect());
            for binding in subject.unify_made(&common, bindings) {
                if let Some(rows) = self.replace(value, qualities.iter().copied().cloned()) {
                    result.extend(arms.unify_made(&rows, &binding));
                }
            }
        }
        result
    }
}

fn atomic_quality(value: &Value) -> bool {
    value["kind"]
        .as_str()
        .is_some_and(|kind| !matches!(kind, "AllOf" | "Any"))
}

fn filter_parts(value: &Value) -> Option<Vec<&Value>> {
    fn collect<'a>(value: &'a Value, parts: &mut Vec<&'a Value>) -> Option<()> {
        match value["kind"].as_str() {
            Some("Any") => {}
            Some("AllOf") => {
                for item in value["args"].as_array()? {
                    collect(item, parts)?;
                }
            }
            _ if parts.len() < MAX_CONJUNCTS && value.is_object() => parts.push(value),
            _ => return None,
        }
        Some(())
    }
    let mut parts = Vec::new();
    collect(value, &mut parts)?;
    (!parts.is_empty()).then_some(parts)
}

fn omit<'a>(parts: &[&'a Value], index: usize) -> impl Iterator<Item = &'a Value> {
    parts[..index].iter().chain(&parts[index + 1..]).copied()
}
