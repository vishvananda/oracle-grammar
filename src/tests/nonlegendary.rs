use super::*;

#[test]
fn a_copy_population_excludes_the_actual_copied_target_and_prints_edited_types() {
    let text =
        "Each other creature becomes a copy of target nonlegendary creature until end of turn.";
    let mut readings = engine().parse("Context example", text).unwrap().readings;
    assert_eq!(readings.len(), 1);
    let mut reading = readings.remove(0);
    let prefixed = engine().parse("Context example",
        "Tap target creature. Each other artifact becomes a copy of target nonlegendary artifact until end of turn.").unwrap();
    assert_eq!(prefixed.readings.len(), 1);
    let copied =
        &prefixed.readings[0][0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"];
    let expected = json!({"kind":"Target","args":{"slot":1,"index":0}});
    assert_eq!(copied["changes"][0]["args"]["object"], expected);
    let mut excluded = Vec::new();
    visit(&copied["affected"], "SameObject", &mut excluded);
    assert_eq!(excluded, vec![expected]);
    let op = &reading[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"];
    let target = op["changes"][0]["args"]["object"].clone();
    assert_eq!(target, json!({"kind":"Target","args":{"slot":0,"index":0}}));
    let mut excluded = Vec::new();
    visit(&op["affected"], "SameObject", &mut excluded);
    assert_eq!(excluded, vec![target]);
    crate::traverse::rewrite(&mut reading, &mut |v| {
        if v["kind"] == "CardType" && v["args"] == "Creature" {
            v["args"] = json!("Artifact");
        }
        crate::traverse::Visit::Children
    });
    let printed = printed_types(&reading, &["Instant"]);
    assert!(printed.contains("artifact"), "{printed}");
}

#[test]
fn immediate_hand_choices_share_the_selected_card_after_a_prefix() {
    let text = "Draw a card. Target opponent reveals their hand. You choose a nonlegendary, nonland card from it. That player discards that card.";
    let parsed = engine().parse("Context example", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let mut discarded = Vec::new();
    for mut reading in parsed.readings {
        let mut choices = Vec::new();
        visit(&reading, "ChooseObjects", &mut choices);
        assert_eq!(choices.len(), 1);
        let chosen = &choices[0];
        assert_eq!(chosen["objects"]["kind"], "Matching");
        assert_eq!(
            chosen["objects"]["args"]["objects"]["args"]["objects"],
            json!({"kind":"Observed","args":0})
        );
        let mut loss = Vec::new();
        visit(&reading, "DiscardObjects", &mut loss);
        assert_eq!(loss.len(), 1);
        discarded.push(loss[0]["objects"].clone());
        crate::traverse::rewrite(&mut reading, &mut |v| {
            if v["kind"] == "Not" && v["args"] == json!({"kind":"CardType","args":"Land"}) {
                *v = json!({"kind":"CardType","args":"Artifact"});
            }
            crate::traverse::Visit::Children
        });
        let printed = printed_types(&reading, &["Sorcery"]);
        assert!(printed.contains("artifact"), "{printed}");
    }
    assert_eq!(discarded, vec![json!({"kind":"Selected","args":1})]);
}

#[test]
fn normalized_hand_choices_keep_exile_printing_and_current_eligibility() {
    let text =
        "Target opponent reveals their hand. You choose a nonland card from it. Exile that card.";
    let parsed = engine().parse("Context example", text).unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let mut reading = parsed.readings[0].clone();
    crate::traverse::rewrite(&mut reading, &mut |v| {
        if v["kind"] == "Not" && v["args"] == json!({"kind":"CardType","args":"Land"}) {
            *v = json!({"kind":"CardType","args":"Artifact"});
        }
        crate::traverse::Visit::Children
    });
    let text = printed_types(&reading, &["Sorcery"]);
    assert!(text.to_lowercase().contains("artifact"), "{text}");
    assert!(text.to_lowercase().contains("exile"), "{text}");
}
