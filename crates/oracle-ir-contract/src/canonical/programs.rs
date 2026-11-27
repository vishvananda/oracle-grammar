//! Schedule program reductions with one bounded, typed use/definition proof.
use super::walk::{Uses, Visit, Walker};
use crate::ir_schema::Ty;
use serde_json::Value;

#[derive(Default)]
struct Needed {
    source_components: Vec<usize>,
    snapshots: bool,
    mana: bool,
    hand_reveal: bool,
    hand_choice: bool,
    native_payment: bool,
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
            self.owners.push(Needed {
                source_components: if self.keep_order {
                    Vec::new()
                } else {
                    super::paid_source::components(value)
                },
                ..Needed::default()
            });
        } else if let Some(needed) = self.owners.last_mut() {
            needed.snapshots |= matches!(ty, Ty::Named(name) if name == "Operation")
                && value["kind"] == "BindValue";
            needed.mana |= matches!(ty, Ty::Named(name) if name == "Instruction")
                && value["kind"] == "ForEachPlayer";
            needed.hand_reveal |=
                matches!(ty, Ty::Named(name) if name == "Operation") && value["kind"] == "Reveal";
            needed.hand_choice |= matches!(ty, Ty::Named(name) if name == "Operation")
                && value["kind"] == "ChooseObjects";
            needed.native_payment |= !self.keep_order
                && matches!(ty, Ty::Named(name) if name == "CostPart")
                && super::resolution_payments::may_need(value);
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
        if !self.keep_order
            && matches!(ty, Ty::Named(name) if name == "ObjectRef")
            && let Some(owner) = self.owners.last()
        {
            self.changed |= super::paid_source::reference(value, &owner.source_components);
        }
        if matches!(ty, Ty::Named(name) if name == "Program") {
            self.changed |= super::observed_top::program(value, self.tracing)?;
            if !self.keep_order {
                self.changed |= super::single_choices::program(value, self.tracing);
            }
        }
        if !self.keep_order && matches!(ty, Ty::Named(name) if name == "ManaProduction") {
            self.changed |= super::fixed_mana::bundle(value, self.tracing);
        }
        if !matches!(ty, Ty::Named(name) if name == "Ability") {
            return Ok(());
        }
        self.changed |= super::target_pairs::normalize(value, self.tracing, walker)?;
        let Some(needed) = self.owners.pop() else {
            return Ok(());
        };
        let hands = needed.hand_reveal && needed.hand_choice;
        if !needed.snapshots && !needed.mana && !hands && !needed.native_payment {
            return Ok(());
        }
        let mut uses = Uses::default();
        walker.walk(value, ty, false, "", 0, &mut uses)?;
        if hands {
            // A created token's independent ability can reuse an unread result
            // label. It cannot erase the surrounding hand-observation proof.
            let mut owner = Uses::within_ability();
            if uses.abilities != 1 {
                walker.walk(value, ty, false, "", 0, &mut owner)?;
            }
            let local = if uses.abilities == 1 { &uses } else { &owner };
            for path in &local.programs {
                if let Some(items) = value.pointer_mut(path).and_then(Value::as_array_mut) {
                    self.changed |= super::observed_hands::normalize(items, local, self.tracing);
                }
            }
        }
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
                    if !self.keep_order {
                        self.changed |= super::immediate_values::inline_array(
                            items,
                            &uses,
                            walker,
                            self.tracing,
                        )?;
                    }
                }
                if needed.mana {
                    self.changed |= super::fixed_mana::flatten(items, path, &uses, self.tracing);
                }
                if needed.native_payment {
                    self.changed |= super::resolution_payments::program(items, &uses, self.tracing);
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
