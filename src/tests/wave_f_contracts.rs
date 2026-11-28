//! Regression tests for Wave F tokenizer, single-stage bare Station context, and
//! same-property filter canonicalization contracts.
use crate::engine::{CardNote, PrintedPowerToughness};
use crate::model::ir::Characteristics;
use crate::model::mtg_types::{CardType, SubType};
use crate::test_support::engine;
use crate::tokens::{Names, Token, detokenize, tokenize};
use rustc_hash::FxHashSet as HashSet;
use serde_json::json;

fn vocab(words: &[&str]) -> HashSet<String> {
    words.iter().map(|word| (*word).to_string()).collect()
}

#[test]
fn self_name_does_not_fire_after_each_quantifier() {
    let names = Names::of("Garruk, Cursed Huntsman");
    let words = vocab(&[
        "put", "a", "loyalty", "counter", "on", "each", "you", "control",
    ]);
    let text = "Put a loyalty counter on each Garruk you control.";
    let tokens = tokenize(text, &names, &words).unwrap();
    assert_eq!(detokenize(&tokens, &names), text);
    assert!(
        !tokens.iter().any(|t| matches!(t, Token::SelfName(_))),
        "{tokens:?}"
    );
    assert!(tokens.contains(&Token::Word("Garruk".into())), "{tokens:?}");
}

#[test]
fn single_stage_bare_station_animates_while_multi_stage_bare_station_refuses() {
    let engine = engine();
    let chars = Characteristics {
        supertypes: vec![],
        card_types: vec![CardType::Artifact],
        subtypes: vec![SubType::Spacecraft],
        mana_cost: None,
        color_indicator: vec![],
        power_toughness: Some((4, 4)),
        loyalty: None,
        defense: None,
    };
    let one_stage = "Station\n10+ | Creatures you control have flying.";
    let ctx = engine
        .card_input_context(
            "Squadron Carrier",
            one_stage,
            &chars,
            PrintedPowerToughness::Known {
                power: 4,
                toughness: 4,
            },
        )
        .unwrap();
    let parsed = engine
        .parse_card_with_source("Squadron Carrier", one_stage, &ctx)
        .unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let reading = &parsed.readings[0];
    assert_eq!(reading.abilities[1]["args"]["stages"][0]["animates"], true);
    assert_eq!(
        reading.abilities[1]["args"]["stages"][0]["power_toughness"],
        json!([4, 4])
    );
    assert_eq!(
        reading.notes,
        vec![CardNote::StationPowerToughness {
            ability: 1,
            stage: 0
        }]
    );

    let two_stages = "Station\n2+ | Other creatures you control get +1/+1.\n12+ | Flying, lifelink";
    let two_ctx = engine
        .card_input_context(
            "Lumen-Class Frigate",
            two_stages,
            &chars,
            PrintedPowerToughness::Known {
                power: 4,
                toughness: 4,
            },
        )
        .unwrap();
    assert!(
        engine
            .parse_card_with_source("Lumen-Class Frigate", two_stages, &two_ctx)
            .is_err()
    );
}

#[test]
fn printed_resource_symbol_is_a_distinct_spaced_mark() {
    let names = Names::of("Example");
    let text = "❖ ❖ — Draw two cards.";
    let words = vocab(&["draw", "two", "cards"]);
    let tokens = tokenize(text, &names, &words).unwrap();
    assert_eq!(tokens[0], Token::Mark('❖'));
    assert_eq!(tokens[1], Token::Mark('❖'));
    assert_eq!(detokenize(&tokens, &names), text);
    assert!(tokenize("❖❖ — Draw two cards.", &names, &words).is_err());
}
