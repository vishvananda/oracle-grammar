//! Schedule program reductions with one bounded, typed use/definition proof.
use super::walk::{Uses, Visit, Walker};
use crate::ir_schema::Ty;
use serde_json::Value;

#[derive(Default)]
struct Needed {
    source_components: Vec<usize>,
    snapshots: bool,
    player_loops: bool,
    player_rule_groups: bool,
    hand_observation: bool,
    hand_choice: bool,
    native_payment: bool,
    pump_untap: bool,
    optional_receipt: bool,
}
#[derive(Default)]
struct Normalize {
    changed: bool,
    owners: Vec<Needed>,
    tracing: bool,
    keep_order: bool,
    costs: usize,
}
impl Visit for Normalize {
    fn enter(&mut self, value: &Value, ty: &Ty) {
        if matches!(ty, Ty::Named(name) if name == "Cost") {
            self.costs += 1;
        }
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
            needed.player_loops |= matches!(ty, Ty::Named(name) if name == "Instruction")
                && value["kind"] == "ForEachPlayer";
            needed.player_rule_groups |= !self.keep_order
                && matches!(ty, Ty::Named(name) if name == "Instruction")
                && super::player_rule_groups::may_need(value);
            needed.hand_observation |= matches!(ty, Ty::Named(name) if name == "Operation")
                && matches!(value["kind"].as_str(), Some("Reveal" | "LookAt"));
            needed.hand_choice |= matches!(ty, Ty::Named(name) if name == "Operation")
                && value["kind"] == "ChooseObjects";
            needed.optional_receipt |= !self.keep_order
                && matches!(ty, Ty::Named(name) if name == "Instruction")
                && super::optional_receipts::may_need(value);
            needed.pump_untap |= !self.keep_order
                && matches!(ty, Ty::Named(name) if name == "Operation")
                && super::pump_untap::may_need(value);
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
        if matches!(ty, Ty::Named(name) if name == "Cost") {
            self.costs -= 1;
        }
        if !self.keep_order
            && self.costs == 0
            && matches!(ty, Ty::Named(name) if name == "Operation")
        {
            self.changed |= super::fractional_sacrifice::normalize(value, self.tracing);
        }
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
        if !self.keep_order
            && matches!(ty, Ty::Named(name) if name == "Instruction")
            && value["kind"] == "If"
        {
            self.changed |= super::keywords::guarded_unsuspect(value, self.tracing);
        }
        if !self.keep_order && matches!(ty, Ty::Named(name) if name == "ManaProduction") {
            self.changed |= super::fixed_mana::bundle(value, self.tracing);
        }
        if !matches!(ty, Ty::Named(name) if name == "Ability") {
            return Ok(());
        }
        self.changed |= super::target_pairs::normalize(value, self.tracing, walker)?;
        if !self.keep_order {
            super::combat_pairs::delayed_bounce(value);
            super::target_mill::normalize(value, self.tracing);
        }
        let Some(needed) = self.owners.pop() else {
            return Ok(());
        };
        let hands = needed.hand_observation && needed.hand_choice;
        if !needed.snapshots
            && !needed.player_loops
            && !needed.player_rule_groups
            && !hands
            && !needed.native_payment
            && !needed.optional_receipt
            && !needed.pump_untap
        {
            return Ok(());
        }
        let mut uses = Uses::default();
        walker.walk(value, ty, false, "", 0, &mut uses)?;
        if hands || needed.native_payment {
            // Evaluate owner instructions here. Payment normalization still counts
            // every read, including any reference from a captured nested ability.
            let mut owner = Uses::within_ability();
            if uses.abilities != 1 {
                walker.walk(value, ty, false, "", 0, &mut owner)?;
            }
            let local = if uses.abilities == 1 { &uses } else { &owner };
            for path in &local.programs {
                if let Some(items) = value.pointer_mut(path).and_then(Value::as_array_mut) {
                    if hands {
                        self.changed |=
                            super::observed_hands::normalize(items, local, self.tracing);
                    }
                    if needed.native_payment {
                        self.changed |=
                            super::resolution_payments::program(items, &uses, self.tracing);
                    }
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
                    self.changed |= super::immediate_values::inline_array(
                        items,
                        &uses,
                        walker,
                        self.tracing,
                        self.keep_order,
                    )?;
                }
                if needed.player_loops {
                    self.changed |= super::fixed_mana::flatten(items, path, &uses, self.tracing);
                    if !self.keep_order {
                        self.changed |=
                            super::player_life::flatten(items, path, &uses, self.tracing);
                    }
                }
                if needed.player_rule_groups {
                    self.changed |= super::player_rule_groups::merge(items, &uses, self.tracing);
                }
                if needed.optional_receipt {
                    self.changed |= super::optional_receipts::program(items, &uses, self.tracing);
                }
                if needed.pump_untap {
                    self.changed |= super::pump_untap::program(items, &uses, self.tracing);
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
