//! The target of support on a permanent excludes its own source.
use crate::model::ir_schema::{Schema, Ty};
use crate::pattern::{Bindings, Pattern, Syntax, elaborate};
use serde_json::{Value, json};

pub(crate) fn checked_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    let [other] = args else {
        return Err("@support_filter needs the printed other flag".into());
    };
    if *ty != Ty::Named("ObjectFilter".into()) {
        return Err("@support_filter produces an ObjectFilter".into());
    }
    Ok(Pattern::Symbol(
        "support_filter".into(),
        vec![elaborate(other, &Ty::Named("bool".into()), schema)?],
    ))
}

pub(crate) fn permanent_ability(ability: &Value) -> bool {
    let mut ability = crate::bind::presentation::core(ability);
    if ability["kind"] == "Scoped" {
        if !ability["args"]["zones"]
            .as_array()
            .is_some_and(|zones| zones.iter().any(|zone| zone == "Battlefield"))
        {
            return false;
        }
        ability = crate::bind::presentation::core(&ability["args"]["ability"]);
    }
    matches!(ability["kind"].as_str(), Some("Activated" | "Triggered"))
}

fn filter(other: bool) -> Value {
    let creature = json!({"kind":"CardType","args":"Creature"});
    if other {
        json!({"kind":"AllOf","args":[creature,
            {"kind":"Not","args":{"kind":"SameObject","args":{"kind":"Source"}}}]})
    } else {
        creature
    }
}

pub(crate) fn bind_filter(node: &Value, permanent: bool) -> Result<Value, String> {
    let other = node["args"][0]
        .as_bool()
        .ok_or("support needs its other flag")?;
    if other && !permanent {
        return Err("only support on a permanent excludes the source".into());
    }
    Ok(filter(permanent))
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [other] = args else {
        return Vec::new();
    };
    if *value == filter(true) {
        // Oracle omits "other" on some noncreature permanents. Their full
        // target domain still excludes the permanent if it becomes a creature.
        [true, false]
            .iter()
            .flat_map(|flag| other.unify_made(&Value::Bool(*flag), &bindings))
            .collect()
    } else if *value == filter(false) {
        other.unify_made(&Value::Bool(false), &bindings)
    } else {
        Vec::new()
    }
}
