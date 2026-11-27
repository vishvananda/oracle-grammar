//! A permission to play a set of cards includes both spells and lands.
use super::{trace, walk::lossless};
use crate::ir::Operation;
use serde_json::{Value, json};

fn permission(change: &Value) -> Option<&Value> {
    (change["kind"] == "PlayerRule")
        .then(|| change["args"].get("PlayPermission"))
        .flatten()
}

pub(super) fn join(effect: &mut Value, tracing: bool) {
    let args = &effect["args"];
    if args["affected"].get("Players").is_none() || args["evaluation"] != "LiveObjects" {
        return;
    }
    let Some([first, second]) = args["changes"].as_array().map(Vec::as_slice) else {
        return;
    };
    let (Some(land), Some(spell)) = (permission(first), permission(second)) else {
        return;
    };
    let pair = (land["action"].as_str(), spell["action"].as_str());
    if !matches!(
        pair,
        (Some("PlayLand"), Some("CastSpell")) | (Some("CastSpell"), Some("PlayLand"))
    ) || land["origins"] != json!(["Exile"])
        || spell["origins"] != json!(["Exile"])
        || land["filter"] != json!({"kind":"Any"})
        || spell["filter"] != land["filter"]
        || land["cards"]["kind"] != "Transition"
        || land["cards"]["args"]["phase"] != "After"
        || !trace::same(&land["cards"], &spell["cards"], tracing)
        || lossless::<Operation>(effect, tracing).is_none()
    {
        return;
    }
    let mut play = first.clone();
    let joined = &mut play["args"]["PlayPermission"];
    joined["action"] = json!("Either");
    trace::merge(&mut joined["cards"], &spell["cards"]);
    effect["args"]["changes"] = json!([play]);
}
