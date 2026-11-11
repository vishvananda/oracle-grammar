//! Reject one bare that-player fallback in an independent self-leave ability.
//! Remembering and later reading the chosen player remain entirely in EBNF.
use super::{references, symbol_name};
use serde_json::{Value, json};
fn empty_announcement(value: &Value) -> bool {
    (symbol_name(value) == Some("announcement") && value["args"] == json!([]))
        || (symbol_name(value).is_none()
            && value["targets"] == json!([])
            && value["modes"].is_null())
}
/// Called only for the top-level ability, after event-source roles are resolved.
/// No holder, conditional/branch, mode or lexical child is traversed.
pub(super) fn prepare(root: &Value) -> Result<(), String> {
    if root["kind"] != "Triggered"
        || root["args"]["event"]
            != json!({"Object":{"event":"LeavesBattlefield",
            "objects":{"kind":"SameObject","args":{"kind":"Source"}}}})
        || !root["args"]["intervening_if"].is_null()
        || root["args"]["limits"] != json!({"per_turn":null,"total":null})
        || !empty_announcement(&root["args"]["announcement"])
    {
        return Ok(());
    }
    let Some([item]) = root["args"]["body"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return Ok(());
    };
    let operation = &item["args"]["operation"];
    let player = &operation["args"]["players"]["args"];
    if item["kind"] != "Effect"
        || operation["kind"] != "GainLife"
        || operation["args"]["players"]["kind"] != "One"
        || !matches!(
            symbol_name(player),
            Some("that_player" | "that_player_literal")
        )
        || player["args"] != json!([])
        || operation["args"]["amount"]["kind"] != "Constant"
        || operation["args"]["amount"]["args"].as_u64().is_none()
        || references::target_footprint(root)? != (0, 0)
    {
        return Ok(());
    }
    Err("self-leave event supplies no player for unresolved that-player gain".into())
}

#[cfg(test)]
mod tests;
