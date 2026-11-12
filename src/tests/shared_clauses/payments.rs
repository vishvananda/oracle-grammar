//! An alternative cost is any payment, two joined by `and`, or mana the player pays,
//! perhaps only while a condition holds; a condition on spent mana counts what was
//! paid, of any type or of one, for the spell the sentence names.
use super::*;

fn alternative(reading: &Value) -> &Value {
    &reading[0]["args"]["AlternativeCost"]["parts"]
}

#[test]
fn an_alternative_cost_is_mana_paid_or_payments_joined_by_and() {
    let mana = "You may pay {1}{U} rather than pay this spell's mana cost.";
    let reading = compositional(mana);
    assert_eq!(
        alternative(&reading),
        &json!([{"Mana":[{"Generic":1},{"Color":"Blue"}]}])
    );
    assert_eq!(printed_types(&reading, &["Instant"]), mana);
    let two = "You may pay 1 life and exile a black card from your hand rather than pay \
               this spell's mana cost.";
    let reading = compositional(two);
    let parts = alternative(&reading);
    assert_eq!(parts[0], json!({"Life":{"kind":"Constant","args":1}}));
    assert_eq!(
        parts[1]["Move"]["selections"][0]["objects"]["args"]["scope"]["Players"]["zone"],
        "Hand"
    );
    assert_eq!(printed_types(&reading, &["Instant"]), two);
}

#[test]
fn an_alternative_cost_may_hold_only_while_a_condition_does() {
    let before = "If you control a Swamp, you may sacrifice a creature rather than pay this \
                  spell's mana cost.";
    let after = "You may sacrifice a creature rather than pay this spell's mana cost if you \
                 control a Swamp.";
    let reading = compositional(before);
    assert_eq!(
        reading[0]["args"]["If"]["condition"]["args"]["filter"],
        json!({"kind":"Subtype","args":"Swamp"})
    );
    assert_eq!(compositional(after), reading);
    assert_eq!(printed_types(&reading, &["Instant"]), before);
}

fn spent(reading: &Value, at: &str) -> Value {
    let condition = match at {
        "intervening" => &reading[0]["args"]["intervening_if"],
        _ => &reading[0]["args"]["body"]["instructions"][0]["args"]["condition"],
    };
    let filter = &condition["args"]["filter"]["args"]["args"];
    json!({
        "object": condition["args"]["object"]["kind"],
        "colors": filter["left"]["args"]["filter"]["colors"],
        "least": filter["test"]["args"]["value"]["args"],
    })
}

#[test]
fn spent_mana_counts_what_paid_for_the_spell_named() {
    let it = "Whenever you cast a noncreature spell, if at least four mana was spent to cast \
              it, put a +1/+1 counter on this creature.";
    let reading = compositional(it);
    assert_eq!(
        spent(&reading, "intervening"),
        json!({"object":"EventSubject","colors":null,"least":4})
    );
    assert_eq!(printed(&reading), it);
    let typed = "If at least three black mana was spent to cast this spell, create a Food \
                 token.";
    let reading = compositional(typed);
    assert_eq!(
        spent(&reading, "body"),
        json!({"object":"Source","colors":["Black"],"least":3})
    );
    assert_eq!(printed_types(&reading, &["Sorcery"]), typed);
}

#[test]
fn a_spell_named_after_an_action_on_the_card_keeps_its_noun_in_print() {
    let text = "Whenever you cast a spell, this creature perpetually gets +1/+1. If five or \
                more mana was spent to cast that spell, draw a card.";
    let reading = compositional(text);
    let condition = &reading[0]["args"]["body"]["instructions"][1]["args"]["condition"];
    assert_eq!(condition["args"]["object"], json!({"kind":"EventSubject"}));
    assert!(printed(&reading).contains("to cast that spell"));
}
