use crate::{canonical, ir::Ability, validate::validate_ability};
use serde_json::{Value, json};
fn source() -> Value {
    serde_json::from_str(r###"[{"kind": "Static", "args": {"kind": "Ordinary", "affected": {"Objects": {"kind": "Zone", "args": {"scope": {"Shared": "Battlefield"}, "kind": "Permanent", "filter": {"kind": "AllOf", "args": [{"kind": "CardType", "args": "Creature"}, {"kind": "MatchesAnyCreatureType", "args": {"allowed": ["Clamfolk", "Crab", "Crocodile", "Fish", "Homarid", "Jellyfish", "Kraken", "Leviathan", "Lobster", "Merfolk", "Nautilus", "Octopus", "Otter", "Oyster", "Seal", "Serpent", "Shark", "Shrimp", "Sponge", "Squid", "Starfish", "Trilobite", "Turtle", "Walrus", "Whale"]}}, {"kind": "ControlledBy", "args": {"kind": "One", "args": {"kind": "AbilityController"}}}]}}}}, "changes": [{"kind": "Pump", "args": {"power": {"kind": "Constant", "args": 2}, "toughness": {"kind": "Constant", "args": 2}}}]}}, {"kind": "Triggered", "args": {"event": {"Turn": {"point": "UpkeepBegins", "players": {"kind": "One", "args": {"kind": "AbilityController"}}}}, "intervening_if": {"kind": "AllOf", "args": [{"kind": "Not", "args": {"kind": "Controls", "args": {"player": {"kind": "AbilityController"}, "filter": {"kind": "Subtype", "args": "Brushwagg"}, "minimum": 1}}}, {"kind": "Compare", "args": {"left": {"kind": "CountCreatureTypeMatching", "args": {"objects": {"kind": "Zone", "args": {"scope": {"Shared": "Battlefield"}, "kind": "Permanent", "filter": {"kind": "AllOf", "args": [{"kind": "CardType", "args": "Creature"}, {"kind": "ControlledBy", "args": {"kind": "One", "args": {"kind": "AbilityController"}}}]}}}, "allowed": ["Clamfolk", "Crab", "Crocodile", "Fish", "Homarid", "Jellyfish", "Kraken", "Leviathan", "Lobster", "Merfolk", "Nautilus", "Octopus", "Otter", "Oyster", "Seal", "Serpent", "Shark", "Shrimp", "Sponge", "Squid", "Starfish", "Trilobite", "Turtle", "Walrus", "Whale"]}}, "test": {"kind": "Compare", "args": {"comparison": "GreaterOrEqual", "value": {"kind": "Constant", "args": 12}}}}}]}, "limits": {"per_turn": null, "total": null}, "announcement": {"targets": [], "modes": null}, "body": {"instructions": [{"kind": "Effect", "args": {"result": 4294967295, "operation": {"kind": "Keyword", "args": {"actor": {"kind": "One", "args": {"kind": "AbilityController"}}, "action": "WinGame"}}}}]}}}]"###).unwrap()
}
fn checked(v: Value) -> bool {
    let a: Ability = serde_json::from_value(v).unwrap();
    validate_ability(&a).is_ok()
}
#[test]
fn static_and_live_trigger_share_the_full_type_list() {
    let a = source();
    for v in a.as_array().unwrap() {
        assert!(checked(v.clone()));
    }
    let filters = a[0]["args"]["affected"]["Objects"]["args"]["filter"]["args"]
        .as_array()
        .unwrap();
    let allowed = &filters
        .iter()
        .find(|f| f["kind"] == "MatchesAnyCreatureType")
        .unwrap()["args"]["allowed"];
    let cond = a[1]["args"]["intervening_if"]["args"].as_array().unwrap();
    let comparison = cond.iter().find(|c| c["kind"] == "Compare").unwrap();
    let left = &comparison["args"]["left"];
    assert_eq!(left["kind"], "CountCreatureTypeMatching");
    assert_eq!(&left["args"]["allowed"], allowed);
    assert_eq!(allowed.as_array().unwrap().len(), 25);
    assert!(allowed.as_array().unwrap().contains(&json!("Clamfolk")));
    assert!(allowed.as_array().unwrap().contains(&json!("Shrimp")));
    assert_eq!(
        comparison["args"]["test"]["args"]["comparison"],
        "GreaterOrEqual"
    );
    assert_eq!(comparison["args"]["test"]["args"]["value"]["args"], 12);
    assert!(cond.iter().any(|c| c["kind"] == "Not"
        && c["args"]["args"]["filter"] == json!({"kind":"Subtype","args":"Brushwagg"})));
    let objfilters = left["args"]["objects"]["args"]["filter"]["args"]
        .as_array()
        .unwrap();
    assert!(objfilters.iter().any(|f| f["kind"] == "ControlledBy"));
    assert!(
        objfilters
            .iter()
            .any(|f| f["kind"] == "CardType" && f["args"] == "Creature")
    );
}
#[test]
fn query_rejects_unbound_receipt_and_oversized_type_list() {
    let mut a = source()[1].clone();
    let conditions = a["args"]["intervening_if"]["args"].as_array_mut().unwrap();
    let c = conditions
        .iter_mut()
        .find(|c| c["kind"] == "Compare")
        .unwrap();
    c["args"]["left"]["args"]["objects"] = json!({"kind":"Selected","args":77});
    assert!(!checked(a));
    let mut a = source()[0].clone();
    let fs = a["args"]["affected"]["Objects"]["args"]["filter"]["args"]
        .as_array_mut()
        .unwrap();
    let f = fs
        .iter_mut()
        .find(|f| f["kind"] == "MatchesAnyCreatureType")
        .unwrap();
    f["args"]["allowed"] = json!(vec!["Fish"; 257]);
    assert!(!checked(a));
}
#[test]
fn creature_type_operands_canonicalize_as_sets() {
    let mut a = source();
    let mut b = a.clone();
    let fs = b[0]["args"]["affected"]["Objects"]["args"]["filter"]["args"]
        .as_array_mut()
        .unwrap();
    let types = fs
        .iter_mut()
        .find(|f| f["kind"] == "MatchesAnyCreatureType")
        .unwrap()["args"]["allowed"]
        .as_array_mut()
        .unwrap();
    types.reverse();
    types.push(json!("Fish"));
    let cond = b[1]["args"]["intervening_if"]["args"]
        .as_array_mut()
        .unwrap();
    let types=cond.iter_mut().find(|c|c["kind"]=="Compare").unwrap()["args"]["left"]["args"]["allowed"].as_array_mut().unwrap();
    types.reverse();
    types.push(json!("Fish"));
    canonical::abilities(&mut a).unwrap();
    canonical::abilities(&mut b).unwrap();
    assert_eq!(a, b);
}
