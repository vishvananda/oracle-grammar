//! `Any number of` is one count wherever targets are counted, and `other` targets right
//! after a target exclude that target. A group of them goes to their owners' hands.
use super::*;

fn first_target_count(reading: &Value) -> &Value {
    &reading[0]["args"]["announcement"]["targets"][0]["count"]
}

#[test]
fn any_number_of_targets_is_one_count_for_every_action() {
    for text in [
        "Destroy any number of target creatures.",
        "Untap any number of target creatures.",
        "Exile any number of target nonland permanents.",
    ] {
        let reading = compositional(text);
        assert_eq!(first_target_count(&reading), "AnyNumber", "{text}");
        assert_eq!(printed_types(&reading, &["Sorcery"]), text);
    }
}

#[test]
fn other_targets_right_after_a_target_exclude_that_target() {
    let text = "Destroy up to one target artifact or enchantment. Put a +1/+1 counter on \
                each of up to two other target creatures.";
    let reading = compositional(text);
    let other = &reading[0]["args"]["announcement"]["targets"][1];
    assert_eq!(other["count"], json!({"UpTo":{"kind":"Constant","args":2}}));
    assert_eq!(
        other["filter"]["args"][1],
        json!({"kind":"Not","args":{"kind":"SameObject","args":{"kind":"Target","args":{"slot":0,"index":0}}}})
    );
}

#[test]
fn a_group_of_targets_returns_to_their_owners_hands() {
    let text = "Return any number of target creatures you control to their owners' hands.";
    let reading = compositional(text);
    assert_eq!(printed_types(&reading, &["Sorcery"]), text);
    let one = "Return target creature to its owner's hand.";
    assert_eq!(printed_types(&compositional(one), &["Sorcery"]), one);
}
