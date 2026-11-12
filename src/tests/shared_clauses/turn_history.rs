//! What happened this turn qualifies objects and conditions: an object that entered, one
//! that left, a spell that was warped. A counter for each kind goes only on that kind.
use super::*;

fn entered_this_turn() -> Value {
    json!({"kind":"Condition","args":{"kind":"Compare","args":{
        "left":{"kind":"EventHistory","args":{
            "event":{"ZoneChange":{"objects":{"kind":"SameObject","args":{"kind":"Candidate"}},"from":null,"to":"Battlefield"}},
            "window":"ThisTurn","measure":"Occurrences"}},
        "test":{"kind":"Compare","args":{"comparison":"GreaterOrEqual","value":{"kind":"Constant","args":1}}}}}})
}

#[test]
fn an_object_that_entered_this_turn_is_any_kind_that_did() {
    let target = "Destroy target nonblack creature that entered this turn.";
    let reading = compositional(target);
    let filter = &reading[0]["args"]["announcement"]["targets"][0]["filter"]["args"];
    assert!(
        filter.as_array().unwrap().contains(&entered_this_turn()),
        "{filter}"
    );
    assert_eq!(printed_types(&reading, &["Instant"]), target);
    let group = "Creatures you control that entered this turn have double strike.";
    let printed = printed(&compositional(group));
    assert!(printed.contains("that entered this turn"), "{printed}");
    let plural = printed.starts_with("Creatures");
    assert_eq!(plural, printed.contains("turn have"), "{printed}");
}

#[test]
fn a_permanent_leaving_or_a_spell_warped_this_turn_is_one_condition() {
    let text = "If a nonland permanent left the battlefield this turn or a spell was warped \
                this turn, draw a card.";
    let reading = compositional(text);
    let condition = &reading[0]["args"]["body"]["instructions"][0]["args"]["condition"];
    assert_eq!(condition["kind"], "AnyOf");
    let left = &condition["args"][0]["args"]["left"]["args"]["event"]["Object"];
    assert_eq!(left["event"], "LeavesBattlefield");
    let warped = &condition["args"][1]["args"]["left"]["args"]["event"]["Spell"]["spells"];
    assert_eq!(warped, &json!({"kind":"State","args":"Warped"}));
    assert_eq!(printed_types(&reading, &["Sorcery"]), text);
}

#[test]
fn each_counter_goes_only_on_its_kind() {
    let text = "{T}: Choose target commander that entered this turn. Put a +1/+1 counter on \
                it if it's a creature and a loyalty counter on it if it's a planeswalker.";
    let reading = compositional(text);
    let tests: Vec<_> = reading[0]["args"]["body"]["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|instruction| instruction["args"]["condition"]["args"]["filter"]["args"].clone())
        .collect();
    assert_eq!(tests, [json!("Creature"), json!("Planeswalker")]);
    let printed = printed_types(&reading, &["Land"]);
    assert!(printed.contains("loyalty counter"), "{printed}");
}
