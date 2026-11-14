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
        assert_prints_as_written(name, types, text);
    }
}

/// The only reading of `text` prints it back along its derivation, else
/// along its written order.
fn assert_prints_as_written(name: &str, types: &str, text: &str) {
    let reading = only_reading(name, text);
    let types = [types.to_string()];
    if keyed(name, &reading, &types, text).as_deref() == Some(text) {
        return;
    }
    let trail = engine()
        .trail_of(name, &reading, text)
        .unwrap()
        .expect("the written order is reachable");
    let printed = engine().print_trail(name, &reading, &trail).unwrap();
    assert_eq!(printed.as_deref(), Some(text), "{name}");
}

/// A where-X may name a count of chosen objects or the varying part of a sum.
#[test]
fn where_x_names_a_chosen_count_or_the_part_of_a_sum() {
    for (name, types, text) in [
        (
            "An-Havva Inn",
            "Sorcery",
            "You gain X plus 1 life, where X is the number of green creatures on the battlefield.",
        ),
        (
            "Hellfire",
            "Sorcery",
            "Destroy all nonblack creatures. Hellfire deals X plus 3 damage to you, where X is the number of creatures that died this way.",
        ),
        (
            "Uncle's Musings",
            "Sorcery",
            "Converge — Return up to X permanent cards from your graveyard to your hand, where X is the number of colors of mana spent to cast this spell.\nExile Uncle's Musings.",
        ),
    ] {
        assert_prints_as_written(name, types, text);
    }
}

/// A later use of a triggering object may name it by its subtype.
#[test]
fn a_triggering_object_is_named_by_its_subtype() {
    for (name, types, text) in [
        (
            "Spined Sliver",
            "Creature",
            "Whenever a Sliver becomes blocked, that Sliver gets +1/+1 until end of turn for each creature blocking it.",
        ),
        (
            "Captain America, Team Leader",
            "Creature",
            "Whenever another Hero you control enters, it gains vigilance and haste until end of turn. Put a +1/+1 counter on that Hero and a +1/+1 counter on Captain America.",
        ),
        (
            "Battering Ram",
            "Creature",
            "Whenever this creature becomes blocked by a Wall, destroy that Wall at end of combat.",
        ),
        (
            "Greatbow Doyen",
            "Creature",
            "Whenever an Archer you control deals damage to a creature, that Archer deals that much damage to that creature's controller.",
        ),
    ] {
        assert_prints_as_written(name, types, text);
    }
}

/// One program written once may stand in both branches of a choice.
#[test]
fn a_program_in_both_branches_prints_once() {
    assert_prints_as_written(
        "Yennett, Cryptic Sovereign",
        "Creature",
        "Whenever Yennett attacks, reveal the top card of your library. You may cast it without paying its mana cost if its mana value is odd. If you don't cast it, draw a card.",
    );
}

/// A target named in a repeated operand still lets a later use read as a pronoun.
#[test]
fn a_target_named_twice_is_a_pronoun_afterwards() {
    for (name, types, text) in [
        (
            "Gift of the Viper",
            "Instant",
            "Put a +1/+1 counter, a reach counter, and a deathtouch counter on target creature. Untap it.",
        ),
        (
            "Natural Selection",
            "Instant",
            "Look at the top three cards of target player's library, then put them back in any order. You may have that player shuffle.",
        ),
        (
            "Diplomacy of the Wastes",
            "Sorcery",
            "Target opponent reveals their hand. You choose a nonland card from it. That player discards that card. If you control a Warrior, that player loses 2 life.",
        ),
    ] {
        assert_prints_as_written(name, types, text);
    }
}
