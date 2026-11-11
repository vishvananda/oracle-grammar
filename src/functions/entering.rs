//! Source-entry replacement "you" uses the incoming prospective controller.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

fn controller() -> Value {
    json!({"kind":"ControllerOf","args":{"kind":"EventSubject"}})
}

fn walk(value: &mut Value, inverse: bool) {
    if value["kind"] == "AbilityController" && !inverse {
        *value = controller();
        return;
    }
    if inverse && value["kind"] == "ControllerOf" && value["args"] == json!({"kind":"EventSubject"})
    {
        *value = json!({"kind":"AbilityController"});
        return;
    }
    if !inverse && value["$sym"] == "subject" {
        *value = json!({"kind":"One","args":controller()});
        return;
    }
    // New ability holders/owning frames keep their own meaning of "you".
    // Scope markers for lexical programs do not themselves introduce a holder.
    if value["$sym"] == "holder"
        || matches!(
            crate::traverse::boundary(value),
            crate::traverse::Boundary::Ability
        )
        || value["kind"] == "GrantAbility"
    {
        return;
    }
    if matches!(
        value["kind"].as_str(),
        Some("CreateTrigger" | "CreateReplacement")
    ) {
        for field in ["event", "duration"] {
            if let Some(header) = value["args"].get_mut(field) {
                walk(header, inverse);
            }
        }
        return;
    }
    crate::traverse::children(value, |child| walk(child, inverse));
}

fn transform(value: &Value, inverse: bool) -> Result<Value, String> {
    if value["kind"] != "Replacement" || value["args"]["active"] != "SourceEntering" {
        return Err("@entering_controller needs a source-entry replacement".into());
    }
    super::branch_bounds(value)?;
    let mut output = value.clone();
    walk(&mut output["args"]["body"], inverse);
    Ok(output)
}

pub(crate) fn bind(value: &Value) -> Result<Value, String> {
    transform(value, false)
}

fn same_native_meaning(left: &Value, right: &Value) -> bool {
    let mut left = json!([left]);
    let mut right = json!([right]);
    crate::model::canonical::abilities(&mut left).is_ok()
        && crate::model::canonical::abilities(&mut right).is_ok()
        && left == right
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else { return Vec::new() };
    let Ok(surface) = transform(value, true) else {
        return Vec::new();
    };
    // Native canonicalization reduces the incoming controller outside event
    // headers/closures. Check the whole owning frame through that same native
    // implementation, including unbinding markers, rather than a local alias.
    // Edited history/closure operands still cannot silently print as entry "you".
    let mut out = Vec::new();
    if bind(value).is_ok_and(|bound| same_native_meaning(&bound, value)) {
        out.extend(inner.unify(value, bindings.clone()));
    }
    if surface != *value && bind(&surface).is_ok_and(|bound| same_native_meaning(&bound, value)) {
        out.extend(inner.unify_made(&surface, &bindings));
    }
    out
}
