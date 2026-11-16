//! A discard trigger's batch is separate from a later discard action's receipt.
use super::*;

#[test]
fn the_first_pump_counts_its_triggering_discard_batch_and_prints_edited_operands() {
    let mut frame = compositional(
        "Whenever you discard one or more cards, target creature gets +2/+0 until end of turn for each card discarded this way.",
    );
    let pump = &mut frame[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["changes"]
        [0]["args"];
    assert_eq!(pump["power"]["args"][1], json!({"kind":"EventAmount"}));
    pump["power"]["args"][0] = json!({"kind":"Constant","args":5});
    let text = printed(&frame);
    assert!(text.contains("+5/+0"), "{text}");
    assert!(text.contains("card discarded this way"), "{text}");
}

#[test]
fn a_later_pump_counts_its_explicit_discard_action() {
    let frame = compositional(
        "Whenever you discard one or more cards, discard a card. Target creature gets +1/+1 until end of turn for each card discarded this way.",
    );
    let body = &frame[0]["args"]["body"]["instructions"];
    let count = &body[1]["args"]["operation"]["args"]["changes"][0]["args"]["power"];
    assert_eq!(count["kind"], "CountObjects");
    assert_eq!(count["args"]["kind"], "Transition");
    assert_eq!(count["args"]["args"]["result"], body[0]["args"]["result"]);
    assert_eq!(count["args"]["args"]["phase"], "Before");
    printed(&frame);
}

#[test]
fn other_events_and_independent_trigger_bodies_cannot_supply_this_discard_count() {
    for text in [
        "Whenever you gain life, target creature gets +1/+1 until end of turn for each card discarded this way.",
        "Whenever you discard one or more cards, create a 1/1 white Human creature token with \"Whenever this creature deals damage, this creature gets +1/+1 until end of turn for each card discarded this way.\"",
    ] {
        assert!(engine().parse("Discard context", text).is_err(), "{text}");
    }
}
