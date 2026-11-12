//! Kinds joined in a list name either kind wherever the list stands, a token made for
//! each object of a kind prints as Oracle counts it, and an own `enters or attacks`
//! names the source in its body.
use super::*;

fn spell_cost_filter(reading: &Value) -> &Value {
    &reading[0]["args"]["changes"][0]["args"]["SpellCost"]["spells"]
}

#[test]
fn subtypes_that_begin_a_line_keep_their_capitals_and_join_as_either_kind() {
    let text = "Aura and Equipment spells you cast cost {1} less to cast.";
    let reading = compositional(text);
    assert_eq!(
        spell_cost_filter(&reading),
        &json!({"kind":"AnyOf","args":[
            {"kind":"Subtype","args":"Aura"},
            {"kind":"Subtype","args":"Equipment"}
        ]})
    );
    assert_eq!(printed(&reading), text);
    let one = "Equipment spells you cast cost {1} less to cast.";
    assert_eq!(
        spell_cost_filter(&compositional(one)),
        &json!({"kind":"Subtype","args":"Equipment"})
    );
}

#[test]
fn a_reduction_of_x_is_the_value_x_names() {
    let text = "Aura and Equipment spells you cast cost {X} less to cast, where X is the \
                number of creatures you control.";
    let reading = compositional(text);
    let reduction = &reading[0]["args"]["changes"][0]["args"]["SpellCost"]["modifier"]["Reduce"]["reductions"]
        [0];
    assert_eq!(reduction["amount"]["kind"], "CountObjects");
}

#[test]
fn a_token_for_each_object_prints_for_each() {
    let text = "Create a 0/1 green Plant creature token for each land you control.";
    let reading = compositional(text);
    let where_x = "Create X 0/1 green Plant creature tokens, where X is the number of lands \
                   you control.";
    assert_eq!(compositional(where_x), reading);
    assert_eq!(printed_types(&reading, &["Sorcery"]), text);
}

#[test]
fn an_own_entry_or_attack_names_the_source_in_its_body() {
    let text = "Whenever this creature enters or attacks, it deals 3 damage to any target.";
    let reading = compositional(text);
    let damage = &reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"];
    assert_eq!(damage["source"], json!({"kind":"Source"}));
    assert_eq!(printed(&reading), text);
    let other = "Whenever this creature enters or attacks, put a +1/+1 counter on another \
                 target creature.";
    let reading = compositional(other);
    assert!(!reading.to_string().contains("EventSubject"), "{reading}");
    assert_eq!(printed(&reading), other);
}
