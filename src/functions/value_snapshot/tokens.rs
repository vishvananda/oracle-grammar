//! The token recipe and subsequent damage can share one written value.
use super::x_count;
use serde_json::Value;

/// CreatingEffect initial copiable values are sampled before the entire batch
/// (IR CreateTokens contract). This is distinct from live token ability/CDA X.
pub(super) fn initial(body: &Value, operation: &Value) -> bool {
    token_stats_use_x(operation) && x_count(body) == 2
}

fn token_stats_use_x(operation: &Value) -> bool {
    // The token verb records its produced batch through this exact transparent
    // binder annotation. It is not an independent holder/scope or operation.
    let operation = if operation["$sym"] == "created_result" {
        let Some([operation]) = operation["args"].as_array().map(Vec::as_slice) else {
            return false;
        };
        operation
    } else {
        operation
    };
    if operation["kind"] != "CreateTokens" || operation["args"]["players"]["kind"] != "One" {
        return false;
    }
    let Some([token]) = operation["args"]["tokens"].as_array().map(Vec::as_slice) else {
        return false;
    };
    if token["words"] != "CreatingEffect" || super::super::constant(&token["amount"]) != Some(1) {
        return false;
    }
    let definition = &token["recipe"]["Defined"];
    // Keep independent definitions entirely outside this first batch contract,
    // including mana-X declarations which are not Expr::AnnouncedX nodes.
    if !definition["abilities"]
        .as_array()
        .is_some_and(Vec::is_empty)
        || !definition["abilities_of"].is_null()
    {
        return false;
    }
    let Some([power, toughness]) = definition["power_toughness"].as_array().map(Vec::as_slice)
    else {
        return false;
    };
    super::super::is_x(power) && super::super::is_x(toughness)
}

/// The written token stats and damage share the same definition. The token
/// chapter captures it and records every actual damage source. Inlining the
/// scalar here would also mistake a potentially expanded token batch for one.
pub(super) fn repeated_damage(body: &Value, amount: &Value) -> bool {
    let Some([create, damage]) = body["instructions"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let damage_operation = &damage["args"]["operation"];
    let source = &damage_operation["args"]["source"];
    let source = if source["$sym"] == "member_count_it" {
        let Some([pronoun]) = source["args"].as_array().map(Vec::as_slice) else {
            return false;
        };
        pronoun
    } else {
        source
    };
    create["kind"] == "Effect"
        && damage["kind"] == "Effect"
        && damage_operation["kind"] == "DealDamage"
        && (matches!(
            source["$sym"].as_str(),
            Some("it" | "current_it" | "created_reference_single")
        ) || source["kind"] == "Result"
            && source["args"]["result"] == create["args"]["result"]
            && source["args"]["phase"] == "After"
            && source["args"]["index"] == 0)
        && damage_operation["args"]["recipients"]["kind"] == "Players"
        && damage_operation["args"]["recipients"]["args"]["kind"] == "One"
        && damage_operation["args"]["recipients"]["args"]["args"]["kind"] == "AbilityController"
        && super::super::is_x(&damage_operation["args"]["amount"])
        && amount["kind"] == "Divide"
        && amount["args"]["numerator"]["kind"] == "PlayerNumber"
        && amount["args"]["numerator"]["args"]["number"] == "Life"
        && amount["args"]["numerator"]["args"]["player"]["kind"] == "AbilityController"
        && super::super::constant(&amount["args"]["denominator"]) == Some(2)
        && token_stats_use_x(&create["args"]["operation"])
        && x_count(body) == 3
}
