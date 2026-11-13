//! Independent exchanges, actual observations and damage-time participants.
use super::*;

fn fixture(name: &str) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/exchange-reveal-history-corrections.json"
    ))
    .unwrap();
    cases["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap()
        .clone()
}
fn frame(name: &str) -> Value {
    canonical_abilities(&fixture(name)["expected"]).unwrap()
}
fn body(frame: &mut Value) -> &mut Vec<Value> {
    frame[0]["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
}
fn op(instruction: &mut Value) -> &mut Value {
    &mut instruction["args"]["operation"]["args"]
}

#[test]
fn three_reported_programs_have_only_the_correct_meaning_and_exact_print() {
    for name in ["Juxtapose", "Vexing Arcanix", "Maelstrom Archangel Avatar"] {
        let case = fixture(name);
        let text = case["text"].as_str().unwrap();
        validate_abilities(&case["expected"]).unwrap();
        let expected = frame(name);
        assert_eq!(
            engine().parse(name, text).unwrap().readings,
            vec![expected.clone()],
            "{name}"
        );
        let trail = engine().trail_of(name, &expected, text).unwrap().unwrap();
        assert_eq!(
            engine().print_trail(name, &expected, &trail).unwrap(),
            Some(text.to_string()),
            "{name}"
        );
        let types: Vec<_> = case["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_string())
            .collect();
        assert!(
            engine()
                .print_keyed(name, &expected, &types, &[])
                .unwrap()
                .is_some(),
            "{name}"
        );
    }
    let text = fixture("Juxtapose")["text"].as_str().unwrap().to_string();
    assert_eq!(compositional(&text), frame("Juxtapose"));
}

#[test]
fn greatest_value_exchanges_keep_their_players_order_and_relative_receipts() {
    let mut reading = frame("Juxtapose");
    assert_eq!(body(&mut reading).len(), 6);
    for (index, player, kind) in [
        (0, json!({"kind":"AbilityController"}), "Creature"),
        (
            1,
            json!({"kind":"Target","args":{"slot":0,"index":0}}),
            "Creature",
        ),
        (3, json!({"kind":"AbilityController"}), "Artifact"),
        (
            4,
            json!({"kind":"Target","args":{"slot":0,"index":0}}),
            "Artifact",
        ),
    ] {
        let operation = op(&mut body(&mut reading)[index]);
        assert_eq!(operation["player"], player);
        assert_eq!(
            operation["count"],
            json!({"AsManyAsCan":{"kind":"Constant","args":1}})
        );
        let extreme = &mut operation["objects"]["args"]["filter"]["args"];
        assert_eq!(extreme["property"], "ManaValue");
        assert_eq!(extreme["highest"], true);
        let filters = extreme["among"]["args"]["filter"]["args"]
            .as_array_mut()
            .unwrap();
        let card_type = filters
            .iter_mut()
            .find(|f| f["kind"] == "CardType")
            .unwrap();
        assert_eq!(card_type["args"], kind);
        card_type["args"] = json!(if kind == "Creature" {
            "Enchantment"
        } else {
            "Land"
        });
    }
    for index in [2, 5] {
        assert_eq!(body(&mut reading)[index]["kind"], "Effect");
        assert_eq!(
            body(&mut reading)[index]["args"]["operation"]["args"]["action"]
                .as_object()
                .unwrap()
                .keys()
                .next()
                .unwrap(),
            "ExchangeControl"
        );
    }
    let mut prefix = compositional("Draw a card.");
    let mut draw = body(&mut prefix).remove(0);
    draw["args"]["result"] = json!(100);
    body(&mut reading).insert(0, draw);
    validate_abilities(&reading).unwrap();
    let text = printed(&reading);
    assert!(
        text.contains("enchantment") && text.contains("lands") && text.contains("Draw a card"),
        "{text}"
    );
}

#[test]
fn revealed_card_existence_guards_both_branches_and_both_moves_use_that_receipt() {
    let mut reading = frame("Vexing Arcanix");
    let items = body(&mut reading);
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["args"]["operation"]["kind"], "ChooseValue");
    assert_eq!(items[1]["args"]["operation"]["kind"], "Reveal");
    let revealed = items[1]["args"]["result"].clone();
    let guard = &mut items[2]["args"];
    assert_eq!(
        guard["condition"]["args"]["left"],
        json!({"kind":"CountObjects","args":{"kind":"Observed","args":revealed}})
    );
    assert_eq!(guard["otherwise"]["instructions"], json!([]));
    let name_test = &mut guard["then"]["instructions"][0]["args"];
    for arm in ["then", "otherwise"] {
        assert_eq!(
            name_test[arm]["instructions"][0]["args"]["operation"]["args"]["objects"],
            json!({"kind":"Observed","args":revealed})
        );
    }
    name_test["otherwise"]["instructions"][1]["args"]["operation"]["args"]["amount"] =
        json!({"kind":"Constant","args":4});
    assert!(printed(&reading).contains("4 damage"));
}

#[test]
fn combat_history_and_copy_type_guards_keep_mana_cost_and_current_operands() {
    let case = fixture("Maelstrom Archangel Avatar");
    let text = case["text"].as_str().unwrap();
    let mut reading = frame("Maelstrom Archangel Avatar");
    let items = body(&mut reading);
    let objects = &mut items[0]["args"]["objects"];
    assert_eq!(objects["kind"], "Union");
    let history = &mut objects["args"][0];
    assert_eq!(history["kind"], "FromHistory");
    assert_eq!(history["args"]["window"], "ThisTurn");
    let damage = &mut history["args"]["event"]["Damage"];
    assert_eq!(damage["kind"], "Combat");
    assert_eq!(damage["grouping"], "Source");
    assert_eq!(damage["subject"], "Source");
    assert_eq!(damage["recipients"], json!({"Players":{"kind":"All"}}));
    let filters = damage["sources"]["args"].as_array_mut().unwrap();
    assert!(filters.contains(
        &json!({"kind":"ControlledBy","args":{"kind":"One","args":{"kind":"AbilityController"}}})
    ));
    filters
        .iter_mut()
        .find(|f| f["kind"] == "CardType")
        .unwrap()["args"] = json!("Artifact");
    let selection =
        &items[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["steps"][0];
    assert_eq!(
        selection["objects"]["args"]["args"]["property"],
        "ManaCosts"
    );
    let paid = items[1]["args"]["result"].clone();
    assert_eq!(
        items[2]["args"]["condition"],
        json!({"kind":"PaymentChosen","args":paid})
    );
    let chosen = &items[2]["args"]["then"]["instructions"];
    assert_eq!(chosen.as_array().unwrap().len(), 3);
    assert_eq!(chosen[1]["args"]["otherwise"]["instructions"], json!([]));
    assert_eq!(chosen[2]["args"]["otherwise"]["instructions"], json!([]));
    assert_eq!(
        chosen[2]["args"]["condition"]["args"]["object"],
        json!({"kind":"Chosen"})
    );
    assert_eq!(
        chosen[2]["args"]["condition"]["args"]["filter"],
        json!({"kind":"AnyOf","args":[{"kind":"CardType","args":"Instant"},{"kind":"CardType","args":"Sorcery"}]})
    );
    op(&mut items[1])["cost"]["parts"][0] = json!({"Mana":[{"Generic":4}]});
    let printed = printed(&reading);
    assert!(
        printed.contains("{4}") && printed.contains("artifact") && !printed.contains("mana value"),
        "{printed}"
    );
    for invalid in [
        text.replace("choose one of those copies", "choose two of those copies"),
        text.replace("as that creature", "as that artifact"),
    ] {
        assert!(
            engine()
                .parse("Maelstrom Archangel Avatar", &invalid)
                .is_err(),
            "{invalid}"
        );
    }
}
