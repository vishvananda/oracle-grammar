//! Availability is outside the granted ability's own binding/result namespace.
use super::{Scope, bind_scope, holder, scope_for};
use serde_json::Value;

pub(super) fn bind(node: &mut Value, outer: &Scope) -> Result<Value, String> {
    crate::functions::branch_bounds(node)?;
    let args = node["args"]
        .as_array_mut()
        .ok_or("conditional ability arguments")?;
    let [mut condition, mut ability]: [Value; 2] = std::mem::take(args)
        .try_into()
        .map_err(|_| "conditional ability arity")?;
    bind_scope(&mut condition, &mut outer.nested())?;
    let mut core = &mut ability;
    while matches!(core["kind"].as_str(), Some("Scoped" | "WithPresentation")) {
        core = &mut core["args"]["ability"];
    }
    let departure_controller = super::departure_controller::prepare(core)?;
    holder::event_sources(core, outer.holders)?;
    let mut scope = scope_for(core, &outer.card_name, outer.holders);
    super::departure_controller::seed(&mut scope, departure_controller);
    scope.binding_floor = outer.binding_floor;
    bind_scope(&mut ability, &mut scope)?;
    Ok(crate::functions::conditional::wrap(condition, ability))
}
