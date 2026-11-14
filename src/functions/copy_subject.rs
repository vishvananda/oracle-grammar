//! A lexical "other" population excludes the object explicitly being copied.
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

/// Legacy literal-other annotations exist only after the full @sem is filled.
/// Resolve this local lens before reference collection fixes binder paths.
pub(crate) fn prepare(root: &mut Value) -> Result<(), String> {
    super::branch_bounds(root)?;
    fn walk(value: &mut Value, depth: usize, work: &mut usize) -> Result<(), String> {
        if depth > 128 || *work == 0 {
            return Err("copy subject preparation work/depth limit".into());
        }
        *work -= 1;
        if value["$sym"] == "copy_subject" {
            let Some([subject, copied]) = value["args"].as_array().map(Vec::as_slice) else {
                return Err("copy subject needs its population and copied object".into());
            };
            *value = lower(subject, copied)?;
        }
        match value {
            Value::Object(fields) => {
                for child in fields.values_mut() {
                    walk(child, depth + 1, work)?;
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(child, depth + 1, work)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    walk(root, 0, &mut 131_072)
}

fn lower(subject: &Value, copied: &Value) -> Result<Value, String> {
    super::branch_bounds(subject)?;
    super::branch_bounds(copied)?;
    let mut out = subject.clone();
    let set = if out["$sym"] == "distributive_pump_subject" {
        out["args"]
            .get_mut(0)
            .ok_or("copy subject annotation needs its population")?
    } else {
        &mut out
    };
    if set["kind"] != "Zone"
        || set["args"]["scope"] != serde_json::json!({"Shared":"Battlefield"})
        || set["args"]["kind"] != "Permanent"
        || set["args"]["filter"]["kind"] != "AllOf"
    {
        return Ok(out);
    }
    if let Some(parts) = set["args"]["filter"]["args"].as_array_mut() {
        for part in parts {
            if part["kind"] != "Not" || part["args"]["kind"] != "SameObject" {
                continue;
            }
            let reference = &mut part["args"]["args"];
            let pronoun = reference["$sym"] == "pump_other_ref"
                && reference["args"].as_array().is_some_and(|args| {
                    matches!(args.as_slice(), [inner] if inner["$sym"] == "it"
                        && inner["args"].as_array().is_some_and(Vec::is_empty))
                });
            if pronoun || crate::bind::target_contrast::lexical_other(reference) {
                *reference = copied.clone();
            }
        }
    }
    Ok(out)
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [subject, _copied] = args else {
        return Vec::new();
    };
    // The copied operand is independently unified by the same @sem's CopyOf
    // change. Both current operands must survive verified whole-ability reparse.
    subject.unify(value, bindings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn population(reference: Value) -> Value {
        json!({"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent",
            "filter":{"kind":"AllOf","args":[{"kind":"CardType","args":"Creature"},
                {"kind":"Not","args":{"kind":"SameObject","args":reference}}]}}})
    }

    #[test]
    fn copied_target_resolves_each_lexical_other_route_but_not_explicit_exclusions() {
        let source = json!({"kind":"Source"});
        let target = json!({"kind":"Target","args":{"slot":3,"index":0}});
        let expected = population(target.clone());
        for reference in [
            json!({"$sym":"pump_other_ref","args":[{"$sym":"it","args":[],"at":1}],"at":1}),
            json!({"$sym":"target_contrast","args":[{"$sym":"pump_other_ref","args":[{"$sym":"move_after_other","args":[source],"at":1}],"at":1},{"kind":"CardType","args":"Creature"}],"at":1}),
        ] {
            assert_eq!(lower(&population(reference), &target).unwrap(), expected);
        }
        let explicit = population(source);
        assert_eq!(lower(&explicit, &target).unwrap(), explicit);
    }
}
