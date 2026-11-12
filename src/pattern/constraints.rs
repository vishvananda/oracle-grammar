//! Which bound references can satisfy a symbolic spelling operand.
use super::Pattern;
use serde_json::Value;

/// The targeting stack object is eligible only for a declared spell noun.
/// Other reference spellings keep their existing constraints and reparse guard.
pub(super) fn qualified_reference(args: &[Pattern], value: &Value) -> bool {
    refers_to("named_object", value)
        || value["kind"] == "EventStackObject"
            && matches!(args.first(), Some(Pattern::Value(Value::String(kind))) if kind == "Spell")
}

/// Symbols the pattern side does not constrain. The binder fills them going
/// forward. Going backward a wrong guess fails the reparse check.
pub(super) fn unconstrained(name: &str) -> bool {
    matches!(
        name,
        "next"
            | "coin_protocol"
            | "coin_receipt"
            | "slot"
            | "prior"
            | "mark"
            | "choice_mark"
            | "marked"
            | "back"
            | "announcement"
            | "share"
            | "cost_index"
            | "length"
            | "next_slot"
            | "last_slot"
            | "last_result"
            | "card_name"
    )
}

/// Whether a word that refers back could stand for this value. Reparsing
/// decides between the ones that could. The mention that introduces a target
/// is not a reference: it has to print as the target.
pub(super) fn refers_to(name: &str, value: &Value) -> bool {
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let introducing = value["args"]["slot"].get("$sym").is_some();
    match name {
        "self" => kind == "Source",
        "exiled_card_reference" => {
            kind == "Result"
                && value["args"]["phase"] == "After"
                && value["args"]["index"] == 0
                && value["args"]["result"].is_u64()
        }
        "drafted_reference" | "drafted_reference_explicit" => {
            kind == "Transition"
                && value["args"]["phase"] == "After"
                && value["args"]["result"].is_u64()
        }
        "other_reference" | "another_target_reference" => {
            !introducing
                && matches!(
                    kind,
                    "Source"
                        | "Target"
                        | "EventSubject"
                        | "EventDestination"
                        | "Result"
                        | "Query"
                        | "Affected"
                        | "AttachedTo"
                )
        }
        "named_source" => matches!(kind, "Source" | "GrantingObject"),
        "sacrificed_object" => {
            let objects = &value["args"]["objects"];
            kind == "Query"
                && value["args"]["index"] == 0
                && objects["args"]["phase"] == "Before"
                && (objects["kind"] == "Transition"
                    || objects["kind"] == "CostObjects"
                        && objects["args"]["view"] == "Inputs"
                        && objects["args"]["payment"].get("Resolution").is_some())
        }
        "defending" => kind == "DefendingPlayerOf",
        "other_party" => matches!(kind, "AbilityController" | "EventSubject"),
        "that_opponent" | "that_player_literal" => {
            !introducing
                && serde_json::from_value::<crate::model::ir::PlayerRef>(value.clone()).is_ok()
        }
        "that_player" if kind == "Acting" => true,
        "it" | "current_it" if kind == "Candidate" => true,
        "it" | "current_it" | "that_object" | "named_object" | "current_named_object"
            if kind == "AttachedTo" =>
        {
            true
        }
        "it"
        | "current_it"
        | "that_object"
        | "named_object"
        | "current_named_object"
        | "that_player" => {
            !introducing
                && matches!(
                    kind,
                    "Target"
                        | "EventSubject"
                        | "Source"
                        | "AbilityController"
                        | "DefendingPlayerOf"
                        | "Bound"
                        | "Result"
                        | "Query"
                        | "EventDestination"
                        | "Discarded"
                        | "Damage"
                        | "BlockPair"
                        | "ControllerOf"
                        | "OwnerOf"
                )
        }
        _ => false,
    }
}
