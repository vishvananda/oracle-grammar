//! Distinguish fresh lexical target declarations from already captured operands.
//! These guards never change instructions or infer a target from native integers.
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

fn introduced(value: &Value) -> Result<bool, String> {
    crate::functions::branch_bounds(value)?;
    let mut pending = vec![value];
    while let Some(node) = pending.pop() {
        match node["$sym"].as_str() {
            Some("target") => return Ok(true),
            Some("scope") if node["args"][0]["kind"] == "CreateTrigger" => {
                let args = &node["args"][0]["args"];
                pending.extend([&args["event"], &args["duration"]]);
                continue;
            }
            Some("scope" | "holder" | "reflexive") => continue,
            _ => {}
        }
        // The creator owns a created trigger's header targets. Its future body,
        // and a granted ability's body, have their own announcement boundary.
        match node["kind"].as_str() {
            Some("CreateTrigger") => {
                pending.extend([&node["args"]["event"], &node["args"]["duration"]]);
            }
            Some("Triggered" | "Activated" | "Spell" | "ReflexiveTrigger") => {}
            _ => match node {
                Value::Array(items) => pending.extend(items),
                Value::Object(fields) => pending.extend(fields.values()),
                _ => {}
            },
        }
    }
    Ok(false)
}

/// EBNF selects two disjoint readings of a leading time clause: fresh targets
/// belong to the later trigger; a target-free body captures existing operands.
/// Inverse matching is transparent because native integers do not record their
/// lexical introduction. Normal target spelling plus full reparse verifies it.
pub(crate) fn guard(value: &Value, announced: bool) -> Result<Value, String> {
    if !value.is_array() {
        return Err("target scope guard needs instructions".into());
    }
    if introduced(value)? != announced {
        return Err("leading delayed clause uses a different target announcement scope".into());
    }
    Ok(value.clone())
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    match args {
        [inner] => inner.unify(value, bindings),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn declaration() -> Value {
        json!({"kind":"Target","args":{"slot":{"$sym":"target","args":[{}],"at":9},"index":0}})
    }

    #[test]
    fn fresh_declarations_and_captured_integer_references_are_disjoint() {
        let captured = json!([{"kind":"Target","args":{"slot":3,"index":0}}]);
        assert!(guard(&captured, false).is_ok());
        assert!(guard(&captured, true).is_err());
        let fresh = json!([declaration()]);
        assert!(guard(&fresh, true).is_ok());
        assert!(guard(&fresh, false).is_err());
    }

    #[test]
    fn nested_bodies_own_their_targets_but_created_headers_belong_to_creator() {
        for name in ["scope", "holder", "reflexive"] {
            let nested = json!([{"$sym":name,"args":[declaration()],"at":4}]);
            assert!(guard(&nested, false).is_ok());
        }
        let mut trigger = json!([{"$sym":"scope","args":[{"kind":"CreateTrigger","args":{
            "event":{},"duration":"EndOfTurn","announcement":{},"body":[declaration()]
        }}],"at":4}]);
        assert!(guard(&trigger, false).is_ok());
        trigger[0]["args"][0]["args"]["event"] = declaration();
        assert!(guard(&trigger, true).is_ok());
    }

    #[test]
    fn leading_land_target_is_announced_later_and_keeps_the_earlier_creature() {
        let case: Vec<Value> =
            serde_json::from_str(include_str!("../../tests/fixtures/shared-clauses-16.json"))
                .unwrap();
        let case = &case[0];
        let engine = crate::test_support::engine();
        let actual = engine
            .parse(
                case["name"].as_str().unwrap(),
                case["text"].as_str().unwrap(),
            )
            .unwrap();
        assert_eq!(actual.readings.len(), 1);
        assert_eq!(
            actual.readings[0],
            crate::engine::canonical_abilities(&case["expected"]).unwrap()
        );
    }
}
