//! Regression tests for tokenizer self-name boundaries, energy event quantities,
//! unique sacrifice cost lookup, single-target player-action move receipts,
//! exile-until continuation visibility, search-move referents, and owner-return role preservation.
use crate::bind;
use crate::tokens::{Names, Token, detokenize, tokenize};
use rustc_hash::FxHashSet as HashSet;
use serde_json::{Value, json};

fn bind_one(mut ability: Value) -> Result<Value, String> {
    bind::bind_ability(&mut ability, "Example")?;
    Ok(ability)
}

fn vocab(words: &[&str]) -> HashSet<String> {
    words.iter().map(|word| (*word).to_string()).collect()
}

#[test]
fn exclamation_self_name_absorbs_sentence_period_and_roundtrips() {
    let names = Names::of("Blood for the Blood God!");
    let words = vocab(&[
        "discard", "your", "hand", "deals", "exile", "draw", "a", "card",
    ]);
    let text = "Blood for the Blood God! deals 8 damage. Exile Blood for the Blood God!";
    let tokens = tokenize(text, &names, &words).unwrap();
    assert_eq!(detokenize(&tokens, &names), text);
    assert_eq!(tokens.first(), Some(&Token::SelfName(0)));
    assert_eq!(tokens.get(1), Some(&Token::Word("deals".into())));
    assert_eq!(tokens.get(tokens.len() - 2), Some(&Token::SelfName(0)));
    assert_eq!(tokens.last(), Some(&Token::Mark('.')));

    let two_sentences = "Exile Blood for the Blood God! Draw a card.";
    let tokens2 = tokenize(two_sentences, &names, &words).unwrap();
    assert_eq!(detokenize(&tokens2, &names), two_sentences);
    assert!(tokens2.contains(&Token::Word("draw".into())));
}

#[test]
fn self_name_does_not_fire_on_type_modifier_adjective_noun_or_named_suffix() {
    let words = vocab(&[
        "target",
        "creature",
        "gets",
        "is",
        "a",
        "colorless",
        "with",
        "named",
        "of",
        "if",
        "control",
        "conjure",
        "card",
        "create",
        "token",
    ]);

    let worker = Names::of("Assembly-Worker");
    let worker_toks = tokenize(
        "Target Assembly-Worker creature gets +1/+1.",
        &worker,
        &words,
    )
    .unwrap();
    assert!(
        !worker_toks.iter().any(|t| matches!(t, Token::SelfName(_))),
        "{worker_toks:?}"
    );

    let noggle = Names::of("Noggle the Mind");
    let noggle_toks = tokenize("Is a colorless Noggle with +1/+1.", &noggle, &words).unwrap();
    assert!(
        !noggle_toks.iter().any(|t| matches!(t, Token::SelfName(_))),
        "{noggle_toks:?}"
    );

    let kookus = Names::of("Kookus");
    let kookus_toks = tokenize(
        "If control a creature named Keeper of Kookus, Kookus.",
        &kookus,
        &words,
    )
    .unwrap();
    let self_positions: Vec<usize> = kookus_toks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| matches!(t, Token::SelfName(_)).then_some(i))
        .collect();
    assert_eq!(self_positions.len(), 1, "{kookus_toks:?}");

    let hawk = Names::of("Squadron Hawk");
    let hawk_toks = tokenize("A card named Squadron Hawk.", &hawk, &words).unwrap();
    assert!(
        hawk_toks.iter().any(|t| matches!(t, Token::SelfName(0))),
        "{hawk_toks:?}"
    );

    let angel = Names::of("Deathpact Angel");
    let angel_toks = tokenize(
        "Return a permanent named Deathpact Angel card from your graveyard to the battlefield.",
        &angel,
        &vocab(&[
            "return",
            "a",
            "permanent",
            "named",
            "card",
            "from",
            "your",
            "graveyard",
            "to",
            "the",
            "battlefield",
        ]),
    )
    .unwrap();
    assert!(
        angel_toks.iter().any(|t| matches!(t, Token::SelfName(0))),
        "{angel_toks:?}"
    );
}

#[test]
fn energy_trigger_exposes_event_amount_and_unique_sacrifice_cost_part() {
    let raw = json!({
        "kind": "Triggered",
        "args": {
            "event": {"Player": {"event": "GetsEnergy", "players": {"kind": "One", "args": {"kind": "AbilityController"}}, "objects": null}},
            "intervening_if": null,
            "limits": {"per_turn": null, "total": null},
            "announcement": {"targets": [], "modes": null},
            "body": {"instructions": [{
                "kind": "Effect",
                "args": {
                    "result": {"$sym": "next", "args": [], "at": 0},
                    "operation": {
                        "kind": "DealDamage",
                        "args": {
                            "source": {"kind": "Source"},
                            "recipients": {"kind": "Players", "args": {"kind": "OpponentsOf", "args": {"kind": "AbilityController"}}},
                            "amount": {"$sym": "that_much", "args": [], "at": 1},
                            "preventable": true
                        }
                    }
                }
            }]}
        }
    });
    let bound = bind_one(raw).unwrap();
    assert_eq!(
        bound["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"],
        json!({"kind": "EventAmount"})
    );

    let dual_sac = json!({
        "kind": "Activated",
        "args": {
            "origin": "Ordinary",
            "cost": {"parts": [
                {"Sacrifice": {"objects": {"kind": "One", "args": {"kind": "Source"}}, "choose": null}},
                {"Sacrifice": {"objects": {"kind": "Zone", "args": {"scope": {"Shared": "Battlefield"}, "kind": "Permanent", "filter": {"kind": "CardType", "args": "Artifact"}}}, "choose": {"Exactly": {"kind": "Constant", "args": 1}}}}
            ]},
            "limits": {"once": false, "once_each_turn": false, "sorcery": false, "condition": null, "activators": null, "keyword": null, "modifiers": []},
            "announcement": {"targets": [], "modes": null},
            "body": {"instructions": [{
                "kind": "If",
                "args": {
                    "condition": {"kind": "Compare", "args": {
                        "left": {"kind": "CountObjects", "args": {"kind": "CostObjects", "args": {
                            "payment": "Activation",
                            "component": [{"$sym": "cost_index", "args": [], "at": 0}],
                            "view": "Performed",
                            "phase": "Before"
                        }}},
                        "test": {"kind": "Compare", "args": {"comparison": "Greater", "value": {"kind": "Constant", "args": 0}}}
                    }},
                    "then": {"instructions": []},
                    "otherwise": {"instructions": []}
                }
            }]}
        }
    });
    assert!(bind_one(dual_sac).is_err());
}

#[test]
fn player_action_single_target_move_payment_supplies_card_receipt() {
    let target_spec = json!({
        "slot": {"$sym": "slot", "args": [], "at": 0},
        "kind": {"Object": {"kind": "Card", "zones": ["Graveyard"]}},
        "count": {"Exactly": {"kind": "Constant", "args": 1}},
        "filter": {"kind": "CardType", "args": "Creature"},
        "players": null,
        "distribute": null,
        "constraints": [],
        "chooser": null
    });
    let move_effect = json!({
        "kind": "Effect",
        "args": {
            "result": 1048576,
            "operation": {
                "kind": "Move",
                "args": {
                    "objects": {"kind": "One", "args": {"kind": "Target", "args": {"slot": {"$sym": "target", "args": [target_spec], "at": 1}, "index": 0}}},
                    "destination": {"Library": {"position": "Top", "order": "DestinationDefault", "arrival": {"counters": [], "states": []}}},
                    "qualifiers": []
                }
            }
        }
    });
    let pay_effect = json!({
        "kind": "Effect",
        "args": {
            "result": {"$sym": "next", "args": [], "at": 0},
            "operation": {
                "kind": "PayCost",
                "args": {
                    "player": {"kind": "AbilityController"},
                    "optional": true,
                    "cost": {"parts": [{"PlayerAction": {"player": {"kind": "AbilityController"}, "body": {"instructions": [move_effect]}}}]}
                }
            }
        }
    });
    let damage_effect = json!({
        "kind": "Effect",
        "args": {
            "result": {"$sym": "next", "args": [], "at": 3},
            "operation": {
                "kind": "DealDamage",
                "args": {
                    "source": {"kind": "Source"},
                    "recipients": {"kind": "Players", "args": {"kind": "OpponentsOf", "args": {"kind": "AbilityController"}}},
                    "amount": {"kind": "ObjectProperty", "args": {
                        "object": {"$sym": "card_receipt", "args": [], "at": 4},
                        "property": "Power",
                        "information": "CurrentOrLastKnown"
                    }},
                    "preventable": true
                }
            }
        }
    });
    let raw = json!({
        "kind": "Spell",
        "args": {
            "announcement": {"$sym": "announcement", "args": [], "at": 0},
            "body": {"instructions": [
                pay_effect,
                {
                    "kind": "If",
                    "args": {
                        "condition": {"kind": "PaymentChosen", "args": {"$sym": "prior", "args": [], "at": 2}},
                        "then": {"instructions": [damage_effect]},
                        "otherwise": {"instructions": []}
                    }
                }
            ]}
        }
    });
    let bound = bind_one(raw).unwrap();
    assert_eq!(
        bound["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0]["args"]["operation"]
            ["args"]["amount"]["args"]["object"],
        json!({"kind": "Target", "args": {"slot": 0, "index": 0}})
    );
}

#[test]
fn exile_until_receipt_reaches_later_instructions_and_target_libraries() {
    let raw = json!({
        "kind": "Spell",
        "args": {
            "announcement": {"targets": [{"slot": 0, "kind": "Player", "count": {"Exactly": {"kind": "Constant", "args": 1}}, "filter": {"kind": "Any"}, "players": {"kind": "OpponentsOf", "args": {"kind": "AbilityController"}}, "distribute": null, "constraints": [], "chooser": null}], "modes": null},
            "body": {"instructions": [
                {
                    "kind": "Effect",
                    "args": {
                        "result": {"$sym": "mark", "args": [], "at": 0},
                        "operation": {
                            "kind": "RevealUntil",
                            "args": {
                                "library": {"kind": "Target", "args": {"slot": 0, "index": 0}},
                                "filter": {"kind": "Not", "args": {"kind": "CardType", "args": "Land"}},
                                "count": {"kind": "Constant", "args": 1},
                                "at_most": null
                            }
                        }
                    }
                },
                {
                    "kind": "Effect",
                    "args": {
                        "result": {"$sym": "next", "args": [], "at": 1},
                        "operation": {
                            "$sym": "exiled_card_result",
                            "args": [{
                                "kind": "Move",
                                "args": {
                                    "objects": {"kind": "Observed", "args": {"$sym": "marked", "args": [], "at": 1}},
                                    "destination": {"Exile": {"visibility": "FaceUp", "arrival": {"counters": [], "states": []}}},
                                    "qualifiers": []
                                }
                            }],
                            "at": 1
                        }
                    }
                },
                {
                    "kind": "Effect",
                    "args": {
                        "result": {"$sym": "next", "args": [], "at": 2},
                        "operation": {
                            "kind": "PutCounters",
                            "args": {
                                "placements": [{
                                    "recipients": {"kind": "Objects", "args": {"kind": "One", "args": {"$sym": "exiled_card_reference", "args": [], "at": 2}}},
                                    "kind": {"Named": "TimeCounter"},
                                    "amount": {"kind": "Constant", "args": 2},
                                    "selection": null
                                }]
                            }
                        }
                    }
                },
                {
                    "kind": "Effect",
                    "args": {
                        "result": {"$sym": "next", "args": [], "at": 3},
                        "operation": {
                            "kind": "Move",
                            "args": {
                                "objects": {"kind": "One", "args": {"$sym": "exiled_card_reference", "args": [], "at": 3}},
                                "destination": {"OwnersHand": {"counters": [], "states": []}},
                                "qualifiers": []
                            }
                        }
                    }
                }
            ]}
        }
    });
    let bound = bind_one(raw).unwrap();
    let expected_ref = json!({
        "kind": "Query",
        "args": {
            "objects": {
                "kind": "Matching",
                "args": {
                    "objects": {"kind": "Transition", "args": {"result": 1, "phase": "After"}},
                    "filter": {"kind": "Not", "args": {"kind": "CardType", "args": "Land"}}
                }
            },
            "index": 0
        }
    });
    assert_eq!(
        bound["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["placements"][0]["recipients"]
            ["args"]["args"],
        expected_ref
    );
    assert_eq!(
        bound["args"]["body"]["instructions"][3]["args"]["operation"]["args"]["objects"]["args"],
        expected_ref
    );
}

#[test]
fn search_move_registers_qualified_subtype_referent() {
    let eng = crate::test_support::engine();
    let parsed = eng
        .parse(
            "Dragon Caller",
            "Search your library for a Dragon permanent card, put that card onto the battlefield, then shuffle. That Dragon gains haste until end of turn.",
        )
        .unwrap();
    assert!(
        !parsed.readings.is_empty(),
        "expected compositional reading for search-to-battlefield followed by subtype referent"
    );
    let steps = &parsed.readings[0][0]["args"]["body"]["instructions"];
    let move_result = &steps[1]["args"]["result"];
    assert_eq!(
        steps[3]["args"]["operation"]["args"]["affected"],
        json!({"Objects": {"kind": "One", "args": {"kind": "Result", "args": {"result": move_result, "phase": "After", "index": 0}}}})
    );
}

#[test]
fn same_object_owner_return_preserves_opponent_controller_role() {
    let eng = crate::test_support::engine();
    let parsed = eng
        .parse(
            "Bounce Study",
            "When this creature enters, return target creature an opponent controls to its owner's hand. That opponent loses 2 life.",
        )
        .unwrap();
    assert_eq!(parsed.readings.len(), 1);
    let steps = &parsed.readings[0][0]["args"]["body"]["instructions"];
    assert_eq!(
        steps[1]["args"]["operation"]["args"]["players"],
        json!({"kind": "One", "args": {"kind": "ControllerOf", "args": {"kind": "Target", "args": {"slot": 0, "index": 0}}}})
    );

    let owner_player = eng
        .parse(
            "Bounce Study",
            "When this creature enters, return target creature an opponent controls to its owner's hand. That player discards a card.",
        )
        .unwrap();
    assert_eq!(owner_player.readings.len(), 1);
    let steps2 = &owner_player.readings[0][0]["args"]["body"]["instructions"];
    assert_eq!(
        steps2[1]["args"]["operation"]["args"]["players"],
        json!({"kind": "One", "args": {"kind": "OwnerOf", "args": {"kind": "Target", "args": {"slot": 0, "index": 0}}}})
    );
}
