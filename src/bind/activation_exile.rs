//! A literal exiled-card reference first names this activation's paid card.
use super::{Step, at_path};
use crate::model::ir::{
    Cost, CostObjectView, CostPart, MovementDestination, ObjectPhase, ObjectRef, ObjectSet,
    PaymentRef,
};
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

pub(super) const NAME: &str = "activation_exiled_card";

pub(super) fn marker(root: &mut Value, path: &[Step], name: &str) -> Result<bool, String> {
    if name != NAME {
        return Ok(false);
    }
    let fallback = at_path(root, path).ok_or("activation exile reference path")?["args"]
        .get(0)
        .ok_or("activation exile needs a fallback")?
        .clone();
    let in_body = matches!(path, [Step::Key(args), Step::Key(body), ..]
        if args == "args" && body == "body");
    let value = if in_body {
        reference(root, fallback)?
    } else {
        fallback
    };
    *at_path(root, path).ok_or("activation exile reference path")? = value;
    Ok(true)
}

fn reference(root: &Value, fallback: Value) -> Result<Value, String> {
    if root["kind"] != "Activated" {
        return Ok(fallback);
    }
    let mut raw = root["args"]["cost"].clone();
    crate::functions::branch_bounds(&raw)?;
    super::filters::strip(&mut raw);
    let cost: Cost =
        serde_json::from_value(raw).map_err(|e| format!("activation exile cost: {e}"))?;
    let mut found = None;
    for (_, component) in crate::model::cost_history::components(&cost) {
        let Some(part) = crate::model::cost_history::component(&cost, &component) else {
            continue;
        };
        if !matches!(
            part,
            CostPart::Move {
                destination: MovementDestination::Exile { .. },
                ..
            }
        ) {
            continue;
        }
        if found.is_some() || !crate::model::cost_history::singular(part) {
            return Err("the exiled card requires one singular activation exile component".into());
        }
        found = Some(ObjectRef::Query {
            objects: Box::new(ObjectSet::CostObjects {
                payment: PaymentRef::Activation,
                component,
                view: CostObjectView::Performed,
                phase: ObjectPhase::After,
            }),
            index: 0,
        });
    }
    found.map_or(Ok(fallback), |value| {
        serde_json::to_value(value).map_err(|e| e.to_string())
    })
}

/// The grammar's own fallback supplies the spelling; the actual cost address
/// must survive semantic reparse. No source text or fixed component is retained.
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else { return Vec::new() };
    let mut out = inner.unify(value, bindings.clone());
    if crate::functions::branch_bounds(value).is_err() {
        return out;
    }
    let Ok(reference) = serde_json::from_value::<ObjectRef>(value.clone()) else {
        return out;
    };
    if serde_json::to_value(&reference).ok().as_ref() != Some(value) {
        return out;
    }
    if matches!(reference, ObjectRef::Query { objects, index: 0 } if matches!(*objects,
        ObjectSet::CostObjects { payment: PaymentRef::Activation, view: CostObjectView::Performed, phase: ObjectPhase::After, .. }))
        && let Ok(fallback) = inner.fill(&Bindings::new(), 0)
    {
        out.extend(inner.unify_made(&fallback, &bindings));
    }
    out
}
