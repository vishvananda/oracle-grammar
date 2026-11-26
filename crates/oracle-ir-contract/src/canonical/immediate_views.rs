//! Bounded inverse views using the same immediate-operation proofs as comparison.
use super::{
    immediate_values, scalar_reads, single_choices,
    walk::{Visit, Walker, lossless},
};
use crate::{ir::Instruction, ir_schema::Ty};
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Default)]
struct Ids(HashSet<u64>);
impl Visit for Ids {
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
            self.0.insert(id);
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
    changed: bool,
}
impl View {
    fn capture(
        &mut self,
        item: &mut Value,
        walker: &mut Walker<'_>,
    ) -> Result<Option<Value>, String> {
        if self.captures == 64
            || item["kind"] != "Effect"
            || lossless::<Instruction>(item, false).is_none()
        {
            return Ok(None);
        }
        let operation = &mut item["args"]["operation"];
        let Some(slots) = immediate_values::slots(operation) else {
            return Ok(None);
        };
        let Some(first) = slots.first().and_then(|slot| operation.pointer(slot)) else {
            return Ok(None);
        };
        // Equal stats share one capture; different stats retain their own operands.
        if scalar_reads::constant(first)
            || slots
                .iter()
                .any(|slot| operation.pointer(slot) != Some(first))
        {
            return Ok(None);
        }
        let mut value = first.clone();
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
        for slot in slots {
            *operation.pointer_mut(slot).expect("checked immediate slot") =
                json!({"kind":"BoundValue","args":id});
        }
        Ok(Some(
            json!({"kind":"Effect","args":{"result":id,"operation":{
            "kind":"BindValue","args":{"value":value}}}}),
        ))
    }
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

/// At most three current-operand views: scalar captures, one-object receipts,
/// and their combination. Fresh result IDs avoid every original typed ID,
/// including nested scopes and unresolved references. Every returned view must
/// normalize to the input; callers must still verify any printed text by reparse.
pub fn print_views(value: &Value) -> Result<Vec<Value>, String> {
    let ty = Ty::Vec(Box::new(Ty::Named("Ability".into())));
    let schema = super::schema()?;
    let mut ids = Ids::default();
    Walker::new(schema).walk(&mut value.clone(), &ty, false, "", 0, &mut ids)?;
    let mut views = Vec::new();
    let mut canonical = None;
    for (scalar, choices) in [(false, true), (true, false), (true, true)] {
        let mut view = value.clone();
        let mut proof = View {
            ids: ids.0.clone(),
            next: 0,
            captures: 0,
            scalar,
            choices,
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
