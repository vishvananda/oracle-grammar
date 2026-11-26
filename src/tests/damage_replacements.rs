//! "If damage would be dealt to …": damage from any source, in the passive. Prevention
//! keeps the event; the instructions after it answer the same damage.
use super::*;

/// The body operations of the replacement ability in `reading`.
fn body(reading: &Value) -> Vec<String> {
    reading[0]["args"]["body"]["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|effect| {
            effect["args"]["operation"]["kind"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

#[test]
fn damage_dealt_to_this_creature_is_damage_from_any_source() {
    let reading = compositional(
        "If damage would be dealt to this creature, prevent that damage. Remove a +1/+1 counter \
         from this creature.",
    );
    let event = &reading[0]["args"]["event"]["Damage"];
    assert_eq!(event["sources"], json!({"kind":"Any"}), "{event}");
    assert_eq!(event["subject"], "Recipient", "{event}");
    assert_eq!(body(&reading), ["ModifyEvent", "RemoveCounters"]);
    printed(&reading);
}

#[test]
fn damage_dealt_to_you_can_become_counters_instead() {
    let reading = compositional(
        "If damage would be dealt to you, put that many depletion counters on this enchantment \
         instead.",
    );
    assert_eq!(
        reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["change"],
        "Cancel"
    );
    assert_eq!(body(&reading), ["ModifyEvent", "PutCounters"]);
    let text = reading.to_string();
    assert!(
        text.contains(r#""amount":{"kind":"EventAmount"}"#),
        "{text}"
    );
    printed_types(&reading, &["Enchantment"]);
}

#[test]
fn a_pronoun_in_a_self_damage_replacement_names_the_damaged_object() {
    let reading = compositional(
        "If damage would be dealt to this creature, put that many +1/+1 counters on it instead.",
    );
    let placement = &reading[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["placements"]
        [0];
    assert_eq!(
        placement["recipients"]["args"],
        json!({"kind":"One","args":{"kind":"Source"}}),
        "{placement}"
    );
    printed(&reading);
}
