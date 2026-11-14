//! One counter amount and its subsequent numeric test use one sampled X.
use serde_json::Value;

pub(super) fn shared(body: &Value) -> bool {
    let Some([effect, condition]) = body["instructions"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let operation = &effect["args"]["operation"];
    if effect["kind"] != "Effect" || operation["kind"] != "PutCounters" {
        return false;
    }
    let Some([placement]) = operation["args"]["placements"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return false;
    };
    if !super::super::is_x(&placement["amount"])
        || super::x_count(effect) != 1
        || !placement["selection"].is_null()
    {
        return false;
    }
    let condition = unwrap(condition);
    let predicate = &condition["args"]["condition"];
    condition["kind"] == "If"
        && predicate["kind"] == "Compare"
        && super::super::is_x(&predicate["args"]["left"])
        && super::x_count(condition) == 1
}

fn unwrap(mut value: &Value) -> &Value {
    for _ in 0..4 {
        if matches!(value["$sym"].as_str(), Some("leading_if" | "resolution"))
            && let Some([inner]) = value["args"].as_array().map(Vec::as_slice)
        {
            value = inner;
        } else {
            break;
        }
    }
    value
}

pub(super) fn definition(value: &Value) -> bool {
    super::x_count(value) == 0
        && (super::shared_definition(value) || value["kind"] == "EventHistory")
}
