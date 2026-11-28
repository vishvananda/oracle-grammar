use super::validate_ability;
use oracle_ir::ir::Ability;
use serde_json::{Value, json};

fn ability() -> Value {
    json!({"kind":"Static","args":{
        "kind":"Ordinary",
        "affected":{"Players":{"kind":"One","args":{"kind":"AbilityController"}}},
        "changes":[{"kind":"PlayerRule","args":{"PlayPermissionWithCastEffects":{
            "permission":{"PlayPermission":{
                "cards":{"kind":"Zone","args":{"scope":{"Players":{"zone":"Graveyard","players":{"kind":"One","args":{"kind":"AbilityController"}}}},"kind":"Card","filter":{"kind":"Any"}}},
                "origins":["Graveyard"],"action":"CastSpell","filter":{"kind":"Any"}
            }},
            "terms":{"limit":"OnePerColorEachTurn","during":{"kind":"One","args":{"kind":"AbilityController"}},"cost":"Normal","spend_as":null,"into_exile":false,"condition":null,"effects":[],"flash":false,"may_look":false,"then":null,"face_down":null,"cast_filter":null},
            "effects":[{"ReplaceStackGraveyard":{"destination":"Exile"}},
                {"EnterBattlefieldWithCounters":{"spell":{"kind":"Any"},"counters":[[{"Named":"FinalityCounter"},{"kind":"Constant","args":1}]]}}]
        }}}]
    }})
}
fn validate(value: Value) -> bool {
    let node = &value["args"]["changes"][0]["args"]["PlayPermissionWithCastEffects"];
    let _: oracle_ir::ir::PlayerRule =
        serde_json::from_value(node["permission"].clone()).expect("permission");
    let _: oracle_ir::ir::PlayTerms = serde_json::from_value(node["terms"].clone()).expect("terms");
    let _: Vec<oracle_ir::ir::PermissionCastEffect> =
        serde_json::from_value(node["effects"].clone()).expect("effects");
    let ability: Ability = serde_json::from_value(value).expect("ability");
    validate_ability(&ability).is_ok()
}
#[test]
fn color_permission_effects_require_casts_and_bound_effect_operands() {
    let good = ability();
    assert!(validate(good.clone()));
    let mut bad = good.clone();
    bad["args"]["changes"][0]["args"]["PlayPermissionWithCastEffects"]["permission"]["PlayPermission"]
        ["action"] = json!("PlayLand");
    assert!(!validate(bad));
    let mut bad = good.clone();
    bad["args"]["changes"][0]["args"]["PlayPermissionWithCastEffects"]["effects"] = json!([]);
    assert!(!validate(bad));
    let mut bad = good.clone();
    bad["args"]["changes"][0]["args"]["PlayPermissionWithCastEffects"]["effects"][1]["EnterBattlefieldWithCounters"]
        ["counters"] = json!([]);
    assert!(!validate(bad));
    let mut bad = good;
    bad["args"]["changes"][0]["args"]["PlayPermissionWithCastEffects"]["effects"][1]["EnterBattlefieldWithCounters"]
        ["spell"] =
        json!({"kind":"SameObject","args":{"kind":"Target","args":{"slot":7,"index":0}}});
    assert!(!validate(bad));
}
