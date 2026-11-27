//! Bounded inverse views using the same immediate-operation proofs as comparison.
use super::{
    immediate_values, scalar_reads, single_choices, target_stats,
    walk::{Visit, Walker, lossless},
};
use crate::{ir::Instruction, ir_schema::Ty};
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Default)]
struct Inputs {
    ids: HashSet<u64>,
    scalar: bool,
    choices: bool,
    affected: bool,
}
impl Visit for Inputs {
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        _: bool,
        _: &str,
        _: &mut Walker<'_>,
    ) -> Result<(), String> {
        if matches!(ty, Ty::Named(name) if name == "ResultId")
            && let Some(id) = value.as_u64()
        {
            self.ids.insert(id);
        }
        if matches!(ty, Ty::Named(name) if name == "Operation") {
            self.affected |= target_stats::may_print(value);
        }
        if matches!(ty, Ty::Named(name) if name == "Program")
            && let Some(items) = value["instructions"].as_array()
        {
            self.scalar |= items.iter().any(|item| capture_parameter(item).is_some());
            self.choices |= single_choices::may_print(items);
        }
        Ok(())
    }
}

struct View {
    ids: HashSet<u64>,
    next: u64,
    captures: usize,
    scalar: bool,
    choices: bool,
    affected: bool,
    changed: bool,
}
impl View {
    fn capture(
        &mut self,
        item: &mut Value,
        walker: &mut Walker<'_>,
    ) -> Result<Option<Value>, String> {
        if self.captures == 64 {
            return Ok(None);
        }
        let Some((slots, value)) = capture_parameter(item) else {
            return Ok(None);
        };
        if lossless::<Instruction>(item, false).is_none() {
            return Ok(None);
        }
        let mut value = value.clone();
        let operation = &mut item["args"]["operation"];
        if !scalar_reads::pure(&mut value, walker)? {
            return Ok(None);
        }
        while self.ids.contains(&self.next) {
            self.next += 1;
        }
        if self.next > u32::MAX as u64 {
            return Ok(None);
        }
        let id = self.next;
        self.ids.insert(id);
        self.next += 1;
        self.captures += 1;
        let reference = json!({"kind":"BoundValue","args":id});
        for slot in slots {
            let operand = operation.pointer_mut(slot).expect("checked immediate slot");
            if let Some((_, sign)) = scalar_reads::parameter(operand) {
                *operand =
                    scalar_reads::amount(&reference, sign).expect("bound capture is nonconstant");
            }
        }
        Ok(Some(
            json!({"kind":"Effect","args":{"result":id,"operation":{
            "kind":"BindValue","args":{"value":value}}}}),
        ))
    }
}
/// A quick eligibility check shared by the scan and capture. Typed shape,
/// purity and final canonical equality are checked before offering the view.
fn capture_parameter(item: &Value) -> Option<(&'static [&'static str], &Value)> {
    if item["kind"] != "Effect" {
        return None;
    }
    let operation = &item["args"]["operation"];
    let slots = immediate_values::slots(operation)?;
    let mut first = None;
    for slot in slots {
        let Some((value, _)) = scalar_reads::parameter(operation.pointer(slot)?) else {
            continue;
        };
        if first.is_some_and(|prior| prior != value) {
            return None;
        }
        first = Some(value);
    }
    let first = first?;
    Some((slots, first))
}

impl Visit for View {
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        _: bool,
        _: &str,
        walker: &mut Walker<'_>,
    ) -> Result<(), String> {
        if self.affected && matches!(ty, Ty::Named(name) if name == "Operation") {
            self.changed |= target_stats::print(value);
        }
        if !matches!(ty, Ty::Named(name) if name == "Program") {
            return Ok(());
        }
        if self.choices {
            self.changed |= single_choices::print_program(value);
        }
        if self.scalar
            && let Some(items) = value.get_mut("instructions").and_then(Value::as_array_mut)
        {
            let mut index = 0;
            while index < items.len() {
                if let Some(capture) = self.capture(&mut items[index], walker)? {
                    items.insert(index, capture);
                    index += 1;
                    self.changed = true;
                }
                index += 1;
            }
        }
        Ok(())
    }
}

/// Current-operand views for scalar captures, one-object receipts and
/// affected-object statistics. Fresh result IDs avoid every original typed ID,
/// including nested scopes and unresolved references. Every returned view must
/// normalize to the input; callers must still verify any printed text by reparse.
pub fn print_views(value: &Value) -> Result<Vec<Value>, String> {
    let ty = Ty::Vec(Box::new(Ty::Named("Ability".into())));
    let schema = super::schema()?;
    let mut inputs = Inputs::default();
    Walker::new(schema).walk(&mut value.clone(), &ty, false, "", 0, &mut inputs)?;
    let mut views = Vec::new();
    let mut canonical = None;
    let combinations = [
        (false, true, false),
        (true, false, false),
        (true, true, false),
        (false, false, true),
        (false, true, true),
        (true, false, true),
        (true, true, true),
    ];
    for (scalar, choices, affected) in combinations {
        if (scalar && !inputs.scalar)
            || (choices && !inputs.choices)
            || (affected && !inputs.affected)
        {
            continue;
        }
        let mut view = value.clone();
        let mut proof = View {
            ids: if scalar {
                inputs.ids.clone()
            } else {
                HashSet::new()
            },
            next: 0,
            captures: 0,
            scalar,
            choices,
            affected,
            changed: false,
        };
        Walker::new(schema).walk(&mut view, &ty, false, "", 0, &mut proof)?;
        if !proof.changed || views.contains(&view) {
            continue;
        }
        if canonical.is_none() {
            let mut normalized = value.clone();
            super::semantic_abilities(&mut normalized)?;
            canonical = Some(normalized);
        }
        let mut normalized = view.clone();
        super::semantic_abilities(&mut normalized)?;
        if Some(&normalized) == canonical.as_ref() {
            views.push(view);
        }
    }
    Ok(views)
}
