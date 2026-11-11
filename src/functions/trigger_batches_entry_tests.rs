use super::*;
use crate::engine::{Engine, canonical_abilities};
use std::sync::OnceLock;
fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| Engine::load().unwrap())
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/capped-entry-native.json"
    ))
    .unwrap()
}
fn owned() -> Value {
    serde_json::from_str(include_str!("../../families/capped-entry-native.json")).unwrap()
}
fn prints(name: &str, source: &str, value: &Value) {
    let trail = engine().trail_of(name, value, source).unwrap().unwrap();
    assert_eq!(
        engine()
            .print_trail(name, value, &trail)
            .unwrap()
            .as_deref(),
        Some(source)
    );
    let text = engine()
        .print_keyed(name, value, &["Creature".into()], &[])
        .unwrap()
        .unwrap();
    assert!(
        engine()
            .parse(name, &text)
            .unwrap()
            .readings
            .contains(value)
    );
}
#[test]
fn complete_archivist_and_smith_keep_all_old_and_add_only_declared_native_views() {
    for case in fixture()["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let permitted: Vec<_> = case["permitted"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| canonical_abilities(v).unwrap())
            .collect();
        let native = canonical_abilities(&case["native_source"]["abilities"]).unwrap();
        let actual = engine().parse(name, text).unwrap();
        assert_eq!(actual.readings.len(), permitted.len(), "{name}");
        assert!(
            permitted.iter().all(|v| actual.readings.contains(v)),
            "{name}"
        );
        assert!(actual.readings.contains(&native));
        for old in case["preserved"].as_array().unwrap() {
            assert!(actual.readings.contains(&canonical_abilities(old).unwrap()));
        }
        prints(name, text, &native);
    }
}
#[test]
fn own_entry_caps_edits_prefixes_and_holder_roles_compose_without_mined_rules() {
    let family = owned();
    let f = fixture();
    let off = engine().rule_set(|name| name.starts_with("m_") || name.starts_with("t_"));
    for (index, case) in family["cases"].as_array().unwrap().iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let permitted: Vec<_> = f["owned_permitted"][index]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| canonical_abilities(v).unwrap())
            .collect();
        for actual in [
            engine().parse(name, text),
            engine().parse_without(name, text, &off),
        ] {
            let readings = actual.unwrap().readings;
            if index == 6 {
                assert!(readings.contains(&permitted[0]));
                assert!(
                    readings
                        .iter()
                        .all(|v| v[0]["args"]["event"].get("Batch").is_some())
                );
            } else {
                assert_eq!(readings.len(), permitted.len(), "{text}");
                assert!(readings.iter().all(|v| permitted.contains(v)), "{text}");
            }
        }
        if case["default"] == true {
            for value in permitted {
                prints(name, text, &value);
            }
        }
    }
}
#[test]
fn edited_type_and_counter_amount_use_current_operands() {
    let family = owned();
    let old_case = &family["cases"][0];
    let edited_case = &family["cases"][1];
    let old = canonical_abilities(&old_case["expected"]).unwrap();
    let edited = canonical_abilities(&edited_case["expected"]).unwrap();
    assert_ne!(old, edited);
    let trail = engine()
        .trail_of("Example", &old, old_case["text"].as_str().unwrap())
        .unwrap()
        .unwrap();
    if let Some(text) = engine().print_trail("Example", &edited, &trail).unwrap() {
        assert!(
            engine()
                .parse("Example", &text)
                .unwrap()
                .readings
                .contains(&edited)
        );
    }
    prints("Example", edited_case["text"].as_str().unwrap(), &edited);
}
#[test]
fn bound_event_reads_cannot_hide_in_body_announcement_or_intervening_if() {
    let f = fixture();
    let mut value = f["owned_permitted"][0][1][0].clone();
    for field in ["body", "announcement", "intervening_if"] {
        let mut marked = mark(&value).unwrap();
        marked["args"][field] = json!({"kind":"ObjectProperty","args":{
            "object":{"kind":"EventSubject"},"property":"Power","information":"CurrentOrLastKnown"}});
        assert!(
            finish(&mut marked)
                .unwrap_err()
                .contains("observes event data")
        );
    }
    let before = value.clone();
    let mut marked = mark(&value).unwrap();
    finish(&mut marked).unwrap();
    assert_eq!(marked, before);
    value["args"]["limits"]["per_turn"] = json!(2);
    assert!(mark(&value).is_err());
    value["args"]["limits"]["per_turn"] = json!(1);
    value["args"]["event"] = json!({"Player":{"event":"CreatesToken","players":{"kind":"All"},"objects":{"kind":"Any"}}});
    assert!(mark(&value).is_err());
}
#[test]
fn existing_death_eligibility_and_payload_guard_remain_identical() {
    let mut value = fixture()["owned_permitted"][0][1][0].clone();
    value["args"]["event"] = json!({"ZoneChange":{"objects":{"kind":"CardType","args":"Creature"},"from":"Battlefield","to":"Graveyard"}});
    let before = value.clone();
    let mut marked = mark(&value).unwrap();
    finish(&mut marked).unwrap();
    assert_eq!(marked, before);
    value["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"] =
        json!({"kind":"CountObjects","args":{"kind":"EventGroup"}});
    let mut marked = mark(&value).unwrap();
    assert!(
        finish(&mut marked)
            .unwrap_err()
            .contains("observes event data")
    );
}
