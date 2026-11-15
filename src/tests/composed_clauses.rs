//! Clauses that ordinary rules now read without a seeded rule: per-opponent targets,
//! main-phase and departure events, activation windows, base stats, parley and
//! hand-to-library moves.
use super::*;

/// The operations of every effect in `reading`, in order.
fn operations(reading: &Value) -> Vec<String> {
    let mut found = Vec::new();
    visit(reading, "Effect", &mut found);
    found
        .iter()
        .map(|effect| effect["operation"]["kind"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn each_opponent_target_is_up_to_one_per_opponent_and_ends_its_sentence() {
    for (text, operation) in [
        (
            "For each opponent, exile up to one target creature that player controls.",
            "Move",
        ),
        (
            "For each opponent, gain control of up to one target creature that player controls \
             until end of turn. Untap those creatures.",
            "CreateContinuousEffect",
        ),
    ] {
        let reading = compositional(text);
        let spec = &reading[0]["args"]["announcement"]["targets"][0];
        assert_eq!(
            spec["count"],
            json!({"PerPlayer":{
                "players":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}},
                "each":{"UpTo":{"kind":"Constant","args":1}}
            }}),
            "{text}"
        );
        assert_eq!(operations(&reading)[0], operation, "{text}");
        printed_types(&reading, &["Sorcery"]);
    }
    // "That player" belongs to the loop; a clause joined by "and" cannot borrow it.
    let joined = engine()
        .parse(
            "Context example",
            "For each opponent, exile up to one target creature that player controls and that \
             player loses 2 life.",
        )
        .map(|parsed| parsed.readings)
        .unwrap_or_default();
    assert!(joined.is_empty(), "{joined:#?}");
}

#[test]
fn each_main_phase_of_your_turn_is_its_own_turn_point() {
    for (text, point) in [
        (
            "At the beginning of each of your main phases, add {G}{G}.",
            "MainPhaseBegins",
        ),
        (
            "At the beginning of each of your postcombat main phases, draw a card.",
            "PostcombatMainPhaseBegins",
        ),
    ] {
        let reading = compositional(text);
        assert_eq!(
            reading[0]["args"]["event"]["Turn"]["point"], point,
            "{text}"
        );
        printed(&reading);
    }
}

#[test]
fn an_activation_window_reads_during_a_named_step() {
    let reading = compositional("{T}: Draw a card. Activate only during combat.");
    let limits = &reading[0]["args"]["limits"];
    assert!(!limits["condition"].is_null(), "{limits}");
    printed(&reading);
}

#[test]
fn base_power_and_toughness_filter_the_printed_or_set_values() {
    let reading = compositional(
        "Whenever another creature you control with base power 1 enters, draw a card.",
    );
    let text = reading.to_string();
    assert!(text.contains("\"BasePower\""), "{text}");
    printed(&reading);
    let reading = compositional("Creatures you control with base power or toughness 1 get +1/+1.");
    let mut either = Vec::new();
    visit(&reading, "AnyOf", &mut either);
    assert!(
        either
            .iter()
            .any(|cases| cases.to_string().contains("BaseToughness")),
        "{reading}"
    );
    printed_types(&reading, &["Enchantment"]);
}

#[test]
fn parley_counts_the_nonland_cards_revealed_this_way() {
    let reading = compositional(
        "Each player reveals the top card of their library. For each nonland card revealed this \
         way, you create a 1/1 white Spirit creature token with flying. Then each player draws \
         a card.",
    );
    assert_eq!(
        operations(&reading),
        ["Reveal", "CreateTokens", "Draw"],
        "{reading}"
    );
    printed_types(&reading, &["Sorcery"]);
}

#[test]
fn cards_from_your_hand_go_to_the_bottom_in_any_order() {
    let reading = compositional(
        "Draw two cards, then put a card from your hand on the bottom of your library.",
    );
    let mut moves = Vec::new();
    visit(&reading, "Move", &mut moves);
    let [destination] = moves
        .iter()
        .map(|args| args["destination"]["Library"].clone())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    assert_eq!(destination["position"], "Bottom", "{destination}");
    assert_eq!(destination["order"], "DestinationDefault", "{destination}");
    printed_types(&reading, &["Sorcery"]);
}

#[test]
fn your_hand_or_graveyard_is_one_pool_for_one_card() {
    let reading = compositional(
        "Whenever a creature you control dies, you may put a land card from your hand or \
         graveyard onto the battlefield tapped.",
    );
    let mut choices = Vec::new();
    visit(&reading, "ChooseObjects", &mut choices);
    let [choice] = choices.try_into().unwrap();
    let zones: Vec<_> = choice["objects"]["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|zone| zone["args"]["scope"]["Players"]["zone"].clone())
        .collect();
    assert_eq!(choice["objects"]["kind"], "Union", "{choice}");
    assert_eq!(zones, [json!("Hand"), json!("Graveyard")], "{choice}");
    printed(&reading);
}

#[test]
fn a_combat_cap_limits_each_group_or_one_defender() {
    let reading = compositional(
        "No more than one creature can attack each combat.\nNo more than two creatures can \
         block each combat.",
    );
    let mut groups = Vec::new();
    visit(&reading, "Rule", &mut groups);
    let caps: Vec<_> = groups
        .iter()
        .map(|rule| {
            let group = &rule["CombatGroup"];
            (group["role"].clone(), group["amount"]["args"].clone())
        })
        .collect();
    assert_eq!(
        caps,
        [(json!("Attacker"), json!(1)), (json!("Blocker"), json!(2))]
    );
    printed_types(&reading, &["Enchantment"]);

    let reading = compositional("No more than two creatures can attack you each combat.");
    assert_eq!(reading[0]["kind"], "AttackCap", "{reading}");
    assert_eq!(reading[0]["args"]["amount"]["args"], 2, "{reading}");
    printed_types(&reading, &["Enchantment"]);
}

#[test]
fn an_activation_cap_counts_twice_or_n_times_each_turn() {
    for (text, most) in [
        (
            "{B}: This creature gets +1/+0 until end of turn. Activate no more than twice each \
             turn.",
            2,
        ),
        (
            "{1}: Add {B} or {R}. Activate no more than three times each turn.",
            3,
        ),
    ] {
        let reading = compositional(text);
        assert_eq!(
            reading[0]["args"]["limits"]["modifiers"],
            json!([{"kind":"ActivationRule","args":{"MaxPerTurn":{"kind":"Constant","args":most}}}]),
            "{text}"
        );
        printed(&reading);
    }
}

#[test]
fn an_alternative_cost_for_the_spells_you_cast_is_your_rule() {
    for (text, spells) in [
        (
            "You may pay {W}{U}{B}{R}{G} rather than pay the mana cost for spells you cast.",
            json!({"kind":"Any"}),
        ),
        (
            "You may pay {1} rather than pay the mana cost for Rune spells you cast.",
            json!({"kind":"Subtype","args":"Rune"}),
        ),
    ] {
        let reading = compositional(text);
        let cost = &reading[0]["args"]["changes"][0]["args"]["SpellCost"];
        assert_eq!(cost["spells"], spells, "{text}");
        assert!(!cost["modifier"]["Alternate"].is_null(), "{cost}");
        printed_types(&reading, &["Artifact"]);
    }
}

#[test]
fn two_cards_from_your_hand_go_on_top_in_any_order() {
    let reading = compositional(
        "Draw three cards, then put two cards from your hand on top of your library in any \
         order.",
    );
    let mut moves = Vec::new();
    visit(&reading, "Move", &mut moves);
    let [destination] = moves
        .iter()
        .map(|args| args["destination"]["Library"].clone())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    assert_eq!(destination["position"], "Top", "{destination}");
    assert_eq!(destination["order"], "DestinationDefault", "{destination}");
    printed_types(&reading, &["Sorcery"]);
    // One card goes on top alone; a plural count takes "in any order".
    let plural = engine()
        .parse(
            "Context example",
            "Draw three cards, then put two cards from your hand on top of your library.",
        )
        .map(|parsed| parsed.readings)
        .unwrap_or_default();
    assert!(plural.is_empty(), "{plural:#?}");
}

#[test]
fn losing_all_abilities_and_becoming_a_colored_creature_type() {
    let reading = compositional(
        "Until end of turn, target creature loses all abilities and becomes a blue Frog with \
         base power and toughness 1/1.",
    );
    let mut effects = Vec::new();
    visit(&reading, "CreateContinuousEffect", &mut effects);
    let kinds: Vec<_> = effects[0]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|change| change["kind"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        kinds,
        [
            "RemoveAllAbilities",
            "Colors",
            "CreatureTypes",
            "SetBasePowerToughness"
        ]
    );
    assert_eq!(effects[0]["duration"], "EndOfTurn");
    printed_types(&reading, &["Instant"]);
}

#[test]
fn a_card_from_a_graveyard_is_any_graveyard_card() {
    let reading = compositional(
        "Exile target creature. Exile up to one target instant or sorcery card from a \
         graveyard.",
    );
    let targets = reading[0]["args"]["announcement"]["targets"]
        .as_array()
        .unwrap();
    assert_eq!(targets.len(), 2, "{reading}");
    assert_eq!(
        targets[1]["kind"],
        json!({"Object":{"kind":"Card","zones":["Graveyard"]}})
    );
    assert_eq!(
        targets[1]["count"],
        json!({"UpTo":{"kind":"Constant","args":1}})
    );
    printed_types(&reading, &["Sorcery"]);
}

#[test]
fn each_card_discarded_this_way_counts_the_discard() {
    let reading = compositional(
        "Discard any number of cards. Creatures you control get +0/+1 until end of turn for \
         each card discarded this way.",
    );
    assert_eq!(
        operations(&reading),
        ["ChooseObjects", "DiscardObjects", "CreateContinuousEffect"],
        "{reading}"
    );
    let text = reading.to_string();
    assert!(
        text.contains(r#"{"kind":"Transition","args":{"phase":"Before","result":1}}"#)
            || text.contains(r#"{"kind":"Transition","args":{"result":1,"phase":"Before"}}"#),
        "{text}"
    );
    printed_types(&reading, &["Instant"]);
}

#[test]
fn a_named_object_leaving_this_turn_triggers_when() {
    for text in [
        "Target creature gets +2/+0 until end of turn. When that creature dies this turn, draw \
         a card.",
        "Target creature gets +2/+2 until end of turn. When that creature leaves the battlefield \
         this turn, you lose 2 life.",
    ] {
        let reading = compositional(text);
        assert_eq!(printed_types(&reading, &["Instant"]), text);
    }
}

#[test]
fn two_cards_go_to_the_bottom_in_any_order() {
    let text = "Draw two cards, then put two cards from your hand on the bottom of your library \
                in any order.";
    let reading = compositional(text);
    assert!(
        printed_types(&reading, &["Sorcery"]).ends_with("in any order."),
        "{reading}"
    );
}
