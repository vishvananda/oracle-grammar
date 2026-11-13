//! An outer where clause cannot inline a shared definition inside a branch.
use serde_json::Value;

pub(super) fn required(body: &Value, amount: &Value) -> Result<bool, String> {
    let shared_definition = super::shared_definition(amount);
    let mut pending = vec![(body, 0_usize)];
    let mut remaining = 131_072_usize;
    while let Some((value, depth)) = pending.pop() {
        if remaining == 0 || depth > 128 {
            return Err("where capture scope work/depth limit".into());
        }
        remaining -= 1;
        if depth != 0
            && matches!(
                crate::traverse::boundary(value),
                crate::traverse::Boundary::Ability
                    | crate::traverse::Boundary::Holder
                    | crate::traverse::Boundary::Lexical
                    | crate::traverse::Boundary::Reflexive
                    | crate::traverse::Boundary::Mode
            )
        {
            continue;
        }
        if super::observed::repeated(value) || shared_definition && super::shared(value) {
            return Ok(true);
        }
        let count = match value {
            Value::Object(fields) => fields.len(),
            Value::Array(items) => items.len(),
            _ => 0,
        };
        if pending.len().saturating_add(count) > remaining {
            return Err("where capture scope work/depth limit".into());
        }
        match value {
            Value::Object(fields) => pending.extend(fields.values().map(|v| (v, depth + 1))),
            Value::Array(items) => pending.extend(items.iter().map(|v| (v, depth + 1))),
            _ => {}
        }
    }
    Ok(false)
}
