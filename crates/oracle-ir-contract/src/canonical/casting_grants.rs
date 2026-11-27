//! Two cast-time declarations of the same receiving spell's ability.
use serde_json::{Value, json};

/// Cast filters have access to the proposal during payment. For a continuous
/// grant to this player's matching proposed spells, the stack-object grant
/// and the casting-player grant give the same spell its own mechanic.
pub(super) fn normalize(value: &mut Value, tracing: bool) {
    let args = &value["args"];
    let controller = json!({"kind":"One","args":{"kind":"AbilityController"}});
    let Some([change]) = args["changes"].as_array().map(Vec::as_slice) else {
        return;
    };
    if args["kind"] != "Ordinary"
        || args["affected"]["Players"] != controller
        || change["kind"] != "PlayerRule"
    {
        return;
    }
    let rule = &change["args"]["SpellsHaveAbilities"];
    let abilities = &rule["abilities"];
    let Some([term]) = abilities["args"].as_array().map(Vec::as_slice) else {
        return;
    };
    if !rule["from"].is_null()
        || !rule["nth_each_turn"].is_null()
        || abilities["kind"] != "List"
        || term["kind"] != "Term"
        || term["args"]["family"] != "SpellEffect"
    {
        return;
    }
    let term = &term["args"];
    let Some([operand]) = term["operands"].as_array().map(Vec::as_slice) else {
        return;
    };
    let mut cost = &Value::Null;
    let mut filter = &Value::Null;
    match term["form"].as_str() {
        Some("Affinity") if operand["kind"] == "Objects" => {
            let population = &operand["args"];
            let zone = &population["args"];
            if population["kind"] != "Zone"
                || zone["kind"] != "Permanent"
                || zone["scope"]["Shared"] != "Battlefield"
                || zone["filter"]["kind"] != "Subtype"
            {
                return;
            }
            filter = &zone["filter"];
        }
        Some("Replicate")
            if operand["kind"] == "Cost"
                && operand["args"]["parts"] == json!([{"ManaCostOf":{"kind":"Source"}}]) =>
        {
            cost = &operand["args"];
        }
        _ => return,
    }
    let cast = json!({"kind":"Cast","args":{"by":controller,"from":null}});
    let spells = match rule["spells"]["kind"].as_str() {
        Some("Any") => cast,
        Some("Subtype") => {
            let mut predicates = [cast, rule["spells"].clone()];
            predicates.sort_by_cached_key(|filter| super::trace::key(filter, tracing));
            json!({"kind":"AllOf","args":predicates})
        }
        _ => return,
    };
    *value = json!({"kind":"Static","args":{"kind":"Ordinary",
        "affected":{"Objects":{"kind":"Zone","args":{
            "scope":{"Shared":"Stack"},"kind":"Spell","filter":spells
        }}},
        "changes":[{"kind":"GrantAbility","args":{"kind":"Mechanic","args":{
            "mechanic":term["form"],"amount":null,"cost":cost,"filter":filter
        }}}]
    }});
}
