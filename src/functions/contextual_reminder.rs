//! A closed explanatory sentence needs a matching operand in its own ability.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

const EXPLANATIONS: [&str; 3] = ["Stun", "Modified", "Historic"];

fn mentions(value: &Value, explanation: &str) -> bool {
    fn matches(value: &Value, explanation: &str) -> bool {
        match explanation {
            "Stun" => value["Named"] == "StunCounter",
            "Modified" => {
                value["kind"] == "Modified"
                    && value.as_object().is_some_and(|fields| fields.len() == 1)
            }
            "Historic" => {
                value["kind"] == "AnyOf"
                    && value["args"].as_array().is_some_and(|items| {
                        items.len() == 3
                            && items
                                .iter()
                                .any(|i| i["kind"] == "CardType" && i["args"] == "Artifact")
                            && items
                                .iter()
                                .any(|i| i["kind"] == "Supertype" && i["args"] == "Legendary")
                            && items
                                .iter()
                                .any(|i| i["kind"] == "Subtype" && i["args"] == "Saga")
                    })
            }
            _ => false,
        }
    }

    fn walk(value: &Value, explanation: &str, depth: usize, remaining: &mut usize) -> bool {
        if *remaining == 0 {
            return false;
        }
        *remaining -= 1;
        if value.is_object() && matches(value, explanation) {
            return true;
        }
        if depth == 80 {
            return false;
        }
        match value {
            Value::Object(fields) => walk_children(fields.values(), explanation, depth, remaining),
            Value::Array(items) => walk_children(items.iter(), explanation, depth, remaining),
            _ => false,
        }
    }

    fn walk_children<'a>(
        children: impl Iterator<Item = &'a Value>,
        explanation: &str,
        depth: usize,
        remaining: &mut usize,
    ) -> bool {
        for child in children {
            if *remaining == 0 {
                break;
            }
            if walk(child, explanation, depth + 1, remaining) {
                return true;
            }
        }
        false
    }

    walk(value, explanation, 0, &mut 8192)
}

pub(crate) fn attach(ability: &Value, explanation: &Value) -> Result<Value, String> {
    if !explanation
        .as_str()
        .is_some_and(|kind| mentions(ability, kind))
    {
        return Err("reminder has no matching operand in this ability".into());
    }
    if ability["kind"] == "WithPresentation" {
        let mut attached = ability.clone();
        attached["args"]["reminder"] = Value::Bool(true);
        Ok(attached)
    } else {
        Ok(
            json!({"kind":"WithPresentation","args":{"ability":ability,"label":null,"reminder":true}}),
        )
    }
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [ability, explanation] = args else {
        return Vec::new();
    };
    if value["kind"] != "WithPresentation" || value["args"]["reminder"] != true {
        return Vec::new();
    }
    let inner = &value["args"]["ability"];
    let mut without_reminder = value.clone();
    without_reminder["args"]["reminder"] = Value::Bool(false);
    let source = if value["args"]["label"].is_null() {
        inner
    } else {
        &without_reminder
    };
    let mut solutions = Vec::new();
    for kind in EXPLANATIONS {
        if mentions(inner, kind) {
            for matched in explanation.unify_made(&Value::from(kind), &bindings) {
                solutions.extend(ability.unify_made(source, &matched));
                solutions.extend(ability.unify(value, matched));
            }
        }
    }
    solutions
}
