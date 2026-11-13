//! Schedule program reductions with one bounded, typed use/definition proof.
use super::walk::{Uses, Visit, Walker};
use crate::ir_schema::Ty;
use serde_json::Value;

#[derive(Default)]
struct Needed {
    snapshots: bool,
    mana: bool,
}
#[derive(Default)]
struct Normalize {
    changed: bool,
    owners: Vec<Needed>,
    tracing: bool,
    keep_order: bool,
}
impl Visit for Normalize {
    fn enter(&mut self, value: &Value, ty: &Ty) {
        if matches!(ty, Ty::Named(name) if name == "Ability") {
            self.owners.push(Needed::default());
        } else if let Some(needed) = self.owners.last_mut() {
            needed.snapshots |= matches!(ty, Ty::Named(name) if name == "Operation")
                && value["kind"] == "BindValue";
            needed.mana |= matches!(ty, Ty::Named(name) if name == "Instruction")
                && value["kind"] == "ForEachPlayer";
        }
    }
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        _: bool,
        _: &str,
        walker: &mut Walker<'_>,
    ) -> Result<(), String> {
        if !self.keep_order && matches!(ty, Ty::Named(name) if name == "ManaProduction") {
            self.changed |= super::fixed_mana::bundle(value, self.tracing);
        }
        if !matches!(ty, Ty::Named(name) if name == "Ability") {
            return Ok(());
        }
        let Some(needed) = self.owners.pop() else {
            return Ok(());
        };
        if !needed.snapshots && !needed.mana {
            return Ok(());
        }
        let mut uses = Uses::default();
        walker.walk(value, ty, false, "", 0, &mut uses)?;
        // Descendant Ability scopes may both shadow and capture outer labels.
        // Leave their enclosing ability alone, independent of numeric spelling;
        // inner abilities were already processed. Delayed Programs stay in this
        // proof so their captures count as later uses.
        if uses.abilities != 1 {
            return Ok(());
        }
        for path in &uses.programs {
            if let Some(items) = value.pointer_mut(path).and_then(Value::as_array_mut) {
                if needed.snapshots {
                    self.changed |=
                        super::snapshots::inline_array(items, &uses, walker, self.tracing)?;
                }
                if needed.mana {
                    self.changed |= super::fixed_mana::flatten(items, path, &uses, self.tracing);
                }
            }
        }
        Ok(())
    }
}

pub(super) fn normalize(
    value: &mut Value,
    schema: &crate::ir_schema::Schema,
    tracing: bool,
    keep_order: bool,
) -> Result<bool, String> {
    let mut visitor = Normalize {
        tracing,
        keep_order,
        ..Normalize::default()
    };
    Walker::new(schema).walk(
        value,
        &Ty::Vec(Box::new(Ty::Named("Ability".into()))),
        false,
        "",
        0,
        &mut visitor,
    )?;
    Ok(visitor.changed)
}
