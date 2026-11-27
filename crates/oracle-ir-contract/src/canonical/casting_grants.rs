//! Two cast-time declarations of the same receiving spell's ability.
use serde_json::{Value, json};

/// Cast filters have access to the proposal during payment. For a continuous
/// grant to this player's matching proposed spells, the stack-object grant
/// and the casting-player grant give the same spell its own mechanic.
pub(super) fn normalize(value: &mut Value, tracing: bool) {
    if typed_replicate_population(value, tracing) {
        return;
    }
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

// The spell population and the casting-player rule apply in the same proposal
// frame. Keep the authored player rule for this defined replicate cost.
fn typed_replicate_population(value: &mut Value, tracing: bool) -> bool {
    let args = &value["args"];
    let subjects = &args["affected"]["Objects"];
    let zone = &subjects["args"];
    let filter = &zone["filter"];
    let Some([first, second]) = filter["args"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let controller = json!({"kind":"One","args":{"kind":"AbilityController"}});
    let cast = json!({"kind":"Cast","args":{"by":controller,"from":null}});
    let types = if *first == cast {
        second
    } else if *second == cast {
        first
    } else {
        return false;
    };
    let Some([left, right]) = types["args"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let instant = json!({"kind":"CardType","args":"Instant"});
    let sorcery = json!({"kind":"CardType","args":"Sorcery"});
    let cost = json!({"parts":[{"ManaCostOf":{"kind":"Source"}}]});
    if args["kind"] != "Ordinary"
        || subjects["kind"] != "Zone"
        || zone["scope"]["Shared"] != "Stack"
        || zone["kind"] != "Spell"
        || filter["kind"] != "AllOf"
        || types["kind"] != "AnyOf"
        || !([left, right].contains(&&instant) && [left, right].contains(&&sorcery))
        || args["changes"]
            != json!([{"kind":"GrantAbility","args":{"kind":"Mechanic","args":{
                "mechanic":"Replicate","cost":cost,"amount":null,"filter":null
            }}}])
        || super::walk::lossless::<crate::ir::Ability>(value, tracing).is_none()
    {
        return false;
    }
    *value = json!({"kind":"Static","args":{"kind":"Ordinary",
        "affected":{"Players":controller},"changes":[{"kind":"PlayerRule","args":{
            "SpellsHaveAbilities":{"spells":types,"from":null,"nth_each_turn":null,
                "abilities":{"kind":"List","args":[{"kind":"Term","args":{
                    "family":"SpellEffect","form":"Replicate","operands":[{"kind":"Cost","args":cost}]
                }}]}}
        }}]
    }});
    true
}
