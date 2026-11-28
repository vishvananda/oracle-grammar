use crate::{canonical, ir::*, validate};
use serde_json::{Value, json};

mod audit;
mod bindings;
mod created;
mod departures;
mod entry;
mod event_order;
mod filters;
mod json_key_order;

// Reuse the existing canonical regression suite without building the export CLI.
mod regressions {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/canonical.rs"));
}

fn draw(result: u32) -> Instruction {
    Instruction::Effect {
        result: ResultId(result),
        operation: Operation::Draw {
            players: PlayerSet::One(PlayerRef::AbilityController),
            amount: Expr::Constant(1),
        },
    }
}

fn spell(instructions: Vec<Instruction>) -> Ability {
    Ability::Spell {
        announcement: Announcement {
            targets: vec![],
            modes: None,
        },
        body: Program { instructions },
    }
}

fn normalized(ability: &Ability) -> Vec<Ability> {
    let mut value = serde_json::to_value(vec![ability]).unwrap();
    canonical::abilities(&mut value).unwrap();
    serde_json::from_value(value).unwrap()
}

fn preserved_fixture(mut value: Value) -> Card {
    // These immutable captures use the pre-180 entry representation. Upgrade
    // that representation explicitly rather than relabeling it as current.
    value["schema_version"] = json!(179);
    let old = json!({"schema_version":179,"layout":"Card","data":[],"faces":[value]});
    crate::migrate::read_json(&old.to_string())
        .unwrap()
        .faces
        .remove(0)
}

#[test]
fn unread_definitions_have_distinct_deterministic_ids_and_remain_valid() {
    let first = normalized(&spell(vec![draw(17), draw(42)]));
    let renamed = normalized(&spell(vec![draw(300), draw(2)]));
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(renamed).unwrap()
    );
    validate::validate_ability(&first[0]).unwrap();
    let Ability::Spell { body, .. } = &first[0] else {
        panic!()
    };
    let ids: Vec<_> = body
        .instructions
        .iter()
        .map(|instruction| match instruction {
            Instruction::Effect { result, .. } => result.0,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(ids, vec![u32::MAX, u32::MAX - 1]);
    assert_eq!(
        serde_json::to_value(normalized(&first[0])).unwrap(),
        serde_json::to_value(first).unwrap()
    );
}

#[test]
fn complete_ability_list_validation_rejects_unbound_reads_and_unreduced_forms() {
    let valid = spell(vec![draw(7), draw(42)]);
    validate::abilities(&[valid]).unwrap();
    let bad = spell(vec![Instruction::Effect {
        result: ResultId(1),
        operation: Operation::Reveal {
            objects: ObjectSet::Transition {
                result: ResultId(7),
                phase: ObjectPhase::After,
            },
        },
    }]);
    assert!(validate::abilities(&[bad]).is_err());
    let named: Ability = serde_json::from_value(json!({"kind":"Named","args":{
        "source_type":"Fixture", "form":"Unreduced", "operands":[]
    }}))
    .unwrap();
    assert!(
        validate::abilities(&[named])
            .unwrap_err()
            .contains("unreduced")
    );
}

#[test]
fn used_results_stay_bound_and_referenced_duplicate_definitions_stay_invalid() {
    let read = Instruction::Effect {
        result: ResultId(50),
        operation: Operation::Reveal {
            objects: ObjectSet::Transition {
                result: ResultId(17),
                phase: ObjectPhase::After,
            },
        },
    };
    let first = normalized(&spell(vec![draw(17), draw(42), read.clone()]));
    validate::validate_ability(&first[0]).unwrap();
    let value = serde_json::to_value(&first).unwrap();
    assert_eq!(
        value[0]["args"]["body"]["instructions"][0]["args"]["result"],
        0
    );
    assert_eq!(
        value[0]["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["objects"]["args"]
            ["result"],
        0
    );
    let invalid = normalized(&spell(vec![draw(17), draw(17), read]));
    assert!(
        validate::validate_ability(&invalid[0])
            .unwrap_err()
            .contains("duplicate result")
    );
}

#[test]
fn all_selected_unsealed_gold_fixtures_validate_before_and_after_normalization() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!("fixtures.json")).unwrap();
    for value in fixtures {
        let before = preserved_fixture(value);
        validate::card(&before).unwrap();
        let after = canonical::card(&before).unwrap();
        validate::card(&after).unwrap_or_else(|why| panic!("{}: {why}", before.name));
        assert_eq!(
            serde_json::to_value(canonical::card(&after).unwrap()).unwrap(),
            serde_json::to_value(&after).unwrap(),
            "{}",
            before.name
        );
        if before.name == "Guile" {
            let value = serde_json::to_value(after).unwrap();
            let target = &value["abilities"][1]["args"]["body"]["instructions"][1]["args"]["body"]
                ["instructions"][0]["args"]["operation"]["args"]["cards"]["args"];
            assert_eq!(target["kind"], "Query");
        }
    }
}

#[test]
fn flattened_typecycling_still_requires_its_exact_search_reveal_move_shuffle_contract() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!("fixtures.json")).unwrap();
    let value = fixtures
        .into_iter()
        .find(|v| v["name"] == "Alabaster Host Intercessor")
        .unwrap();
    let card = preserved_fixture(value);
    let canonical = canonical::card(&card).unwrap();
    for mutation in 0..3 {
        let mut invalid = canonical.clone();
        let Ability::Scoped { ability, .. } = &mut invalid.abilities[1] else {
            panic!()
        };
        let Ability::Activated { body, .. } = ability.as_mut() else {
            panic!()
        };
        match mutation {
            0 => {
                body.instructions.pop();
            }
            1 => {
                body.instructions.swap(1, 2);
            }
            _ => {
                let Instruction::Effect {
                    operation: Operation::Search { choices, .. },
                    ..
                } = &mut body.instructions[0]
                else {
                    panic!()
                };
                choices[0].count = Count::UpTo(Expr::Constant(2));
            }
        }
        assert!(validate::card(&invalid).is_err());
    }
}

fn destination(zone: &str) -> Value {
    json!({"kind":"Query","args":{"objects":{"kind":"InZone","args":{
        "objects":{"kind":"One","args":{"kind":"EventDestination"}},"zone":zone}},"index":0}})
}

#[test]
fn destination_queries_keep_their_current_zone_checks() {
    let mut value = json!([{"kind":"Triggered","args":{
        "event":{"ZoneChange":{"objects":{"kind":"Any"},"from":null,"to":"Graveyard"}},
        "body":{"instructions":[{"kind":"Effect","args":{"result":3,"operation":{"kind":"Move","args":{
            "objects":{"kind":"One","args":destination("Exile")},"destination":"OwnersHand"}}}}]}}}]);
    canonical::abilities(&mut value).unwrap();
    assert_eq!(
        value[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"]["args"]
            ["kind"],
        "Query"
    );
    value[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"]["args"] =
        destination("Graveyard");
    canonical::abilities(&mut value).unwrap();
    assert_eq!(
        value[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["objects"]["args"]
            ["kind"],
        "Query"
    );
}

#[test]
fn delayed_closures_keep_explicit_damage_roles_when_the_event_subject_changes() {
    let mut value = json!([{"kind":"Triggered","args":{"event":{"Damage":{"subject":"Source"}},"body":{"instructions":[{"kind":"Effect","args":{"result":1,"operation":{"kind":"CreateDelayedTrigger","args":{"event":{"Damage":{"subject":"Recipient"}},"body":{"instructions":[{"kind":"Effect","args":{"result":2,"operation":{"kind":"Destroy","args":{"objects":{"kind":"One","args":{"kind":"Damage","args":"Source"}},"regeneration":false}}}}]}}}}}]}}}]);
    canonical::abilities(&mut value).unwrap();
    let role = &value[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["body"]["instructions"]
        [0]["args"]["operation"]["args"]["objects"]["args"];
    assert_eq!(role, &json!({"kind":"Damage","args":"Source"}));
}

#[test]
fn same_property_any_of_filters_merge_into_property_number_test_any_of() {
    let mut value = json!([{
        "kind": "Spell",
        "args": {
            "announcement": {"targets": [], "modes": null},
            "body": {
                "instructions": [{
                    "kind": "Effect",
                    "args": {
                        "result": 0,
                        "operation": {
                            "kind": "PlayCards",
                            "args": {
                                "actor": {"kind": "One", "args": {"kind": "AbilityController"}},
                                "action": "CastSpell",
                                "cards": {
                                    "kind": "Zone",
                                    "args": {
                                        "scope": {"Players": {"zone": "Hand", "players": {"kind": "One", "args": {"kind": "AbilityController"}}}},
                                        "kind": "Card",
                                        "filter": {
                                            "kind": "AnyOf",
                                            "args": [
                                                {"kind": "Property", "args": {"property": "ManaValue", "test": {"kind": "Compare", "args": {"comparison": "Equal", "value": {"kind": "Constant", "args": 2}}}}},
                                                {"kind": "Property", "args": {"property": "ManaValue", "test": {"kind": "Compare", "args": {"comparison": "Equal", "value": {"kind": "Constant", "args": 1}}}}}
                                            ]
                                        }
                                    }
                                },
                                "count": {"Exactly": {"kind": "Constant", "args": 1}},
                                "cost": "Free",
                                "into_exile": false
                            }
                        }
                    }
                }]
            }
        }
    }]);
    canonical::abilities(&mut value).unwrap();
    let filter = &value[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cards"]
        ["args"]["filter"];
    assert_eq!(filter["kind"], "Property");
    assert_eq!(filter["args"]["property"], "ManaValue");
    assert_eq!(filter["args"]["test"]["kind"], "AnyOf");
    let tests = filter["args"]["test"]["args"].as_array().unwrap();
    assert_eq!(tests.len(), 2);

    // Idempotent on second canonicalization.
    let once = value.clone();
    canonical::abilities(&mut value).unwrap();
    assert_eq!(value, once);

    // Distinct properties stay as ObjectFilter::AnyOf.
    let mut mixed = json!([{
        "kind": "Static",
        "args": {
            "kind": "Ordinary",
            "affected": {
                "Objects": {
                    "kind": "Zone",
                    "args": {
                        "scope": {"Shared": "Battlefield"},
                        "kind": "Permanent",
                        "filter": {
                            "kind": "AnyOf",
                            "args": [
                                {"kind": "Property", "args": {"property": "Power", "test": {"kind": "Compare", "args": {"comparison": "Equal", "value": {"kind": "Constant", "args": 1}}}}},
                                {"kind": "Property", "args": {"property": "Toughness", "test": {"kind": "Compare", "args": {"comparison": "Equal", "value": {"kind": "Constant", "args": 1}}}}}
                            ]
                        }
                    }
                }
            },
            "changes": [{"kind": "GrantKeyword", "args": {"kind": "Flying"}}]
        }
    }]);
    canonical::abilities(&mut mixed).unwrap();
    assert_eq!(
        mixed[0]["args"]["affected"]["Objects"]["args"]["filter"]["kind"],
        "AnyOf"
    );
}

#[test]
fn cast_payment_action_set_is_canonical_but_distinct_actions_stay_distinct() {
    fn action_filter(actions: Value) -> Value {
        json!([{"kind":"Triggered","args":{
            "event":{"Spell":{"event":"Cast","players":{"kind":"One","args":{"kind":"AbilityController"}},
                "spells":{"kind":"CastPaymentParticipant","args":{"participant":{"kind":"CardType","args":"Creature"},"actions":actions}}}},
            "intervening_if":null,"limits":{"per_turn":null,"total":null},
            "announcement":{"targets":[],"modes":null},"body":{"instructions":[]}
        }}])
    }
    let mut first = action_filter(json!(["TapForCost", "TapForManaSpent", "SacrificeForCost"]));
    let mut reordered = action_filter(json!(["SacrificeForCost", "TapForCost", "TapForManaSpent"]));
    let mut different = action_filter(json!(["TapForCost", "SacrificeForCost"]));
    canonical::abilities(&mut first).unwrap();
    canonical::abilities(&mut reordered).unwrap();
    canonical::abilities(&mut different).unwrap();
    assert_eq!(first, reordered);
    assert_ne!(first, different);
}
