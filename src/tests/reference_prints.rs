use super::*;

/// The exact text a lowered form prints along the derivation of `text`.
fn keyed(name: &str, abilities: &Value, types: &[String], text: &str) -> Option<String> {
    let key = engine()
        .derivation_of(name, abilities, text)
        .unwrap()
        .expect("the written text is reachable");
    engine().print_keyed(name, abilities, types, &key).unwrap()
}

fn only_reading(name: &str, text: &str) -> Value {
    let mut readings = engine().parse(name, text).unwrap().readings;
    assert_eq!(readings.len(), 1, "{text}");
    readings.remove(0)
}

/// A self-entry trigger may name its X as the entering event's subject's.
#[test]
fn a_self_entry_trigger_prints_its_subjects_cast_x() {
    let name = "Farmer Cotton";
    let text =
        "When Farmer Cotton enters, create X 1/1 white Halfling creature tokens and X Food tokens.";
    let source_x = json!({"kind": "CastX", "args": {"kind": "Source"}}).to_string();
    let subject_x = json!({"kind": "CastX", "args": {"kind": "EventSubject"}}).to_string();
    let written = only_reading(name, text).to_string();
    assert!(written.contains(&source_x), "{written}");
    let lowered: Value = serde_json::from_str(&written.replace(&source_x, &subject_x)).unwrap();
    let types = ["Creature".to_string()];
    assert_eq!(keyed(name, &lowered, &types, text).as_deref(), Some(text));
}

/// Triggers lowered apart join when one body names the dying source as the
/// event's subject and the other names it as the source.
#[test]
fn triggers_join_when_their_bodies_differ_only_in_how_they_name_the_source() {
    let name = "Pashalik Mons";
    let text = "Whenever Pashalik Mons or another Goblin you control dies, Pashalik Mons deals 1 damage to any target.";
    let trigger = |objects: Value, source: &str| {
        json!({"kind": "Triggered", "args": {
            "event": {"ZoneChange": {"objects": objects, "from": "Battlefield", "to": "Graveyard"}},
            "intervening_if": null, "limits": {"per_turn": null, "total": null},
            "announcement": {"targets": [{"slot": 0, "kind": "DamageRecipient",
                "count": {"Exactly": {"kind": "Constant", "args": 1}}, "filter": {"kind": "Any"},
                "players": null, "distribute": null, "constraints": [], "chooser": null}], "modes": null},
            "body": {"instructions": [{"kind": "Effect", "args": {"result": 0, "operation": {
                "kind": "DealDamage", "args": {"source": {"kind": source},
                "recipients": {"kind": "Targets", "args": 0},
                "amount": {"kind": "Constant", "args": 1}, "preventable": true}}}}]}}})
    };
    let itself = json!({"kind": "SameObject", "args": {"kind": "Source"}});
    let another = json!({"kind": "AllOf", "args": [
        {"kind": "Not", "args": itself},
        {"kind": "AllOf", "args": [{"kind": "Subtype", "args": "Goblin"},
            {"kind": "ControlledBy", "args": {"kind": "One", "args": {"kind": "AbilityController"}}}]}]});
    let lowered = json!([trigger(itself, "EventSubject"), trigger(another, "Source")]);
    let types = ["Creature".to_string()];
    assert_eq!(keyed(name, &lowered, &types, text).as_deref(), Some(text));
}

/// A player spared by a sacrifice of their choice or another cost prints once
/// as the one who loses life and once as the one who pays.
#[test]
fn a_sacrifice_or_other_cost_spares_the_player_as_written() {
    let name = "Tergrid's Lantern";
    for text in [
        "{T}: Target player loses 3 life unless they sacrifice a nonland permanent of their choice or discard a card.",
        "When this creature enters, target opponent loses 3 life unless they sacrifice a nonland permanent of their choice or discard a card.",
    ] {
        let reading = only_reading(name, text);
        assert_eq!(keyed(name, &reading, &[], text).as_deref(), Some(text));
    }
}

/// Triggers written one per event print split from the one trigger on either.
#[test]
fn triggers_written_per_event_print_split_from_the_trigger_on_either() {
    for (name, types, text) in [
        (
            "Necklace of Girion",
            "Artifact",
            "Whenever you cast a green spell and whenever a Forest you control enters, put a +1/+1 counter on target creature you control.",
        ),
        (
            "Perimeter Enforcer",
            "Creature",
            "Whenever another Detective you control enters and whenever a Detective you control is turned face up, this creature gets +1/+1 until end of turn.",
        ),
    ] {
        let reading = only_reading(name, text);
        let types = [types.to_string()];
        if keyed(name, &reading, &types, text).as_deref() == Some(text) {
            continue;
        }
        let trail = engine()
            .trail_of(name, &reading, text)
            .unwrap()
            .expect("the written order is reachable");
        let printed = engine().print_trail(name, &reading, &trail).unwrap();
        assert_eq!(printed.as_deref(), Some(text), "{name}");
    }
}
