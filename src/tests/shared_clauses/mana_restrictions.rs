//! Spending qualifications preserve every alternative, actor and source domain.
use super::*;
use crate::functions::mana_restriction;
use crate::model::canonical;

fn body(value: &mut Value) -> &mut Value {
    &mut value[0]["args"]["body"]["instructions"]
}
fn operation(value: &Value) -> &Value {
    &value[0]["args"]["operation"]["args"]
}
fn conditional() -> Value {
    compositional(
        "Whenever an Island is tapped for mana, its controller may add an additional {U}. If that Island is snow, its controller may add an additional {U}{U} instead. Spend this mana only to pay cumulative upkeep costs.",
    )
}
fn alternatives(value: &mut Value) -> &mut Value {
    &mut body(value)[0]["args"]
}
fn producer<'a>(arms: &'a mut Value, arm: &str) -> &'a mut Value {
    &mut arms[arm]["instructions"][0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]
}

#[test]
fn mana_spending_keeps_spell_filters_and_ability_source_domains_separate() {
    let mut any_source = compositional(
        "Add {C}{C}. Spend this mana only to cast an artifact spell or activate an ability of an artifact source.",
    );
    let use_ = &operation(body(&mut any_source))["restriction"];
    assert_eq!(
        *use_,
        json!({"Either":[{"CastSpell":{"kind":"CardType","args":"Artifact"}},
        {"ActivateAbility":{"sources":{"kind":"CardType","args":"Artifact"},"permanents_only":false}}]})
    );
    let mut permanent = compositional(
        "Add {C}{C}. Spend this mana only to cast artifact spells or activate abilities of artifacts.",
    );
    assert_eq!(
        operation(body(&mut permanent))["restriction"]["Either"][1]["ActivateAbility"]["permanents_only"],
        true
    );
    assert_ne!(any_source, permanent);
    printed(&any_source);
    printed(&permanent);
    let mut any = compositional("Add {G}. Spend this mana only to activate abilities.");
    assert_eq!(
        operation(body(&mut any))["restriction"],
        json!({"ActivateAbility":{"sources":{"kind":"Any"},"permanents_only":false}})
    );
}

#[test]
fn optional_mana_keeps_the_named_choice_and_recipient() {
    let mut reading = compositional(
        "Target player may add {G}{G}. Spend this mana only to pay cumulative upkeep costs.",
    );
    let optional = &body(&mut reading)[0]["args"];
    assert_eq!(
        optional["decider"],
        json!({"kind":"Target","args":{"slot":0,"index":0}})
    );
    let op = operation(&optional["body"]["instructions"]);
    assert_eq!(
        op["players"],
        json!({"kind":"One","args":optional["decider"]})
    );
    assert_eq!(op["restriction"], json!({"Purpose":"CumulativeUpkeep"}));
    printed(&reading);
    let optional = &mut body(&mut reading)[0]["args"];
    optional["decider"] = json!({"kind":"AbilityController"});
    optional["body"]["instructions"][0]["args"]["operation"]["args"]["players"] =
        json!({"kind":"One","args":{"kind":"AbilityController"}});
    reading[0]["args"]["announcement"]["targets"] = json!([]);
    let text = printed(&reading);
    assert!(!text.to_lowercase().contains("target player"), "{text}");
}

#[test]
fn optional_instead_mana_qualifies_both_exclusive_alternatives() {
    let mut reading = conditional();
    let arms = alternatives(&mut reading);
    assert_eq!(
        arms["condition"],
        json!({"kind":"ObjectMatches","args":{
        "object":{"kind":"EventSubject"},"filter":{"kind":"Supertype","args":"Snow"}}})
    );
    for (arm, amount) in [("then", 2), ("otherwise", 1)] {
        let optional = &arms[arm]["instructions"][0];
        assert_eq!(optional["kind"], "Optional");
        assert_eq!(
            optional["args"]["decider"],
            json!({"kind":"ControllerOf","args":{"kind":"EventSubject"}})
        );
        let op = producer(arms, arm);
        assert_eq!(
            op["players"],
            json!({"kind":"One","args":{"kind":"ControllerOf","args":{"kind":"EventSubject"}}})
        );
        assert_eq!(
            op["mana"],
            json!({"Amount":{"color":"Blue","amount":{"kind":"Constant","args":amount}}})
        );
        assert_eq!(op["restriction"], json!({"Purpose":"CumulativeUpkeep"}));
    }
    printed(&reading);
    let text = "Whenever an Island is tapped for mana, its controller may add an additional {U}. If that Island is snow, its controller may add an additional {U}{U} instead. Spend this mana only to pay cumulative upkeep costs.";
    let trail = engine()
        .trail_of("Context example", &reading, text)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine()
            .print_trail("Context example", &reading, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
}

#[test]
fn edited_mana_alternatives_print_current_filters_amounts_and_restrictions() {
    let mut reading = conditional();
    let arms = alternatives(&mut reading);
    arms["condition"]["args"]["filter"] = json!({"kind":"Supertype","args":"Legendary"});
    for (arm, color, n) in [("then", "Green", 3), ("otherwise", "Red", 1)] {
        let op = producer(arms, arm);
        op["mana"] = json!({"Amount":{"color":color,"amount":{"kind":"Constant","args":n}}});
        op["restriction"] = json!({"CastSpell":{"kind":"CardType","args":"Artifact"}});
    }
    let text = printed(&reading);
    assert!(
        text.contains("legendary") && text.contains("{G}{G}{G}") && text.contains("{R}"),
        "{text}"
    );
    assert!(
        text.contains("artifact") && !text.contains("cumulative upkeep") && !text.contains("snow"),
        "{text}"
    );
}

#[test]
fn a_common_spending_sentence_cannot_merge_different_branch_restrictions() {
    use crate::model::ir_schema::Ty;
    use crate::pattern::Pattern;
    let mut reading = conditional();
    producer(alternatives(&mut reading), "then")["restriction"] =
        json!({"CastSpell":{"kind":"CardType","args":"Artifact"}});
    let patterns = [
        Pattern::Hole(
            "body".into(),
            Ty::Vec(Box::new(Ty::Named("Instruction".into()))),
        ),
        Pattern::Hole("use".into(), Ty::Named("ManaUse".into())),
    ];
    assert!(mana_restriction::unify(&patterns, body(&mut reading), Default::default()).is_empty());
    validate_abilities(&reading).unwrap();
    printed(&reading);
}

#[test]
fn mana_qualification_refuses_other_actions_unknown_fields_and_unbounded_data() {
    let mut mana = compositional("Add {R}.");
    let base = body(&mut mana).clone();
    let use_ = json!({"CastSpell":{"kind":"CardType","args":"Creature"}});
    let mut draw = compositional("Draw a card.");
    assert!(mana_restriction::attach(body(&mut draw), &use_).is_err());
    let mut extra = base.clone();
    extra[0]["extra"] = json!(true);
    assert!(mana_restriction::attach(&extra, &use_).is_err());
    let mut sequential = base.clone();
    sequential.as_array_mut().unwrap().push(base[0].clone());
    assert!(mana_restriction::attach(&sequential, &use_).is_err());
    let qualified = mana_restriction::attach(&base, &use_).unwrap();
    assert!(mana_restriction::attach(&qualified, &use_).is_err());
    let mut deep = Value::Null;
    for _ in 0..140 {
        deep = json!([deep]);
    }
    assert!(mana_restriction::attach(&deep, &use_).is_err());
    assert!(mana_restriction::attach(&base, &json!("x".repeat(1024 * 1024 + 1))).is_err());
}

#[test]
fn mana_spending_words_keep_negation_and_distinct_kinds_of_casting_filters() {
    for text in [
        "You may add {C}{C}. This mana can't be spent to cast spells.",
        "Add {U}. Spend this mana only to cast an instant or sorcery spell.",
        "Add {R}. Spend this mana only to cast kicked spells.",
        "Add {G}. Spend this mana only to cast a multicolored spell.",
        "Add {U}. Spend this mana only to cast an instant or sorcery spell or a kicked spell.",
        "Add {R}{R}. Spend this mana only to cast Dwarf, Equipment, and Saga spells.",
        "Add {C}{C}. Spend this mana only to cast colorless Eldrazi spells or activate abilities of colorless Eldrazi.",
    ] {
        let frame = compositional(text);
        printed(&frame);
    }
    assert!(matches!(
        engine().parse("Context example", "Add {G}. Spend this mana only to cast."),
        Err(Refusal::NoParse { .. })
    ));
}

#[test]
fn alternative_casting_uses_one_predicate_and_prints_shared_spell_phrases() {
    let mut reading = compositional(
        "Add {G}. Spend this mana only to cast a creature spell of the chosen type or a legendary creature spell.",
    );
    let current = operation(body(&mut reading))["restriction"].clone();
    let filters = current["CastSpell"]["args"].as_array().unwrap();
    let mut legacy = reading.clone();
    body(&mut legacy)[0]["args"]["operation"]["args"]["restriction"] =
        json!({"Either":filters.iter().map(|f| json!({"CastSpell":f})).collect::<Vec<_>>()});
    validate_abilities(&legacy).unwrap();
    assert_eq!(canonical_abilities(&legacy).unwrap(), reading);
    assert_eq!(canonical_abilities(&reading).unwrap(), reading);
    printed(&reading);
    reading = compositional(
        "Add {U}. Spend this mana only to cast an instant or sorcery spell or a kicked spell.",
    );
    let text =
        "Add {U}. Spend this mana only to cast an instant or sorcery spell or a kicked spell.";
    let trail = engine()
        .trail_of("Context example", &reading, text)
        .unwrap()
        .unwrap();
    assert_eq!(
        engine()
            .print_trail("Context example", &reading, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
}

#[test]
fn alternative_casting_preserves_target_filter_origins_and_invalid_fields() {
    use crate::model::ir;
    let mut reading = compositional("Destroy target creature.");
    body(&mut reading)[0]["args"]["operation"] = json!({"kind":"AddMana","args":{
        "players":{"kind":"One","args":{"kind":"AbilityController"}},
        "mana":{"Amount":{"color":"Blue","amount":{"kind":"Constant","args":1}}},
        "restriction":{"Either":[{"CastSpell":{"kind":"SameObject","args":{"kind":"Target","args":{"slot":0,"index":0}}}},
            {"CastSpell":{"kind":"CardType","args":"Creature"}}]}}});
    validate_abilities(&reading).unwrap();
    let card: ir::UnifiedCard = serde_json::from_value(json!({
        "schema_version":ir::SCHEMA_VERSION,"layout":"Card","data":[],"faces":[{
            "schema_version":ir::SCHEMA_VERSION,"name":"Context example",
            "characteristics":{"supertypes":[],"card_types":["Sorcery"],"subtypes":[],
                "mana_cost":null,"color_indicator":[],"power_toughness":null,"loyalty":null,"defense":null},
            "abilities":reading}]})).unwrap();
    let mapped = canonical::mapped_unified(&card).unwrap();
    let prefix = "/faces/0/abilities/0/args/body/instructions/0/args/operation/args/restriction";
    let old = format!("{prefix}/Either/0/CastSpell/args/args/slot");
    let sites = &mapped.map.sites[&old];
    assert_eq!(sites.len(), 1);
    assert!(sites[0].starts_with(&format!("{prefix}/CastSpell/args/")));
    assert_eq!(mapped.card.faces[0].abilities.len(), 1);
    assert!(mapped.map.introduced.is_empty());
    let mut invalid = reading;
    invalid[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["restriction"]["Either"]
        [0]["extra"] = json!(true);
    assert!(validate_abilities(&invalid).is_err());
    assert!(validate_abilities(&canonical_abilities(&invalid).unwrap()).is_err());
}
