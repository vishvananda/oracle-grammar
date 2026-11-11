//! Conservatively identify unused definitions without depending on canonical ids.
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn unread(value: &Value) -> BTreeSet<u64> {
    let mut defined = BTreeSet::new();
    let mut mentioned = BTreeSet::new();
    collect(value, &mut defined, &mut mentioned);
    defined.difference(&mentioned).copied().collect()
}

fn collect(value: &Value, defined: &mut BTreeSet<u64>, mentioned: &mut BTreeSet<u64>) {
    match value {
        Value::Object(fields) if value["kind"] == "Effect" => {
            if let Some(id) = value.pointer("/args/result").and_then(Value::as_u64) {
                defined.insert(id);
            }
            if let Some(args) = value["args"].as_object() {
                for (field, item) in args {
                    if field != "result" {
                        collect(item, defined, mentioned);
                    }
                }
            }
            for (field, item) in fields {
                if field != "args" {
                    collect(item, defined, mentioned);
                }
            }
        }
        Value::Object(fields) => {
            for item in fields.values() {
                collect(item, defined, mentioned);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect(item, defined, mentioned);
            }
        }
        Value::Number(number) => {
            if let Some(id) = number.as_u64() {
                // Any occurrence outside a definition might be a read. Numeric
                // literals and nested scopes can only make this conservative.
                mentioned.insert(id);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn descending_ids_are_definitions_but_references_and_literal_collisions_are_kept() {
        let value = json!([
            {"kind":"Effect","args":{"result":4294967295_u64,"operation":{"kind":"Draw"}}},
            {"kind":"Effect","args":{"result":4294967294_u64,"operation":{"kind":"Draw"}}},
            {"kind":"Effect","args":{"result":42,"operation":{"kind":"Draw","args":{"amount":42}}}},
            {"kind":"Effect","args":{"result":17,"operation":{"kind":"Reveal","args":{
                "objects":{"kind":"Transition","args":{"result":17,"phase":"After"}}}}}}
        ]);
        assert_eq!(unread(&value), BTreeSet::from([4294967295, 4294967294]));
    }
}
