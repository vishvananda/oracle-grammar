//! A `where X is` clause names a counted or aggregate value; the counted
//! population keeps its own zone, owner and counter recipient.
use super::*;

fn amounts(reading: &Value, kind: &str) -> Vec<Value> {
    let mut found = Vec::new();
    visit(reading, kind, &mut found);
    found
}

#[test]
fn entering_counters_take_their_number_from_the_where_clause() {
    let text = "This creature enters with X +1/+1 counters on it, where X is the number of \
                creature cards in all graveyards.";
    let reading = compositional(text);
    let counted = amounts(&reading, "CountObjects");
    assert_eq!(
        counted[0]["args"]["scope"],
        json!({"Players":{"zone":"Graveyard","players":{"kind":"All"}}})
    );
    assert!(amounts(&reading, "AnnouncedX").is_empty());
    assert_eq!(printed(&reading), text);
}

#[test]
fn named_card_zones_keep_their_owner() {
    let text = "Whenever this creature attacks, it gets +X/+0 until end of turn, where X is the \
                number of creature cards in defending player's graveyard.";
    let readings = engine().parse("Context example", text).unwrap().readings;
    assert!(!readings.is_empty());
    for reading in &readings {
        let counted = amounts(reading, "CountObjects");
        assert_eq!(
            counted[0]["args"]["scope"]["Players"]["players"]["args"]["kind"],
            "DefendingPlayerOf"
        );
    }
}

#[test]
fn counts_of_players_names_counters_and_totals_are_values() {
    for (text, kind) in [
        (
            "Draw X cards, where X is the number of opponents you have.",
            "CountPlayers",
        ),
        (
            "Draw X cards, where X is the number of differently named lands you control.",
            "CountDistinct",
        ),
        (
            "Draw X cards, where X is the total power of creatures you control.",
            "Aggregate",
        ),
        (
            "Draw X cards, where X is the number of experience counters you have.",
            "CountCounters",
        ),
        (
            "Draw X cards, where X is the number of tokens you created this turn.",
            "EventHistory",
        ),
        (
            "Draw X cards, where X is the number of spells you've cast this turn.",
            "EventHistory",
        ),
    ] {
        let readings = engine().parse("Context example", text).unwrap().readings;
        assert!(!readings.is_empty(), "{text}");
        for reading in &readings {
            assert!(amounts(reading, "AnnouncedX").is_empty(), "{text}");
            assert_eq!(amounts(reading, kind).len(), 1, "{text}");
        }
    }
}

#[test]
fn shared_x_is_captured_after_the_prefix_and_prints_from_edited_operands() {
    for (text, prefix, object) in [
        (
            "Destroy target creature. You draw X cards and you lose X life, where X is the \
             number of counters on that creature.",
            "Destroy",
            json!({"kind":"Target","args":{"slot":0,"index":0}}),
        ),
        (
            "Put a +1/+1 counter on this creature. Then you draw X cards and lose X life, \
             where X is the number of counters on this creature.",
            "PutCounters",
            json!({"kind":"Source"}),
        ),
    ] {
        let mut reading = compositional(text);
        let instructions = reading[0]["args"]["body"]["instructions"]
            .as_array_mut()
            .unwrap();
        let kinds: Vec<_> = instructions
            .iter()
            .map(|i| i["args"]["operation"]["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, [prefix, "BindValue", "Draw", "LoseLife"]);
        let definition = &instructions[1]["args"]["operation"]["args"]["value"];
        assert_eq!(definition["kind"], "CountCounters");
        assert_eq!(definition["args"]["recipients"]["args"]["args"], object);
        assert_eq!(definition["args"]["information"], "CurrentOrLastKnown");
        for instruction in &instructions[2..] {
            assert_eq!(
                instruction["args"]["operation"]["args"]["amount"],
                json!({"kind":"BoundValue","args":instructions[1]["args"]["result"]})
            );
        }
        printed_types(&reading, &["Sorcery"]);
        reading[0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["value"]["args"]
            ["kind"] = json!({"Named":"ChargeCounter"});
        let edited = printed_types(&reading, &["Sorcery"]);
        assert!(edited.contains("charge counters"), "{edited}");
    }
}

#[test]
fn shared_snapshot_inverse_preserves_capture_position_and_unrelated_results() {
    use crate::pattern::{Bindings, Pattern};
    let text = "Draw a card. You draw X cards. Destroy target artifact. You lose X life, \
                where X is the number of cards in your hand.";
    let reading = compositional(text);
    let body = &reading[0]["args"]["body"];
    let instructions = body["instructions"].as_array().unwrap();
    assert_eq!(instructions[0]["args"]["operation"]["kind"], "Draw");
    assert_eq!(instructions[1]["args"]["operation"]["kind"], "BindValue");
    assert_eq!(instructions[3]["args"]["operation"]["kind"], "Destroy");
    printed_types(&reading, &["Sorcery"]);

    let holes = [
        Pattern::Hole("program".into(), crate::model::ir_schema::Ty::Opaque),
        Pattern::Hole("value".into(), crate::model::ir_schema::Ty::Opaque),
    ];
    let inverse = |program: &Value| {
        crate::functions::value_snapshot::unify(&holes, program, Bindings::new()).len()
    };
    assert_eq!(inverse(body), 1);
    let mut moved = body.clone();
    moved["instructions"].as_array_mut().unwrap().swap(0, 1);
    assert_eq!(inverse(&moved), 0, "a capture cannot cross the first draw");
    let mut moved = body.clone();
    moved["instructions"].as_array_mut().unwrap().swap(1, 2);
    assert_eq!(inverse(&moved), 0, "a capture cannot follow its first use");
}

#[test]
fn kinded_counter_definitions_share_the_same_snapshot_contract() {
    for text in [
        "When this creature dies, you draw X cards and you lose X life, where X is the \
         number of +1/+1 counters on it.",
        "Put a death counter on this creature. You draw X cards and you lose X life, \
         where X is the number of death counters on this creature.",
    ] {
        let reading = compositional(text);
        assert_eq!(amounts(&reading, "CountCounters").len(), 1);
        assert_eq!(amounts(&reading, "BindValue").len(), 1);
        let uses = amounts(&reading, "BoundValue");
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0], uses[1]);
        printed_types(&reading, &["Creature"]);
    }
}

#[test]
fn card_populations_share_zones_without_losing_their_filters() {
    for (population, kind, zone, player, filter) in [
        (
            "cards named Context example in target player's library",
            "CountObjects",
            "Library",
            "Target",
            "Name",
        ),
        (
            "cards with a cycling ability in all players' hands",
            "CountObjects",
            "Hand",
            "All",
            "HasActivatedAbility",
        ),
        (
            "card types among creature cards in all graveyards",
            "CountDistinct",
            "Graveyard",
            "All",
            "CardType",
        ),
    ] {
        let text =
            format!("You draw X cards and you lose X life, where X is the number of {population}.");
        let reading = compositional(&text);
        let values = amounts(&reading, kind);
        assert_eq!(values.len(), 1);
        let objects = if kind == "CountObjects" {
            &values[0]
        } else {
            &values[0]["objects"]
        };
        assert_eq!(objects["kind"], "Zone");
        assert_eq!(objects["args"]["scope"]["Players"]["zone"], zone);
        let players = &objects["args"]["scope"]["Players"]["players"];
        assert_eq!(
            if player == "All" {
                &players["kind"]
            } else {
                &players["args"]["kind"]
            },
            player
        );
        assert_eq!(objects["args"]["filter"]["kind"], filter);
        printed(&reading);
    }
}

#[test]
fn editing_a_named_card_population_prints_its_current_zone() {
    let mut reading = compositional(
        "This creature enters with X +1/+1 counters on it, where X is the number of \
         cards named Context example in your graveyard.",
    );
    // The shared value can occur inside the replacement tree; edit its typed zone.
    fn library(value: &mut Value) {
        if value["kind"] == "Zone" && value["args"]["filter"]["kind"] == "Name" {
            value["args"]["scope"]["Players"]["zone"] = json!("Library");
        }
        match value {
            Value::Object(fields) => fields.values_mut().for_each(library),
            Value::Array(items) => items.iter_mut().for_each(library),
            _ => {}
        }
    }
    library(&mut reading);
    let text = printed(&reading);
    assert!(text.contains("your library"), "{text}");
    assert!(!text.contains("graveyard"), "{text}");
}

#[test]
fn a_same_name_population_keeps_the_cast_spell_and_its_own_zone() {
    let text = "Whenever you cast a spell, you draw X cards and you lose X life, where X \
                is the number of cards in your library with the same name as that spell.";
    let reading = compositional(text);
    let counted = amounts(&reading, "CountObjects");
    assert_eq!(counted.len(), 1);
    assert_eq!(counted[0]["args"]["scope"]["Players"]["zone"], "Library");
    assert_eq!(
        counted[0]["args"]["filter"],
        json!({"kind":"SharesNameWith","args":{"kind":"EventSubject"}})
    );
    printed(&reading);
}
