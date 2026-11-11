//! The reference context a filter operand establishes. Resolve it after pattern
//! functions (notably `@where`) have substituted their expressions.
use super::symbol_name;
use serde_json::Value;

/// A closed operand needs no marker. Keep the marker around unresolved pronouns
/// and X, which a surrounding `@where` can still replace with such a pronoun.
pub(crate) fn operand(value: &Value) -> Value {
    if needs_context(value) {
        serde_json::json!({"$sym":"in_filter","args":[value]})
    } else {
        value.clone()
    }
}

fn needs_context(value: &Value) -> bool {
    match symbol_name(value) {
        Some("it") => return true,
        Some("in_filter" | "holder" | "scope" | "reflexive") => return false,
        _ => {}
    }
    if value["kind"] == "AnnouncedX" {
        return true;
    }
    match value {
        Value::Object(fields) => fields.values().any(needs_context),
        Value::Array(items) => items.iter().any(needs_context),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Context {
    Outside,
    Scalar,
    Query,
}

impl Context {
    pub(super) fn enter(self, value: &Value, symbol: Option<&str>) -> Self {
        match symbol {
            Some("in_filter") => return Self::Scalar,
            Some("holder" | "scope" | "reflexive") => return Self::Outside,
            Some("target") => return Self::Outside,
            _ => {}
        }
        if self == Self::Outside {
            return self;
        }
        let ability = matches!(
            value["kind"].as_str(),
            Some("Spell" | "Activated" | "Triggered" | "ReflexiveTrigger")
        ) && value["args"].get("announcement").is_some();
        if ability {
            return Self::Outside;
        }
        self
    }

    /// A query's domain is evaluated before its predicate introduces a new
    /// candidate. Its zone owner can still name the enclosing candidate.
    pub(super) fn child(self, _parent: &Value, field: &str) -> Self {
        if self == Self::Scalar && field == "filter" {
            return Self::Query;
        }
        self
    }
}

/// Context markers carry no game meaning or spelling in the final IR.
pub(super) fn strip(value: &mut Value) {
    while matches!(
        symbol_name(value),
        Some(
            "member_count_it"
                | "literal_damage_itself"
                | "literal_damage_itself_objects"
                | "indefinite_opponent_controller"
                | "in_filter"
                | "damage_actor_value"
                | "its_stat"
                | "fight_prior_counterpart"
                | "reflexive_moved_fight_object"
                | "move_after_current_type"
                | "move_after_actor"
                | "move_after_property"
                | "move_after_other"
                | "distributive_pump_subject"
                | "pump_other_ref"
                | "pump_name_ref"
                | "recipient_value"
                | "remember_object"
                | "remember_operand"
                | "owner_return"
                | "sacrificed_result"
                | "manifest_result"
                | "created_result"
                | "selected_result"
                | "drafted_result"
                | "exiled_card_result"
                | "search_move_result"
                | "seek_move_result"
        )
    ) {
        let index = usize::from(symbol_name(value) == Some("recipient_value"));
        *value = value["args"][index].take();
    }
    match value {
        Value::Object(items) => items.values_mut().for_each(strip),
        Value::Array(items) => items.iter_mut().for_each(strip),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn queries_capture_an_outer_candidate_and_inner_filters_start_fresh() {
        let scalar = Context::Outside.enter(&Value::Null, Some("in_filter"));
        assert_eq!(scalar, Context::Scalar);
        let query = scalar.enter(&json!({"kind":"CountObjects"}), None);
        assert_eq!(query, Context::Scalar);
        assert_eq!(query.child(&json!({}), "scope"), Context::Scalar);
        let query = query.child(&json!({}), "filter");
        assert_eq!(query, Context::Query);
        assert_eq!(
            query.enter(&Value::Null, Some("in_filter")),
            Context::Scalar
        );
    }

    #[test]
    fn embedded_abilities_keep_their_own_reference_context() {
        assert_eq!(
            Context::Scalar.enter(&Value::Null, Some("holder")),
            Context::Outside
        );
        let ability = json!({"kind":"Activated","args":{"announcement":{}}});
        assert_eq!(Context::Query.enter(&ability, None), Context::Outside);
    }

    #[test]
    fn stripping_contexts_keeps_the_bound_expression() {
        let expr = json!({"kind":"ObjectProperty","args":{"object":{"kind":"Candidate"},"property":"Power"}});
        let mut wrapped = json!({"$sym":"in_filter","at":5,"args":[expr]});
        strip(&mut wrapped);
        assert_eq!(wrapped, expr);
    }

    #[test]
    fn closed_operands_are_shared_but_x_waits_for_substitution() {
        let constant = json!({"kind":"Constant","args":3});
        assert_eq!(operand(&constant), constant);
        let x = json!({"kind":"AnnouncedX"});
        assert_eq!(operand(&x)["$sym"], "in_filter");
        assert_eq!(operand(&x)["args"][0], x);
    }
}
