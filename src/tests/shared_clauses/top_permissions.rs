//! Playing or casting from the top of your library is one permission per action:
//! "play lands", "cast <kind> spells" and "play cards", joined by "and". A condition
//! governs the whole static, and looking at the top card any time for a duration adds
//! its visibility before the permissions.
use super::*;

fn permission(action: &str, filter: Value) -> Value {
    let top = json!({"kind":"Take","args":{"objects":{"kind":"Zone","args":{"scope":{"Players":{
        "zone":"Library","players":{"kind":"One","args":{"kind":"Affected"}}}},
        "kind":"Card","filter":{"kind":"Any"}}},"count":{"kind":"Constant","args":1},"from":"Top"}});
    json!({"kind":"PlayerRule","args":{"PlayPermission":{"cards":top,"from":"Library",
        "action":action,"filter":filter}}})
}

fn kind(card_type: &str) -> Value {
    json!({"kind":"CardType","args":card_type})
}

#[test]
fn each_action_from_the_top_is_one_permission() {
    let text = "You may play lands and cast creature spells from the top of your library.";
    let reading = compositional(text);
    assert_eq!(
        reading[0]["args"]["changes"],
        json!([
            permission("PlayLand", json!({"kind":"Any"})),
            permission("CastSpell", kind("Creature"))
        ])
    );
    assert_eq!(printed_types(&reading, &["Enchantment"]), text);
    let text = "You may play snow lands and cast snow spells from the top of your library.";
    let snow = json!({"kind":"Supertype","args":"Snow"});
    assert_eq!(
        compositional(text)[0]["args"]["changes"],
        json!([
            permission("PlayLand", snow.clone()),
            permission("CastSpell", snow)
        ])
    );
}

#[test]
fn spells_of_two_written_kinds_are_one_permission() {
    let text = "You may cast artifact spells and colorless spells from the top of your library.";
    let changes = &compositional(text)[0]["args"]["changes"];
    assert_eq!(
        changes,
        &json!([permission(
            "CastSpell",
            json!({"kind":"AnyOf","args":[kind("Artifact"),{"kind":"Colorless"}]})
        )])
    );
    let text = "You may cast Cleric, Rogue, and Wizard spells from the top of your library.";
    let subtype = |name: &str| json!({"kind":"Subtype","args":name});
    assert_eq!(
        compositional(text)[0]["args"]["changes"],
        json!([permission(
            "CastSpell",
            json!({"kind":"AnyOf","args":[subtype("Cleric"), subtype("Rogue"), subtype("Wizard")]})
        )])
    );
}

#[test]
fn looking_any_time_comes_before_the_permissions() {
    let text = "{T}: Until end of turn, you may look at the top card of your library any time, \
                and you may play lands from the top of your library.";
    let reading = compositional(text);
    let effect = &reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"];
    let changes = effect["changes"].as_array().unwrap();
    assert!(changes[0]["args"]["Visibility"].is_object(), "{changes:?}");
    assert_eq!(changes[1], permission("PlayLand", json!({"kind":"Any"})));
    assert_eq!(effect["duration"], "EndOfTurn");
    printed_types(&reading, &["Artifact"]);
}

#[test]
fn a_condition_governs_the_whole_permission() {
    let text = "As long as this Equipment is attached to a creature, you may cast creature \
                spells from the top of your library.";
    let reading = compositional(text);
    assert_eq!(reading[0]["kind"], "Conditional");
    let condition = &reading[0]["args"]["condition"];
    assert_eq!(condition["kind"], "ObjectMatches");
    assert_eq!(condition["args"]["object"], json!({"kind":"Source"}));
    assert_eq!(condition["args"]["filter"]["kind"], "AttachedTo");
    assert_eq!(
        reading[0]["args"]["then"][0]["args"]["changes"],
        json!([permission("CastSpell", kind("Creature"))])
    );
}
