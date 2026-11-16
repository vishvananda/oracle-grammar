//! Print-only inverse of the exact Matching(objects, Any) identity. The
//! grammar's declaration remains authoritative; complete reparse is required.
use super::{Bindings, Pattern};
use serde_json::{Value, json};

pub(super) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [objects, filter] = args else {
        return Vec::new();
    };
    if value["kind"] == "Matching"
        && value.as_object().is_some_and(|fields| fields.len() == 2)
        && value["args"].as_object().is_some_and(|fields| {
            fields.len() == 2 && fields.contains_key("objects") && fields.contains_key("filter")
        })
    {
        let mut found = Vec::new();
        for binding in objects.unify(&value["args"]["objects"], bindings.clone()) {
            found.extend(filter.unify(&value["args"]["filter"], binding));
            if found.len() > 128 {
                return Vec::new();
            }
        }
        if !found.is_empty() {
            return found;
        }
    }
    let mut found = Vec::new();
    for binding in filter.unify_made(&json!({"kind":"Any"}), &bindings) {
        found.extend(objects.unify(value, binding));
        if found.len() > 128 {
            return Vec::new();
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ir_schema::Ty;

    #[test]
    fn only_a_proved_vacuous_predicate_can_be_restored() {
        let query = json!({"kind":"Observed","args":7});
        let hole = Pattern::Hole("filter".into(), Ty::Named("ObjectFilter".into()));
        let args = [Pattern::Value(query.clone()), hole];
        let found = unify(&args, &query, Bindings::new());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].get("filter"), Some(&json!({"kind":"Any"})));
        let args = [
            Pattern::Value(query.clone()),
            Pattern::Value(json!({"kind":"Token"})),
        ];
        assert!(unify(&args, &query, Bindings::new()).is_empty());
        let wrapped = json!({"kind":"Matching","args":{"objects":query,"filter":{"kind":"Token"}}});
        assert_eq!(unify(&args, &wrapped, Bindings::new()).len(), 1);
        let mut invalid = wrapped;
        invalid["extra"] = json!(true);
        assert!(unify(&args, &invalid, Bindings::new()).is_empty());
    }
}
