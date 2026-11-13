//! The explicit X definition belongs to the whole look/selection sequence.
use serde_json::Value;

/// These programs have one eager LookAt count and only immediate card handling.
/// Other lexical owners, optional programs and delayed uses need their own
/// scope proof; accepting arbitrary instructions here would capture their X.
pub(super) fn shared(body: &Value) -> bool {
    let Some(instructions) = body["instructions"].as_array() else {
        return false;
    };
    let expected: &[&str] = match instructions.len() {
        4 => &["LookAt", "ChooseObjects", "Move", "ReorderLibrary"],
        5 => &[
            "LookAt",
            "ChooseObjects",
            "Reveal",
            "Move",
            "ReorderLibrary",
        ],
        _ => return false,
    };
    instructions
        .iter()
        .zip(expected)
        .all(|(instruction, kind)| {
            instruction["kind"] == "Effect" && instruction["args"]["operation"]["kind"] == *kind
        })
        && instructions[0]["args"]["operation"]["args"]["objects"]["kind"] == "Take"
        && super::super::is_x(
            &instructions[0]["args"]["operation"]["args"]["objects"]["args"]["count"],
        )
        && super::x_count(&instructions[0]) == 1
        && super::x_count(body) > 1
}
