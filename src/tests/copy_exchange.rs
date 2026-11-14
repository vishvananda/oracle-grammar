use super::*;

#[test]
fn an_exchange_group_keeps_both_indexes_after_an_earlier_target() {
    let text = "{T}: Tap target land. Exchange control of two target nonlegendary creatures. You can't activate this ability during combat.";
    let parsed = engine().parse("Context example", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let mut reading = parsed.readings[0].clone();
    let args = &mut reading[0]["args"];
    let pair =
        &args["body"]["instructions"][1]["args"]["operation"]["args"]["action"]["ExchangeControl"];
    for (name, index) in [("first", 0), ("second", 1)] {
        assert_eq!(
            pair[name],
            json!({"kind":"Target","args":{"slot":1,"index":index}})
        );
    }
    assert_eq!(
        args["announcement"]["targets"][1]["count"],
        json!({"Exactly":{"kind":"Constant","args":2}})
    );
    assert_eq!(
        args["limits"]["condition"],
        json!({"kind":"Not","args":{"kind":"Window","args":"Combat"}})
    );
    crate::traverse::rewrite(
        &mut args["announcement"]["targets"][1]["filter"],
        &mut |v| {
            if v["kind"] == "CardType" && v["args"] == "Creature" {
                v["args"] = json!("Artifact");
            }
            crate::traverse::Visit::Children
        },
    );
    args["cost"] = json!({"parts":[{"Mana":[{"Generic":2}]}]});
    let text = printed_types(&reading, &["Creature"]);
    assert!(text.contains("{2}"), "{text}");
    assert!(
        text.to_lowercase()
            .contains("two target nonlegendary artifacts"),
        "{text}"
    );
    assert!(text.to_lowercase().contains("during combat"), "{text}");
}

#[test]
fn copy_exceptions_print_the_current_native_mechanic_and_optional_target() {
    let text = "Tap target land. This creature becomes a copy of up to one target nonlegendary creature you control until end of turn, except it has myriad.";
    let parsed = engine().parse("Context example", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let mut reading = parsed.readings[0].clone();
    let args = &mut reading[0]["args"];
    assert_eq!(
        args["announcement"]["targets"][1]["count"],
        json!({"UpTo":{"kind":"Constant","args":1}})
    );
    let copied = &mut args["body"]["instructions"][1]["args"]["operation"]["args"];
    assert_eq!(
        copied["affected"],
        json!({"Objects":{"kind":"One","args":{"kind":"Source"}}})
    );
    assert_eq!(
        copied["changes"][0]["args"]["object"],
        json!({"kind":"Target","args":{"slot":1,"index":0}})
    );
    let ability = &mut copied["changes"][0]["args"]["exceptions"][0]["args"][0];
    assert_eq!(ability["kind"], "Mechanic");
    assert_eq!(ability["args"]["mechanic"], "Myriad");
    ability["args"]["mechanic"] = json!("Toxic");
    ability["args"]["amount"] = json!({"kind":"Constant","args":4});
    copied["duration"] = json!("EndOfCombat");
    crate::traverse::rewrite(
        &mut args["announcement"]["targets"][1]["filter"],
        &mut |v| {
            if v["kind"] == "CardType" && v["args"] == "Creature" {
                v["args"] = json!("Artifact");
            }
            crate::traverse::Visit::Children
        },
    );
    let text = printed_types(&reading, &["Creature"]).to_lowercase();
    assert!(text.contains("artifact"), "{text}");
    assert!(text.contains("toxic 4"), "{text}");
    assert!(
        !text.contains("myriad") && !text.contains("copy of that creature"),
        "{text}"
    );
    assert!(
        text.contains("this combat") || text.contains("until end of combat"),
        "{text}"
    );
}

#[test]
fn granted_exchange_groups_start_their_own_target_scope() {
    let text = "Target creature gains \"{T}: Exchange control of two target artifacts. You can't activate this ability during combat.\" until end of turn.";
    let parsed = engine().parse("Context example", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let reading = &parsed.readings[0];
    assert_eq!(
        reading[0]["args"]["announcement"]["targets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut activations = Vec::new();
    visit(reading, "Activated", &mut activations);
    assert_eq!(activations.len(), 1);
    let granted = &activations[0];
    assert_eq!(
        granted["announcement"]["targets"][0]["count"],
        json!({"Exactly":{"kind":"Constant","args":2}})
    );
    let pair = &granted["body"]["instructions"][0]["args"]["operation"]["args"]["action"]["ExchangeControl"];
    for (name, index) in [("first", 0), ("second", 1)] {
        assert_eq!(
            pair[name],
            json!({"kind":"Target","args":{"slot":0,"index":index}})
        );
    }
    printed_types(reading, &["Sorcery"]);
}
