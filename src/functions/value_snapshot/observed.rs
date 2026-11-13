//! The explicit X definition belongs to the whole look/selection sequence.
use serde_json::Value;

/// Unsupported repeated look definitions must refuse capture, never fall back
/// to reevaluating a definition in an arbitrary later scope.
pub(super) fn repeated(body: &Value) -> bool {
    let Some(look) = body["instructions"].as_array().and_then(|i| i.first()) else {
        return false;
    };
    look["kind"] == "Effect"
        && look["args"]["operation"]["kind"] == "LookAt"
        && look["args"]["operation"]["args"]["objects"]["kind"] == "Take"
        && super::super::is_x(&look["args"]["operation"]["args"]["objects"]["args"]["count"])
        && super::x_count(look) == 1
        && super::x_count(body) > 1
}

fn effect(instruction: &Value, kind: &str) -> bool {
    instruction["kind"] == "Effect" && instruction["args"]["operation"]["kind"] == kind
}

fn choice_tail(tail: &[Value]) -> bool {
    matches!(tail, [choice, action, rest]
        if effect(choice, "ChooseObjects")
        && (effect(action, "Move") || effect(action, "ReorderLibrary"))
        && effect(rest, "ReorderLibrary"))
}

/// These suffixes do not introduce another definition of X. An If may test this
/// X after library changes; its branch programs need their own capture proof if
/// they use X. An optional exiled-card cast has no further numeric use.
fn suffix(instruction: &Value) -> bool {
    let instruction = if matches!(
        instruction["$sym"].as_str(),
        Some("leading_if" | "resolution")
    ) {
        &instruction["args"][0]
    } else {
        instruction
    };
    if instruction["kind"] == "Optional" {
        return super::x_count(instruction) == 0;
    }
    let args = &instruction["args"];
    instruction["kind"] == "If"
        && args["condition"]["kind"] == "Compare"
        && super::super::is_x(&args["condition"]["args"]["left"])
        && super::x_count(instruction) == 1
}

/// The whitelist keeps captures outside independent abilities, loops and local
/// payments. Both recognition and inverse matching enforce the same shapes.
pub(super) fn shared(body: &Value) -> bool {
    if !repeated(body) {
        return false;
    }
    let tail = &body["instructions"].as_array().unwrap()[1..];
    choice_tail(tail)
        || matches!(tail, [choice, reveal, movement, rest]
            if effect(choice, "ChooseObjects") && effect(reveal, "Reveal")
            && effect(movement, "Move") && effect(rest, "ReorderLibrary"))
        || tail.len() == 4 && choice_tail(&tail[..3]) && suffix(&tail[3])
        || matches!(tail, [cast, rest]
            if effect(cast, "PlayWith") && effect(rest, "ReorderLibrary")
            && super::x_count(cast) == super::x_count(&cast["args"]["operation"]["args"]["cards"]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unsupported_later_x_owners_refuse_instead_of_reevaluating_the_definition() {
        let look = json!({"kind":"Effect","args":{"result":0,"operation":{
            "kind":"LookAt","args":{"objects":{"kind":"Take","args":{
                "count":{"kind":"AnnouncedX"}}}}}}});
        let x = json!({"kind":"AnnouncedX"});
        for later in [
            json!({"kind":"Optional","args":{"body":{"instructions":[x]}}}),
            json!({"kind":"Effect","args":{"operation":{"kind":"GrantAbility","args":x}}}),
            json!({"kind":"ForEachPlayer","args":{"body":{"instructions":[x]}}}),
        ] {
            let body = json!({"instructions":[look,later]});
            let amount = json!({"kind":"Constant","args":3});
            assert!(repeated(&body));
            assert!(!shared(&body));
            assert!(super::super::required(&body, &amount));
            assert!(super::super::snapshot(&body, &amount, 7).is_err());
        }
    }
}
