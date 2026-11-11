use crate::engine::{Engine, canonical_abilities};
use serde_json::{Value, json};

fn engine() -> &'static Engine {
    crate::test_support::engine()
}

#[test]
fn original_coin_text_retains_physical_outcomes_and_scoped_native_protocols() {
    let cases: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/coin-protocol-187.json")).unwrap();
    for case in cases["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let expected = canonical_abilities(&case["expected"]).unwrap();
        let parsed = engine()
            .parse(name, text)
            .unwrap_or_else(|why| panic!("{name}: {why:?}"));
        assert_eq!(parsed.readings, vec![expected], "{name}");
    }
}

#[test]
fn a_win_procedure_and_a_physical_procedure_do_not_share_receipt_zero() {
    let cases: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/coin-protocol-187.json")).unwrap();
    let odds = cases["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Odds")
        .unwrap();
    let mut win = odds["expected"][0].clone();
    fn winning(value: &mut Value) {
        if value["kind"] == "CoinFlip" {
            value["args"]["outcome"] = match value["args"]["outcome"].as_str() {
                Some("Heads") => json!("Won"),
                Some("Tails") => json!("Lost"),
                _ => value["args"]["outcome"].clone(),
            };
        }
        match value {
            Value::Object(fields) => fields.values_mut().for_each(winning),
            Value::Array(items) => items.iter_mut().for_each(winning),
            _ => (),
        }
    }
    winning(&mut win);
    let activated = |mut ability: Value| {
        ability["kind"] = json!("Activated");
        ability["args"]["origin"] = json!("Ordinary");
        ability["args"]["cost"] = json!({"parts":[]});
        ability["args"]["limits"] =
            serde_json::to_value(crate::model::ir::ActivationLimits::unrestricted()).unwrap();
        ability["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["protocol"] =
            json!("Called");
        ability
    };
    let mut ability = json!({"kind":"Static","args":{"kind":"Ordinary",
        "affected":{"Objects":{"kind":"One","args":{"kind":"Source"}}},
        "changes":[{"kind":"GrantAbility","args":activated(win)},
                   {"kind":"GrantAbility","args":activated(odds["expected"][0].clone())}]}});
    super::lower(&mut ability).unwrap();
    let grants = &ability["args"]["changes"];
    assert_eq!(
        grants[0]["args"]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["protocol"],
        "Called"
    );
    assert_eq!(
        grants[1]["args"]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["protocol"],
        "Uncalled"
    );
}
