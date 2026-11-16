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
    sets::matching(&mut filtered);
    assert_eq!(filtered, original);
    let mut all = json!({"kind":"Matching","args":{
        "objects":objects,"filter":{"kind":"Any"}}});
    sets::matching(&mut all);
    assert_eq!(all, objects);
}
