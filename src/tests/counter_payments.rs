use super::*;

#[test]
fn counter_payment_keeps_its_target_and_receipt_and_prints_current_animation() {
    let text = "Tap target creature. You may put three +1/+1 counters on target land you control. If you do, that land becomes a 0/0 Elemental creature with haste that's still a land.";
    let parsed = engine().parse("Context example", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let mut reading = parsed.readings[0].clone();
    let body = &mut reading[0]["args"]["body"]["instructions"];
    let result = body[1]["args"]["result"].clone();
    let paid = &mut body[1]["args"]["operation"]["args"];
    assert_eq!(paid["player"], json!({"kind":"AbilityController"}));
    assert_eq!(paid["optional"], true);
    let placement = &mut paid["cost"]["parts"][0]["PutCounters"][0];
    let target = json!({"kind":"Target","args":{"slot":1,"index":0}});
    assert_eq!(
        placement["recipients"],
        json!({"kind":"Objects","args":{"kind":"One","args":target}})
    );
    placement["amount"] = json!({"kind":"Constant","args":4});
    assert_eq!(
        body[2]["args"]["condition"],
        json!({"kind":"PaymentChosen","args":result})
    );
    let animated = &mut body[2]["args"]["then"]["instructions"][0]["args"]["operation"]["args"];
    assert_eq!(
        animated["affected"],
        json!({"Objects":{"kind":"One","args":target}})
    );
    animated["duration"] = json!("EndOfCombat");
    for change in animated["changes"].as_array_mut().unwrap() {
        match change["kind"].as_str().unwrap() {
            "SetBasePowerToughness" => {
                change["args"]["power"] = json!({"kind":"Constant","args":1});
                change["args"]["toughness"] = json!({"kind":"Constant","args":2});
            }
            "CreatureTypes" => change["args"]["types"]["Listed"] = json!(["Wolf"]),
            "GrantKeyword" => change["args"]["kind"] = json!("Vigilance"),
            _ => {}
        }
    }
    crate::traverse::rewrite(&mut reading, &mut |v| {
        if v["kind"] == "CardType" && v["args"] == "Land" {
            v["args"] = json!("Artifact");
        }
        crate::traverse::Visit::Children
    });
    let text = printed_types(&reading, &["Sorcery"]).to_lowercase();
    for current in ["artifact", "four", "1/2", "wolf", "vigilance"] {
        assert!(text.contains(current), "missing {current}: {text}");
    }
    assert!(
        text.contains("this combat") || text.contains("until end of combat"),
        "{text}"
    );
    assert!(!text.contains("that land"), "{text}");
}

#[test]
fn another_counter_payer_keeps_the_separate_recipient_and_decline_test() {
    for decline in [false, true] {
        let condition = if decline { "don't" } else { "do" };
        let text = format!(
            "Target player may put a shield counter on target artifact. If they {condition}, draw a card."
        );
        let parsed = engine().parse("Context example", &text).unwrap();
        assert_eq!(parsed.readings.len(), 1);
        let reading = &parsed.readings[0];
        let body = &reading[0]["args"]["body"]["instructions"];
        let paid = &body[0]["args"]["operation"]["args"];
        assert_eq!(
            paid["player"],
            json!({"kind":"Target","args":{"slot":0,"index":0}})
        );
        assert_eq!(
            paid["cost"]["parts"][0]["PutCounters"][0]["recipients"],
            json!({"kind":"Objects","args":{"kind":"One","args":{"kind":"Target","args":{"slot":1,"index":0}}}})
        );
        let chosen = json!({"kind":"PaymentChosen","args":body[0]["args"]["result"]});
        assert_eq!(
            body[1]["args"]["condition"],
            if decline {
                json!({"kind":"Not","args":chosen})
            } else {
                chosen
            }
        );
        printed_types(reading, &["Sorcery"]);
    }
}

#[test]
fn an_optional_counter_instruction_does_not_shadow_a_counter_payment() {
    let name = "Chisei, Heart of Oceans";
    let text = "Flying\nAt the beginning of your upkeep, sacrifice Chisei unless you remove a counter from a permanent you control.";
    let parsed = engine().parse(name, text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let reading = &parsed.readings[0];
    let payment = &reading[1]["args"]["body"]["instructions"][0]["args"]["operation"];
    assert_eq!(payment["kind"], "PayCost");
    assert_eq!(payment["args"]["optional"], true);
    assert!(payment["args"]["cost"]["parts"][0]["RemoveCounters"].is_object());
    let trail = engine().trail_of(name, reading, text).unwrap().unwrap();
    assert_eq!(
        engine()
            .print_trail(name, reading, &trail)
            .unwrap()
            .as_deref(),
        Some(text),
    );
}
