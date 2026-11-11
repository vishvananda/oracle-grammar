//! A private inverse view carries leading-amount context to the literal stat.
//! It retains the actual Granter operand; ordinary pronoun eligibility is unchanged.
use super::{Bindings, Pattern};
use serde_json::{Value, json};

const VIEW: &str = "leading_damage_actor_stat_view";

pub(super) fn is_view(value: &Value) -> bool {
    value["$sym"] == VIEW
}

fn bare_granter(value: &Value) -> bool {
    value.as_object().is_some_and(|v| v.len() == 2)
        && value["kind"] == "ObjectProperty"
        && value["args"].as_object().is_some_and(|v| v.len() == 3)
        && value["args"]["object"] == json!({"kind":"GrantingObject"})
        && matches!(
            value["args"]["property"].as_str(),
            Some("Power" | "Toughness")
        )
        && value["args"]["information"] == "CurrentOrLastKnown"
}

pub(super) fn leading<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else {
        return Vec::new();
    };
    // Preserve every ordinary inverse solution and its existing order.
    let mut found = inner.unify(value, bindings.clone());
    if bare_granter(value)
        && matches!(inner, Pattern::Hole(_, crate::model::ir_schema::Ty::Named(name)) if name == "Expr")
    {
        // The declared Expr hole has exactly one binding for this constant-size
        // view. No candidate truncation or arbitrary expression walk is needed.
        let marked = json!({"$sym":VIEW,"args":[value]});
        found.extend(inner.unify_made(&marked, &bindings));
    }
    found
}

pub(super) fn literal<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else {
        return Vec::new();
    };
    if !is_view(value) {
        return inner.unify(value, bindings);
    }
    if !value.as_object().is_some_and(|v| v.len() == 2) {
        return Vec::new();
    }
    let Some([actual]) = value["args"].as_array().map(Vec::as_slice) else {
        return Vec::new();
    };
    if !bare_granter(actual) {
        return Vec::new();
    }
    let Pattern::Object(fields) = inner else {
        return Vec::new();
    };
    if fields.len() != 2 {
        return Vec::new();
    }
    let mut solutions = vec![bindings];
    for (name, pattern) in fields {
        let Some(field) = actual.get(name) else {
            return Vec::new();
        };
        if name == "args" {
            let Pattern::Object(parts) = pattern else {
                return Vec::new();
            };
            if parts.len() != 3 {
                return Vec::new();
            }
            for (name, pattern) in parts {
                let Some(field) = field.get(name) else {
                    return Vec::new();
                };
                if name == "object" {
                    // Only the literal @it leaf receives this extra spelling
                    // candidate. No operand is replaced with Source or erased.
                    if !matches!(pattern, Pattern::Symbol(n, a) if n == "it" && a.is_empty()) {
                        return Vec::new();
                    }
                } else {
                    solutions = solutions
                        .into_iter()
                        .flat_map(|b| pattern.unify(field, b))
                        .collect();
                }
            }
        } else {
            solutions = solutions
                .into_iter()
                .flat_map(|b| pattern.unify(field, b))
                .collect();
        }
    }
    solutions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ir_schema::Ty;

    fn property(stat: &str) -> Value {
        json!({"kind":"ObjectProperty","args":{"object":{"kind":"GrantingObject"},
            "property":stat,"information":"CurrentOrLastKnown"}})
    }
    fn pattern() -> Pattern {
        Pattern::Object(vec![
            ("kind".into(), Pattern::Value(json!("ObjectProperty"))),
            (
                "args".into(),
                Pattern::Object(vec![
                    ("object".into(), Pattern::Symbol("it".into(), vec![])),
                    (
                        "property".into(),
                        Pattern::Hole("stat".into(), Ty::Named("Property".into())),
                    ),
                    (
                        "information".into(),
                        Pattern::Hole("timing".into(), Ty::Named("Information".into())),
                    ),
                ]),
            ),
        ])
    }

    #[test]
    fn only_leading_literal_view_admits_granter_and_keeps_holes() {
        let hole = [Pattern::Hole("value".into(), Ty::Named("Expr".into()))];
        let literal_pattern = [pattern()];
        for stat in ["Power", "Toughness"] {
            let value = property(stat);
            assert!(literal(&literal_pattern, &value, Bindings::new()).is_empty());
            let marked = leading(&hole, &value, Bindings::new());
            assert_eq!(marked.len(), 2);
            assert_eq!(marked[0].get("value"), Some(&value));
            let view = marked[1].get("value").unwrap();
            assert_eq!(view["args"][0], value);
            let found = literal(&literal_pattern, view, Bindings::new());
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].get("stat"), Some(&json!(stat)));
            assert_eq!(found[0].get("timing"), Some(&json!("CurrentOrLastKnown")));
        }
    }

    #[test]
    fn explicit_reference_and_mismatched_property_timing_do_not_take_view() {
        let mut explicit = pattern();
        if let Pattern::Object(fields) = &mut explicit
            && let Pattern::Object(parts) = &mut fields[1].1
        {
            parts[0].1 = Pattern::Symbol("named_object".into(), vec![]);
        }
        let marked = json!({"$sym":VIEW,"args":[property("Power")]});
        assert!(literal(&[explicit], &marked, Bindings::new()).is_empty());
        for field in ["property", "information"] {
            let mut actual = property("Power");
            actual["args"][field] = json!("foreign");
            let marked = json!({"$sym":VIEW,"args":[actual]});
            assert!(literal(&[pattern()], &marked, Bindings::new()).is_empty());
        }
        let mut fixed = pattern();
        if let Pattern::Object(fields) = &mut fixed
            && let Pattern::Object(parts) = &mut fields[1].1
        {
            parts[1].1 = Pattern::Value(json!("Toughness"));
        }
        assert!(literal(&[fixed], &marked, Bindings::new()).is_empty());
    }

    #[test]
    fn composites_and_foreign_metadata_are_never_actor_stat_views() {
        let hole = [Pattern::Hole("value".into(), Ty::Named("Expr".into()))];
        let composite =
            json!({"kind":"Add","args":[property("Power"),{"kind":"Constant","args":1}]});
        let found = leading(&hole, &composite, Bindings::new());
        assert_eq!(found[0].get("value"), Some(&composite));
        let foreign = json!({"$sym":"foreign","args":[property("Power")]});
        assert!(literal(&[pattern()], &foreign, Bindings::new()).is_empty());
    }
    #[test]
    fn ordinary_explicit_named_source_inverse_is_retained_first() {
        let hole = [Pattern::Hole("value".into(), Ty::Named("Expr".into()))];
        let mut explicit = pattern();
        if let Pattern::Object(fields) = &mut explicit
            && let Pattern::Object(parts) = &mut fields[1].1
        {
            parts[0].1 = Pattern::Symbol("named_source".into(), vec![]);
        }
        let actual = property("Power");
        let original = hole[0].unify(&actual, Bindings::new());
        let extended = leading(&hole, &actual, Bindings::new());
        assert_eq!(original.len(), 1);
        assert_eq!(extended.len(), 2);
        assert_eq!(extended[0].get("value"), original[0].get("value"));
        assert!(
            !explicit
                .unify(extended[0].get("value").unwrap(), Bindings::new())
                .is_empty()
        );
        assert!(
            explicit
                .unify(extended[1].get("value").unwrap(), Bindings::new())
                .is_empty()
        );
    }
}
