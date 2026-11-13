//! Fixed simultaneous mana has one shape; choices and observed receipts keep scope.
use super::*;
use crate::model::canonical;

const ITEMS: &str = "/0/args/body/instructions";
fn items(value: &mut Value) -> &mut Vec<Value> {
    value.pointer_mut(ITEMS).unwrap().as_array_mut().unwrap()
}
fn direct() -> Value {
    compositional("Each player adds {B}{R}{G}.")
}
fn looped(mut value: Value, binding: u32, result: u32) -> Value {
    let mut child = items(&mut value)[0].clone();
    child["args"]["result"] = json!(result);
    child["args"]["operation"]["args"]["players"] =
        json!({"kind":"One","args":{"kind":"Bound","args":binding}});
    items(&mut value)[0] = json!({"kind":"ForEachPlayer","args":{
        "players":{"kind":"All"},"binding":binding,"starting_with":null,
        "in_chosen_direction":false,"body":{"instructions":[child]}}});
    value
}
fn mana(value: &mut Value) -> &mut Value {
    &mut items(value)[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["mana"]
}
fn loops(value: &Value) -> usize {
    let mut found = Vec::new();
    visit(value, "ForEachPlayer", &mut found);
    found.len()
}
fn observe(value: &mut Value) {
    items(value).push(json!({"kind":"Effect","args":{"result":43,"operation":{
        "kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},
            "amount":{"kind":"PerformedTotal","args":42}}}}}));
}

#[test]
fn unobserved_fixed_player_mana_normalizes_and_prints_current_operands() {
    let reading = direct();
    assert_eq!(loops(&reading), 0);
    let mut legacy = looped(reading.clone(), 17, 42);
    validate_abilities(&legacy).unwrap();
    let normalized = canonical_abilities(&legacy).unwrap();
    assert_eq!(normalized, reading);
    assert_eq!(canonical_abilities(&normalized).unwrap(), normalized);
    let mut reduced = legacy.clone();
    canonical::reduced(&mut reduced).unwrap();
    assert_eq!(loops(&reduced), 0);
    assert_eq!(items(&mut reduced)[0]["args"]["result"], 42);
    let production = mana(&mut legacy);
    production["All"][0]["Amount"]["color"] = json!("Blue");
    production["All"][0]["Amount"]["amount"]["args"] = json!(2);
    let text = printed_types(&legacy, &["Sorcery"]);
    assert!(
        text.matches("{U}").count() == 2 && text.contains("{R}") && text.contains("{G}"),
        "{text}"
    );
    assert!(!text.contains("{B}"), "{text}");
}

#[test]
fn fixed_mana_does_not_move_choices_dynamic_values_or_restrictions() {
    let base = looped(direct(), 17, 42);
    for replacement in [
        json!({"ChooseOne":[{"Amount":{"color":"Blue","amount":{"kind":"Constant","args":1}}},
            {"Amount":{"color":"Red","amount":{"kind":"Constant","args":1}}}]}),
        json!({"Amount":{"color":"Blue","amount":{"kind":"PlayerNumber","args":{
            "player":{"kind":"Bound","args":17},"number":"CardsInHand"}}}}),
        json!({"Amount":{"color":"Chosen","amount":{"kind":"Constant","args":1}}}),
    ] {
        let mut reading = base.clone();
        *mana(&mut reading) = replacement;
        validate_abilities(&reading).unwrap();
        let normalized = canonical_abilities(&reading).unwrap();
        assert_eq!(loops(&normalized), 1);
        validate_abilities(&normalized).unwrap();
    }
    let mut restricted = base.clone();
    items(&mut restricted)[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["restriction"] =
        json!({"CastSpell":{"kind":"Any"}});
    validate_abilities(&restricted).unwrap();
    assert_eq!(loops(&canonical_abilities(&restricted).unwrap()), 1);
    let mut ordered = base.clone();
    items(&mut ordered)[0]["args"]["starting_with"] = json!({"kind":"Active"});
    assert_eq!(loops(&canonical_abilities(&ordered).unwrap()), 1);
    items(&mut ordered)[0]["args"]["starting_with"] = Value::Null;
    items(&mut ordered)[0]["args"]["in_chosen_direction"] = json!(true);
    assert_eq!(loops(&canonical_abilities(&ordered).unwrap()), 1);
    let mut other_player = base;
    items(&mut other_player)[0]["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["players"] =
        json!({"kind":"One","args":{"kind":"AbilityController"}});
    assert_eq!(loops(&canonical_abilities(&other_player).unwrap()), 1);
}

#[test]
fn fixed_mana_preserves_results_observed_afterward_or_by_a_future_program() {
    let base = looped(direct(), 17, 42);
    let mut observed = base.clone();
    observe(&mut observed);
    validate_abilities(&observed).unwrap();
    assert_eq!(loops(&canonical_abilities(&observed).unwrap()), 1);
    let mut delayed = base;
    observe(&mut delayed);
    let draw = items(&mut delayed).pop().unwrap();
    items(&mut delayed).push(json!({"kind":"Effect","args":{"result":44,"operation":{
        "kind":"CreateDelayedTrigger","args":{"event":{"Turn":{"point":"EndStepBegins","players":{"kind":"All"}}},
            "start":"Immediately","expires":null,"intervening_if":null,"body":{"instructions":[draw]}}}}}));
    validate_abilities(&delayed).unwrap();
    assert_eq!(loops(&canonical_abilities(&delayed).unwrap()), 1);
}

#[test]
fn fixed_mana_keeps_duplicate_bindings_and_invalid_extra_operands_invalid() {
    let base = looped(direct(), 17, 42);
    for suffix in [
        "",
        "/args",
        "/args/body",
        "/args/body/instructions/0",
        "/args/body/instructions/0/args/operation/args/mana/All/0/Amount",
    ] {
        let mut invalid = base.clone();
        invalid.pointer_mut(&format!("{ITEMS}/0{suffix}")).unwrap()["extra"] = json!(true);
        assert!(validate_abilities(&invalid).is_err());
        let normalized = canonical_abilities(&invalid).unwrap();
        assert_eq!(loops(&normalized), 1);
        assert!(validate_abilities(&normalized).is_err());
    }
    let mut negative = base.clone();
    mana(&mut negative)["All"][0]["Amount"]["amount"]["args"] = json!(-1);
    assert!(validate_abilities(&negative).is_err());
    let normalized = canonical_abilities(&negative).unwrap();
    assert_eq!(loops(&normalized), 1);
    assert!(validate_abilities(&normalized).is_err());
    let mut forged = base.clone();
    items(&mut forged)[0]["args"]["body"]["instructions"][0]["args"]["result"] =
        json!({"$mtgish_id":42,"$site_origins":[0]});
    assert!(validate_abilities(&forged).is_err());
    let normalized = canonical_abilities(&forged).unwrap();
    assert_eq!(loops(&normalized), 1);
    assert!(validate_abilities(&normalized).is_err());
    let mut duplicate = base;
    let inner = items(&mut duplicate)[0].clone();
    items(&mut duplicate)[0]["args"]["body"]["instructions"] = json!([inner]);
    assert!(validate_abilities(&duplicate).is_err());
    let normalized = canonical_abilities(&duplicate).unwrap();
    assert_eq!(loops(&normalized), 2);
    assert!(validate_abilities(&normalized).is_err());
}

#[test]
fn fixed_mana_normalizes_sibling_binding_labels_independently() {
    let mut reading = looped(direct(), 17, 42);
    let mut other = items(&mut reading)[0].clone();
    other["args"]["body"]["instructions"][0]["args"]["result"] = json!(43);
    items(&mut reading).push(other);
    validate_abilities(&reading).unwrap();
    let normalized = canonical_abilities(&reading).unwrap();
    assert_eq!(loops(&normalized), 0);
    validate_abilities(&normalized).unwrap();
}

#[test]
fn fixed_mana_moves_the_original_result_origin_and_deletes_only_binding_sites() {
    use crate::model::ir;
    let legacy = looped(direct(), 17, 42);
    let card: ir::UnifiedCard = serde_json::from_value(json!({
        "schema_version":ir::SCHEMA_VERSION,"layout":"Card","data":[],"faces":[{
            "schema_version":ir::SCHEMA_VERSION,"name":"Context example",
            "characteristics":{"supertypes":[],"card_types":["Sorcery"],"subtypes":[],
                "mana_cost":null,"color_indicator":[],"power_toughness":null,"loyalty":null,
                "defense":null},"abilities":legacy}]}))
    .unwrap();
    let mapped = canonical::mapped_unified(&card).unwrap();
    let prefix = "/faces/0/abilities/0/args/body/instructions/0";
    assert_eq!(
        mapped.map.sites[&format!("{prefix}/args/body/instructions/0/args/result")],
        [format!("{prefix}/args/result")]
    );
    for suffix in [
        "/args/binding",
        "/args/body/instructions/0/args/operation/args/players/args/args",
    ] {
        assert!(mapped.map.sites[&format!("{prefix}{suffix}")].is_empty());
    }
    assert!(mapped.map.introduced.is_empty());
    assert_eq!(
        serde_json::to_value(mapped.card).unwrap(),
        serde_json::to_value(canonical::unified(&card).unwrap()).unwrap()
    );
}

#[test]
fn fixed_mana_preserves_duplicate_results_and_unbound_player_errors() {
    let mut duplicate = looped(direct(), 17, 42);
    let mut draw = compositional("Draw a card.");
    let mut draw = items(&mut draw).remove(0);
    draw["args"]["result"] = json!(42);
    items(&mut duplicate).push(draw.clone());
    assert!(validate_abilities(&duplicate).is_err());
    let normalized = canonical_abilities(&duplicate).unwrap();
    assert_eq!(loops(&normalized), 1);
    assert!(validate_abilities(&normalized).is_err());
    let mut unbound = looped(direct(), 17, 42);
    draw["args"]["result"] = json!(43);
    draw["args"]["operation"]["args"]["amount"] = json!({"kind":"PlayerNumber","args":{
        "player":{"kind":"Bound","args":17},"number":"CardsInHand"}});
    items(&mut unbound).push(draw);
    assert!(validate_abilities(&unbound).is_err());
    assert!(validate_abilities(&canonical_abilities(&unbound).unwrap()).is_err());
}

#[test]
fn fixed_mana_keeps_unproved_deep_productions_and_enclosing_abilities() {
    let mut deep = looped(direct(), 17, 42);
    let mut production = json!({"Amount":{"color":"Blue","amount":{"kind":"Constant","args":1}}});
    for _ in 0..40 {
        production = json!({"All":[production]});
    }
    *mana(&mut deep) = production;
    let normalized = canonical_abilities(&deep).unwrap();
    assert_eq!(loops(&normalized), 0);
    let mut enclosing = looped(direct(), 17, 42);
    // A descendant Ability introduces a separate ID scope, even when its body
    // does not happen to use the enclosing receipt in this particular value.
    let grant = json!([{"kind":"GrantAbility","args":{"kind":"Keyword","args":{"kind":"Flying"}}}]);
    items(&mut enclosing).push(json!({"kind":"Effect","args":{"result":43,"operation":{
        "kind":"CreateContinuousEffect","args":{"source":{"kind":"Source"},
            "affected":{"Objects":{"kind":"One","args":{"kind":"Source"}}},
            "changes":grant,"duration":"EndOfTurn","evaluation":"Snapshot"}}}}));
    validate_abilities(&enclosing).unwrap();
    assert_eq!(loops(&canonical_abilities(&enclosing).unwrap()), 1);
}

#[test]
fn literal_mana_totals_print_every_current_color_order_and_multiplicity() {
    for text in [
        "Add {U}{U}.",
        "Add {G}{B}{G}{U}.",
        "Add {C}{G}{R}{B}{U}{W}.",
    ] {
        let reading = compositional(text);
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
        printed(&reading);
        assert_eq!(canonical_abilities(&reading).unwrap(), reading);
    }
    let mut reading = compositional("Add {U}{U}.");
    let production = &mut items(&mut reading)[0]["args"]["operation"]["args"]["mana"];
    assert_eq!(
        *production,
        json!({"Amount":{"color":"Blue","amount":{"kind":"Constant","args":2}}})
    );
    production["Amount"]["amount"]["args"] = json!(3);
    let text = printed(&reading);
    assert!(text.contains("{U}{U}{U}"), "{text}");
}

#[test]
fn literal_mana_does_not_guess_choices_zeroes_or_overflow() {
    use crate::model::ir::ManaProduction;
    for mana in [
        json!({"All":[{"Amount":{"color":"Blue","amount":{"kind":"Constant","args":0}}}]}),
        json!({"All":[{"Amount":{"color":"Chosen","amount":{"kind":"Constant","args":2}}}]}),
        json!({"All":[{"Amount":{"color":"Blue","amount":{"kind":"Constant","args":i64::MAX}}},
            {"Amount":{"color":"Blue","amount":{"kind":"Constant","args":1}}}]}),
    ] {
        let typed: ManaProduction = serde_json::from_value(mana).unwrap();
        assert!(canonical::literal_mana(&typed).is_none());
    }
}

#[test]
fn literal_mana_style_groups_units_without_changing_cost_order_or_keyed_orders() {
    for (text, violation) in [
        ("Add {G}{U}{U}{R}.", false),
        ("Add {G}{U}{R}{U}.", true),
        ("Add {C}{U}.", false),
        ("Add {U}{C}.", true),
        ("{W}{W}{U}{U}{B}{B}{R}{R}{G}{G}", false),
        ("{W}{W}{R}{R}{U}{U}{B}{B}{G}{G}", true),
        ("{R}{U}, {T}: Add {G}.", false),
    ] {
        let tokens = engine().tokens_of("Context example", text).unwrap();
        assert_eq!(
            crate::style::breaches(&tokens, &[])
                .iter()
                .any(|(name, _, _)| *name == "literal mana order"),
            violation,
            "{text}"
        );
    }
}

#[test]
fn closed_literal_mana_declarations_share_the_current_ir_normal_form() {
    use crate::model::ir_schema::{Schema, Ty};
    use crate::pattern::{elaborate, parse};
    let (_, syntax) = parse("ManaProduction::All([Amount{color:Green, amount:Constant(1)}, Amount{color:Green, amount:Constant(1)}])").unwrap();
    let pattern = elaborate(
        &syntax,
        &Ty::Named("ManaProduction".into()),
        &Schema::of_ir().unwrap(),
    )
    .unwrap();
    let mut frame = compositional("Add {G}{G}.");
    let current = &items(&mut frame)[0]["args"]["operation"]["args"]["mana"];
    assert_eq!(
        *current,
        json!({"Amount":{"color":"Green","amount":{"kind":"Constant","args":2}}})
    );
    assert!(!pattern.unify(current, Default::default()).is_empty());
}

#[test]
fn literal_mana_defaults_keep_long_grouped_color_bundles() {
    let text = "Add {W}{W}{U}{U}{B}{B}{R}{R}{G}{G}.";
    let frame = compositional(text);
    assert_eq!(printed(&frame), text);
}
