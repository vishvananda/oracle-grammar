//! A hole a pattern names twice holds one value. Canonical IR gives each
//! definition nobody reads its own id, counting down from `u32::MAX`, so two
//! copies of one captured program differ in exactly those ids. They are still
//! the same capture; reparse proves whatever text prints from the first.

use serde_json::Value;

/// How deep two held values are compared before they count as different.
const DEPTH: usize = 128;

/// Whether `held` and `again` differ at most in the ids of unread results.
pub(super) fn same(held: &Value, again: &Value) -> bool {
    alike(held, again, DEPTH)
}

fn alike(held: &Value, again: &Value, depth: usize) -> bool {
    let Some(depth) = depth.checked_sub(1) else {
        return false;
    };
    match (held, again) {
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right.get(key).is_some_and(|other| {
                        key == "result" && unread(value) && unread(other)
                            || alike(value, other, depth)
                    })
                })
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(value, other)| alike(value, other, depth))
        }
        _ => held == again,
    }
}

/// An id the canonical form gives a result nothing reads.
fn unread(id: &Value) -> bool {
    id.as_u64()
        .is_some_and(|id| id > u64::from(u32::MAX / 2) && id <= u64::from(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn draw(result: u64) -> Value {
        json!({"instructions":[{"kind":"Effect","args":{"result":result,
            "operation":{"kind":"Draw","args":{"amount":{"kind":"Constant","args":1}}}}}]})
    }

    #[test]
    fn copies_of_one_program_differ_only_in_unread_result_ids() {
        assert!(same(&draw(4_294_967_294), &draw(4_294_967_293)));
        assert!(!same(&draw(0), &draw(1)));
        assert!(!same(&draw(4_294_967_294), &draw(2)));
        let mut other = draw(4_294_967_293);
        other["instructions"][0]["args"]["operation"]["args"]["amount"]["args"] = json!(2);
        assert!(!same(&draw(4_294_967_294), &other));
    }

    #[test]
    fn comparison_stops_at_its_depth_bound() {
        let mut deep = json!(1);
        for _ in 0..DEPTH {
            deep = json!([deep]);
        }
        assert!(!same(&deep, &deep.clone()));
    }
}
