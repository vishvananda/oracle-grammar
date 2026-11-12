use super::*;
use crate::engine::{Engine, canonical_abilities};
fn engine() -> &'static Engine {
    crate::test_support::engine()
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/attached-counter-other.json"
    ))
    .unwrap()
}
fn owned() -> Value {
    serde_json::from_str(include_str!("../../families/attached-counter-other.json")).unwrap()
}
fn count(reference: Value) -> Value {
    json!({"kind":"CountCounters","args":{"recipients":{"kind":"Objects","args":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"AllOf","args":[creature(),controlled(),{"kind":"Not","args":{"kind":"SameObject","args":reference}}]}}}},"kind":{"PowerToughness":[1,1]},"information":"CurrentOrLastKnown"}})
}
fn raw() -> Value {
    let r = json!({"$sym":"other_reference","args":[{"kind":"AllOf","args":[creature(),controlled()]}],"at":8});
    json!({"kind":"Static","args":{"kind":"Ordinary","affected":{"Objects":{"kind":"One","args":literal(&host(),1).unwrap()}},"changes":[{"kind":"Pump","args":{"power":count(r.clone()),"toughness":count(r)}}]}})
}
fn power(root: &mut Value) -> &mut Value {
    &mut root["args"]["changes"][0]["args"]["power"]
}
fn exclusion(root: &mut Value) -> &mut Value {
    &mut power(root)["args"]["recipients"]["args"]["args"]["filter"]["args"][2]["args"]["args"]
}
fn unchanged(mut root: Value) {
    let before = root.clone();
    prepare(&mut root).unwrap();
    assert_eq!(root, before);
}
fn prints(name: &str, text: &str, expected: &Value) {
    let trail = engine().trail_of(name, expected, text).unwrap().unwrap();
    assert_eq!(
        engine()
            .print_trail(name, expected, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
    let printed = engine()
        .print_keyed(name, expected, &[], &[])
        .unwrap()
        .unwrap();
    assert!(
        engine()
            .parse(name, &printed)
            .unwrap()
            .readings
            .contains(expected)
    );
}
#[test]
fn actual_whole_gleam_native_and_original_print_before_cli_relink() {
    let f = fixture();
    let name = f["name"].as_str().unwrap();
    let text = f["text"].as_str().unwrap();
    let wanted = canonical_abilities(&f["native"]).unwrap();
    let bad = canonical_abilities(&f["wrong_old"]).unwrap();
    let actual = engine().parse(name, text).unwrap();
    assert_eq!(actual.readings, vec![wanted.clone()]);
    assert!(!actual.readings.contains(&bad));
    prints(name, text, &wanted);
}
#[test]
fn actual_edited_counter_kind_amount_and_both_holder_compositions_without_mined() {
    let off = engine().rule_set(|n| n.starts_with("m_") || n.starts_with("t_"));
    for c in owned()["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let text = c["text"].as_str().unwrap();
        let wanted = canonical_abilities(&c["expected"]).unwrap();
        assert_eq!(
            engine().parse(name, text).unwrap().readings,
            vec![wanted.clone()]
        );
        assert_eq!(
            engine().parse_without(name, text, &off).unwrap().readings,
            vec![wanted.clone()]
        );
        prints(name, text, &wanted);
    }
}
#[test]
fn actual_matching_object_counts_and_mask_vampirism_keep_exact_sets() {
    let f = fixture();
    for c in f["proper_controls"].as_array().unwrap() {
        let wanted = canonical_abilities(&c["expected"]).unwrap();
        assert!(
            engine()
                .parse(c["name"].as_str().unwrap(), c["text"].as_str().unwrap())
                .unwrap()
                .readings
                .contains(&wanted)
        );
    }
    for c in f["unchanged_boundaries"].as_array().unwrap() {
        let wanted: Vec<_> = c["readings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| canonical_abilities(v).unwrap())
            .collect();
        let actual = engine()
            .parse(c["name"].as_str().unwrap(), c["text"].as_str().unwrap())
            .unwrap();
        assert_eq!(actual.readings.len(), wanted.len());
        assert!(actual.readings.iter().all(|v| wanted.contains(v)));
    }
}
#[test]
fn edited_current_value_cannot_replay_original_counter_amount() {
    let f = owned();
    let c = &f["cases"][0];
    let edited = &f["cases"][1];
    let name = c["name"].as_str().unwrap();
    let text = c["text"].as_str().unwrap();
    let old = canonical_abilities(&c["expected"]).unwrap();
    let new = canonical_abilities(&edited["expected"]).unwrap();
    let trail = engine().trail_of(name, &old, text).unwrap().unwrap();
    assert!(!engine().parse(name, text).unwrap().readings.contains(&new));
    if let Some(output) = engine().print_trail(name, &new, &trail).unwrap() {
        assert_ne!(output, text);
        assert!(
            engine()
                .parse(name, &output)
                .unwrap()
                .readings
                .contains(&new)
        );
    }
    prints(name, edited["text"].as_str().unwrap(), &new);
}
#[test]
fn literal_annotation_keeps_one_original_reference_and_refuses_foreign_identities() {
    let annotated = literal(&host(), 4).unwrap();
    assert_eq!(symbol_name(&annotated), Some("enchanted_operand"));
    assert_eq!(annotated["args"], json!([host()]));
    assert_eq!(annotated["attached_noun"], "Creature");
    for r in [
        json!({"kind":"Source"}),
        json!({"kind":"AttachedTo","args":{"kind":"GrantingObject"}}),
        json!({"kind":"Target","args":{"slot":0,"index":0}}),
    ] {
        assert!(literal(&r, 0).is_err());
    }
    for r in [
        host(),
        json!({"$sym":"enchanted_operand","args":[host()],"at":1}),
        json!({"$sym":"remember_operand","args":[host()],"at":1}),
    ] {
        let mut x = raw();
        x["args"]["affected"]["Objects"]["args"] = r;
        unchanged(x);
    }
}
#[test]
fn lexical_counter_owner_changes_only_exact_marked_exclusions() {
    let mut x = raw();
    prepare(&mut x).unwrap();
    assert_eq!(*exclusion(&mut x), host());
    let mut x = raw();
    *exclusion(&mut x) = json!({"$sym":"pump_other_ref","args":[{"$sym":"move_after_other","args":[{"kind":"Source"}],"at":8}],"at":8});
    prepare(&mut x).unwrap();
    assert_eq!(*exclusion(&mut x), host());
    for r in [
        json!({"kind":"Source"}),
        json!({"kind":"GrantingObject"}),
        json!({"kind":"Target","args":{"slot":0,"index":0}}),
        json!({"kind":"EventSubject"}),
        json!({"kind":"Query","args":{"objects":{"kind":"Targets","args":0},"index":0}}),
    ] {
        let mut x = raw();
        *exclusion(&mut x) = r;
        let old = power(&mut x).clone();
        prepare(&mut x).unwrap();
        assert_eq!(power(&mut x), &old);
    }
}
#[test]
fn objects_queries_other_verbs_and_nested_holders_cannot_acquire_counter_owner() {
    let x = raw();
    for wrapped in [
        json!({"kind":"GrantAbility","args":x.clone()}),
        json!({"$sym":"holder","args":[x.clone()],"at":0}),
        json!({"kind":"If","args":{"then":{"instructions":[x.clone()]}}}),
    ] {
        unchanged(wrapped);
    }
    let mut x = raw();
    for field in ["power", "toughness"] {
        x["args"]["changes"][0]["args"][field] = json!({"kind":"CountObjects","args":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"AllOf","args":[creature(),controlled(),{"kind":"Not","args":{"kind":"SameObject","args":{"$sym":"other_reference","args":[creature()],"at":8}}}]}}}});
    }
    unchanged(x);
    let mut x = raw();
    x["args"]["changes"][0]["kind"] = json!("Rule");
    unchanged(x);
}
#[test]
fn negative_or_foreign_populations_and_noncreature_nouns_do_not_bind() {
    for filter in [
        json!({"kind":"AnyOf","args":[creature(),controlled()]}),
        json!({"kind":"AllOf","args":[{"kind":"Not","args":creature()},controlled()]}),
        json!({"kind":"CardType","args":"Enchantment"}),
    ] {
        let mut x = raw();
        for field in ["power", "toughness"] {
            x["args"]["changes"][0]["args"][field]["args"]["recipients"]["args"]["args"]["filter"] =
                filter.clone();
        }
        unchanged(x);
    }
    let mut x = raw();
    x["args"]["affected"]["Objects"]["args"]["attached_noun"] = json!("Artifact");
    unchanged(x);
}
#[test]
fn inverse_view_requires_same_current_host_and_keeps_original_ordinary_forms() {
    let mut x = raw();
    prepare(&mut x).unwrap();
    x["args"]["affected"]["Objects"]["args"] = host();
    let original = x.clone();
    let view = surface(&x).unwrap().unwrap();
    assert_eq!(x, original);
    assert!(view.to_string().contains("attached_counter_other_view"));
    let mut foreign = x.clone();
    foreign["args"]["affected"]["Objects"]["args"] =
        json!({"kind":"AttachedTo","args":{"kind":"GrantingObject"}});
    assert!(surface(&foreign).unwrap().is_none());
    let mut wrong = x;
    *exclusion(&mut wrong) = json!({"kind":"Source"});
    wrong["args"]["changes"][0]["args"]["toughness"] = json!({"kind":"Constant","args":0});
    assert!(surface(&wrong).unwrap().is_none());
}
#[test]
fn malformed_nominal_annotation_and_bounded_proofs_fail_closed() {
    let mut x = raw();
    x["args"]["affected"]["Objects"]["args"]["args"]
        .as_array_mut()
        .unwrap()
        .push(host());
    assert!(prepare(&mut x).is_err());
    let mut x = raw();
    x["args"]["affected"]["Objects"]["args"]["args"][0] = json!({"kind":"Source"});
    assert!(prepare(&mut x).is_err());
    let mut x = raw();
    let mut deep = power(&mut x).clone();
    for _ in 0..130 {
        deep = json!({"kind":"Multiply","args":[{"kind":"Constant","args":1},deep]});
    }
    *power(&mut x) = deep;
    assert!(prepare(&mut x).is_err());
}
