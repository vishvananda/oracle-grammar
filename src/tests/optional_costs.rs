//! Overload and optional additional costs are cost branches: overload changes each
//! "target" to "each", and "if a Dragon was beheld" asks whether the cost to behold a
//! Dragon was paid. A replacing action may also trail its condition ("instead if").
use super::*;

// Scored text drops reminder text, so these read as the corpus does.
const BEHOLD: &str = "As an additional cost to cast this spell, you may behold a Dragon.";

fn branch<'a>(reading: &'a Value, keyword: &str) -> &'a Value {
    let ability = &reading[0];
    assert_eq!(ability["kind"], "CostBranch", "{reading:#}");
    assert_eq!(ability["args"]["keyword"], keyword, "{reading:#}");
    &ability["args"]
}

fn effects(spell: &Value) -> Vec<Value> {
    let mut found = Vec::new();
    visit(&spell["args"]["body"], "Effect", &mut found);
    found.into_iter().map(|e| e["operation"].clone()).collect()
}

#[test]
fn overload_changes_each_target_to_each() {
    let reading = compositional("Tap target creature you don't control.\nOverload {3}{U}");
    let branch = branch(&reading, "Overload");
    assert_eq!(branch["paid"]["args"]["announcement"]["targets"], json!([]));
    let [tap] = effects(&branch["paid"]).try_into().unwrap();
    assert_eq!(tap["args"]["objects"]["kind"], "Zone", "{tap:#}");
    assert_eq!(
        branch["unpaid"]["args"]["announcement"]["targets"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    printed_types(&reading, &["Instant"]);
}

#[test]
fn an_overloaded_player_target_is_each_player() {
    let reading = compositional("Target player discards two cards.\nOverload {1}{B}");
    let [discard] = effects(&branch(&reading, "Overload")["paid"])
        .try_into()
        .unwrap();
    assert_eq!(discard["args"]["players"], json!({"kind":"All"}));
    printed_types(&reading, &["Sorcery"]);
}

#[test]
fn what_was_beheld_is_the_optional_cost_paid() {
    let text = format!("{BEHOLD}\nDestroy target tapped creature. If a Dragon was beheld, scry 2.");
    let reading = compositional(&text);
    let branch = branch(&reading, "AdditionalCost");
    assert_eq!(effects(&branch["paid"]).len(), 2);
    assert_eq!(effects(&branch["unpaid"]).len(), 1);
    let printed = printed_types(&reading, &["Instant"]);
    assert!(printed.contains("If a Dragon was beheld"), "{printed}");
}

#[test]
fn what_was_beheld_names_the_cost_to_behold() {
    let other = BEHOLD.replace("behold a Dragon.", "behold a Goblin.");
    for text in [
        format!("{other}\nDestroy target tapped creature. If a Dragon was beheld, scry 2."),
        "Destroy target tapped creature. If a Dragon was beheld, scry 2.".into(),
        format!(
            "{BEHOLD}\nDestroy target nonland permanent. If a Dragon was beheld, it gets +2/+2 \
             until end of turn."
        ),
    ] {
        let parsed = engine().parse("Context example", &text);
        assert!(
            parsed.as_ref().is_err() || parsed.as_ref().unwrap().readings.is_empty(),
            "{text}: {parsed:?}"
        );
    }
}

#[test]
fn a_beheld_dragon_lets_that_spell_ask_for_more() {
    let reading = compositional(&format!(
        "{BEHOLD}\nCounter target spell unless its controller pays {{2}}. If a Dragon was \
         beheld, counter that spell unless its controller pays {{4}} instead."
    ));
    let branch = branch(&reading, "AdditionalCost");
    let pay = |side: &str| effects(&branch[side])[0]["args"].clone();
    assert_eq!(pay("paid")["player"], pay("unpaid")["player"]);
    assert_eq!(
        pay("paid")["cost"]["parts"][0]["Mana"],
        json!([{"Generic":4}])
    );
    printed_types(&reading, &["Instant"]);
}

#[test]
fn a_replacing_action_may_trail_its_condition() {
    let reading = compositional(
        "Target creature gets -1/-1 until end of turn.\nThat creature gets -13/-13 until end of \
         turn instead if a creature died this turn.",
    );
    let body = &reading[0]["args"]["body"]["instructions"];
    let [choice] = body.as_array().unwrap().as_slice() else {
        panic!("{reading:#}");
    };
    assert_eq!(choice["kind"], "If");
    printed_types(&reading, &["Instant"]);
    let reading = compositional(
        "As an additional cost to cast this spell, you may blight 1.\nContext example deals 2 \
         damage to target \
         creature. It deals 4 damage to that creature instead if this spell's additional cost \
         was paid.",
    );
    let branch = branch(&reading, "AdditionalCost");
    let amount = |side: &str| effects(&branch[side])[0]["args"]["amount"].clone();
    assert_eq!(amount("paid"), json!({"kind":"Constant","args":4}));
    assert_eq!(amount("unpaid"), json!({"kind":"Constant","args":2}));
    printed_types(&reading, &["Sorcery"]);
}

#[test]
fn a_leading_condition_may_raise_damage_with_its_recipient_unsaid() {
    let reading = compositional(
        "As an additional cost to cast this spell, you may sacrifice an artifact.\nContext \
         example deals 2 damage to target creature or planeswalker. If this spell's additional \
         cost was paid, Context example deals 4 damage instead.",
    );
    let additional = branch(&reading, "AdditionalCost");
    let damage = |side: &str| effects(&additional[side])[0]["args"].clone();
    assert_eq!(
        damage("paid")["amount"],
        json!({"kind":"Constant","args":4})
    );
    assert_eq!(damage("paid")["recipients"], damage("unpaid")["recipients"]);
    printed_types(&reading, &["Instant"]);
    let reading = compositional(
        "Kicker {4}\nContext example deals 2 damage to any target. If this spell was kicked, it \
         deals 4 damage instead.",
    );
    let branch = branch(&reading, "Kicker");
    let source = |side: &str| effects(&branch[side])[0]["args"]["source"].clone();
    assert_eq!(source("paid"), json!({"kind":"Source"}));
    assert_eq!(source("paid"), source("unpaid"));
    printed_types(&reading, &["Instant"]);
}

#[test]
fn a_returned_card_deals_the_damage_it_names() {
    let reading = compositional(
        "When this creature dies, return it to the battlefield tapped under its owner's \
         control. It deals 1 damage to each opponent.",
    );
    let [moved, damage] = reading[0]["args"]["body"]["instructions"]
        .as_array()
        .unwrap()
        .as_slice()
    else {
        panic!("{reading:#}");
    };
    let result = moved["args"]["result"].clone();
    assert_eq!(
        damage["args"]["operation"]["args"]["source"],
        json!({"kind":"Result","args":{"result":result,"phase":"After","index":0}})
    );
    printed(&reading);
}
