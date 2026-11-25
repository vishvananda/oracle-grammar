use super::*;

fn live(scope: Value) -> Value {
    json!({"kind":"Zone","args":{"scope":scope,"kind":"Card",
        "filter":{"kind":"ExiledWith","args":{"kind":"Source"}}}})
}

#[test]
fn same_zone_live_queries_keep_their_player_filter_and_origin_operands() {
    for scope in [
        json!({"Shared":"Exile"}),
        json!({"Players":{"zone":"Library",
        "players":{"kind":"One","args":{"kind":"Target","args":{
            "slot":{"$mtgish_id":7,"$site_origins":[2]},"index":0}}}}}),
    ] {
        let zone = scope
            .get("Shared")
            .unwrap_or(&scope["Players"]["zone"])
            .clone();
        let objects = live(scope);
        let mut value = json!({"kind":"InZone","args":{"objects":objects,"zone":zone}});
        repeated_current_zone(&mut value);
        assert_eq!(value, objects);
    }
}

#[test]
fn historical_selected_observed_and_top_queries_keep_their_zone_qualification() {
    for objects in [
        json!({"kind":"Observed","args":0}),
        json!({"kind":"Selected","args":0}),
        json!({"kind":"Transition","args":{"result":0,"phase":"After"}}),
        json!({"kind":"Take","args":{"objects":live(json!({"Shared":"Exile"})),
            "count":{"kind":"Constant","args":1},"from":"Top"}}),
    ] {
        let original = json!({"kind":"InZone","args":{"objects":objects,"zone":"Exile"}});
        let mut value = original.clone();
        repeated_current_zone(&mut value);
        assert_eq!(value, original);
    }
    let mut mismatch = json!({"kind":"InZone","args":{
        "objects":live(json!({"Shared":"Graveyard"})),"zone":"Exile"}});
    let original = mismatch.clone();
    repeated_current_zone(&mut mismatch);
    assert_eq!(mismatch, original);
}

#[test]
fn reductions_do_not_erase_unknown_outer_fields_or_nonvacuous_predicates() {
    let objects = live(json!({"Shared":"Exile"}));
    let mut unknown = json!({"kind":"InZone","args":{"objects":objects,"zone":"Exile"},
        "unknown":true});
    let original = unknown.clone();
    repeated_current_zone(&mut unknown);
    assert_eq!(unknown, original);
    let mut filtered = json!({"kind":"Matching","args":{
        "objects":objects,"filter":{"kind":"CardType","args":"Creature"}}});
    let original = filtered.clone();
    sets::matching(&mut filtered, false, false);
    assert_eq!(filtered, original);
    let mut all = json!({"kind":"Matching","args":{
        "objects":objects,"filter":{"kind":"Any"}}});
    sets::matching(&mut all, false, false);
    assert_eq!(all, objects);
}

fn matched(objects: Value, filter: Value) -> Value {
    json!({"kind":"Matching","args":{"objects":objects,"filter":filter}})
}

#[test]
fn nested_matching_combines_filters_and_keeps_recorded_arrival_and_id_origins() {
    let receipt = json!({"kind":"Transition","args":{
        "result":{"$mtgish_id":7,"$site_origins":[2]},"phase":"After"}});
    let creature = json!({"kind":"CardType","args":"Creature"});
    let exile = json!({"kind":"InZone","args":{"Shared":"Exile"}});
    let nested = matched(matched(receipt.clone(), creature.clone()), exile.clone());
    let mut written = nested.clone();
    sets::matching(&mut written, true, true);
    assert_eq!(written, nested, "authored nesting remains printable");
    let mut compared = nested;
    sets::matching(&mut compared, false, true);
    assert_eq!(compared["args"]["objects"], receipt);
    let mut predicates = vec![creature, exile];
    predicates.sort_by_cached_key(|value| trace::key(value, true));
    assert_eq!(
        compared["args"]["filter"],
        json!({"kind":"AllOf","args":predicates})
    );
    let once = compared.clone();
    sets::matching(&mut compared, false, true);
    assert_eq!(compared, once);
}

#[test]
fn matching_proof_keeps_unknown_fields_and_respects_its_conjunct_bound() {
    let receipt = json!({"kind":"Transition","args":{"result":7,"phase":"Before"}});
    let filter = json!({"kind":"CardType","args":"Creature"});
    let mut unknown = matched(receipt.clone(), filter.clone());
    unknown["unknown"] = json!(true);
    let mut nested = matched(unknown, filter.clone());
    let original = nested.clone();
    sets::matching(&mut nested, false, false);
    assert_eq!(nested, original);
    let mut wide = matched(
        matched(
            receipt,
            json!({"kind":"AllOf","args":vec![filter.clone();64]}),
        ),
        filter,
    );
    let original = wide.clone();
    sets::matching(&mut wide, false, false);
    assert_eq!(wide, original);
}

#[test]
fn malformed_identity_filters_cannot_disappear_in_a_conjunction() {
    let unknown = json!({"kind":"Any","unknown":true});
    let mut filters = vec![
        unknown.clone(),
        json!({"kind":"CardType","args":"Creature"}),
    ];
    drop_identity(&mut filters);
    assert!(filters.contains(&unknown));
    let unknown =
        json!({"kind":"AllOf","args":[{"kind":"CardType","args":"Creature"}],"unknown":true});
    let mut filters = vec![unknown.clone()];
    flatten(&mut filters, "AllOf");
    assert_eq!(filters, vec![unknown]);
}
