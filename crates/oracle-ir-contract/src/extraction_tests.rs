//! Frozen outputs from the independently built MTGish extraction baseline.
use crate::{canonical, corpus, human, ir::UnifiedCard, migrate, validate};
use serde_json::Value;

fn current_expected(name: &str, mut value: Value, source: &Value) -> Value {
    for (epoch, text) in [
        (192, include_str!("fixtures/extraction-192-delta.json")),
        (193, include_str!("fixtures/extraction-193-delta.json")),
    ] {
        apply_delta(name, &mut value, source, epoch, text);
    }
    value
}

fn apply_delta(name: &str, value: &mut Value, source: &Value, epoch: u32, text: &str) {
    let delta: Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        delta["source_fixture_sha256"],
        source["source_fixture_sha256"]
    );
    // Each reviewed comparison delta keeps its own epoch. Migration already
    // upgrades the cards; historical fixture bytes never change.
    assert_eq!(delta["schema_version"], epoch);
    for change in delta["changes"].as_array().unwrap() {
        if change["name"] != name {
            continue;
        }
        let mut place = &mut *value;
        for step in change["path"].as_array().unwrap() {
            place = if let Some(key) = step.as_str() {
                place.get_mut(key).unwrap()
            } else {
                place
                    .get_mut(usize::try_from(step.as_u64().unwrap()).unwrap())
                    .unwrap()
            };
        }
        assert_eq!(
            *place, change["from"],
            "{name}: reviewed schema-{epoch} delta"
        );
        *place = change["to"].clone();
    }
}

#[test]
fn schema_190_extraction_preserves_json_binary_canonical_and_authoring_contracts() {
    corpus::on_large_stack(|| {
        let packet: Value =
            serde_json::from_str(include_str!("fixtures/extraction-190.json")).unwrap();
        assert_eq!(
            packet["producer_revision"],
            "54abf45e9992e52a49f7569f1d038b393a0fa9b1"
        );
        let records = packet["records"].as_array().unwrap();
        assert_eq!(records.len(), 40);
        for record in records {
            let name = record["name"].as_str().unwrap();
            // Historical JSON goes through its frozen schema before current
            // binary and authoring roundtrips.
            let card = migrate::read_json(&record["card"].to_string()).unwrap();
            let current = serde_json::to_value(&card).unwrap();
            assert_eq!(
                serde_json::to_value(
                    serde_json::from_value::<UnifiedCard>(current.clone()).unwrap()
                )
                .unwrap(),
                current,
                "{name}: JSON"
            );
            let bytes = bincode::encode_to_vec(&card, bincode::config::standard()).unwrap();
            let (decoded, consumed): (UnifiedCard, usize) =
                bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
            assert_eq!(consumed, bytes.len());
            assert_eq!(
                serde_json::to_value(decoded).unwrap(),
                current,
                "{name}: binary"
            );
            validate::unified(&card).unwrap_or_else(|e| panic!("{name}: {e}"));
            for face in &card.faces {
                validate::card(face).unwrap_or_else(|e| panic!("{name}: {e}"));
            }
            let expected = migrate::read_json(&record["canonical"].to_string()).unwrap();
            let expected = current_expected(name, serde_json::to_value(expected).unwrap(), &packet);
            let normalized = canonical::unified(&card).unwrap();
            assert_eq!(
                serde_json::to_value(&normalized).unwrap(),
                expected,
                "{name}: canonical"
            );
            let code = human::write(&card).unwrap_or_else(|e| panic!("{name}: {e}"));
            let legacy_code = human::read(record["code"].as_str().unwrap())
                .unwrap_or_else(|e| panic!("{name}: legacy authoring: {e}"));
            assert_eq!(
                human::write(&legacy_code).unwrap(),
                code,
                "{name}: authoring code"
            );
            let reparsed = human::read(&code).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                serde_json::to_value(canonical::unified(&reparsed).unwrap()).unwrap(),
                expected,
                "{name}: edited-code path"
            );
        }
    });
}

#[test]
fn schema_200_upgrades_pile_visibility_and_preserves_play_restrictions() {
    let packet: Value = serde_json::from_str(include_str!("fixtures/extraction-190.json")).unwrap();
    let records = packet["records"].as_array().unwrap();
    let current = |name: &str| {
        let record = records
            .iter()
            .find(|record| record["name"] == name)
            .unwrap();
        let card = migrate::read_json(&record["card"].to_string()).unwrap();
        serde_json::to_value(card).unwrap()
    };
    let as_200 = |mut value: Value| {
        value["schema_version"] = serde_json::json!(200);
        value["faces"][0]["schema_version"] = serde_json::json!(200);
        value
    };

    let mut piles = current("Brainstorm");
    piles["faces"][0]["abilities"] = serde_json::json!([{
        "kind": "Spell", "args": {"announcement": {"targets": [], "modes": null},
        "body": {"instructions": [{"kind": "Effect", "args": {"result": 0,
        "operation": {"kind": "SeparatePiles", "args": {
            "separator": {"kind": "AbilityController"},
            "chooser": {"kind": "OneOf", "args": {"kind": "OpponentsOf", "args": {"kind": "AbilityController"}}},
            "objects": {"kind": "Zone", "args": {"scope": {"Players": {"zone": "Library",
                "players": {"kind": "One", "args": {"kind": "AbilityController"}}}},
                "kind": "Card", "filter": {"kind": "Any"}}}, "visibility": null
        }}}}]}}
    }]);
    let mut old_piles = as_200(piles.clone());
    old_piles
        .pointer_mut("/faces/0/abilities/0/args/body/instructions/0/args/operation/args")
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("visibility");
    let mut restricted = current("Evelyn, the Covetous");
    let terms = concat!(
        "/faces/0/abilities/2/args/changes/0/args/SpecialAction/body/args",
        "/instructions/0/args/operation/args/terms/cast_filter"
    );
    *restricted.pointer_mut(terms).unwrap() =
        serde_json::json!({"kind":"CardType", "args":"Creature"});
    let permission = current("Conspicuous Snoop");
    for (old, expected) in [
        (old_piles, piles),
        (as_200(restricted.clone()), restricted),
        (as_200(permission.clone()), permission),
    ] {
        let mapped = migrate::mapped_read_json(&old.to_string()).unwrap();
        assert_eq!(mapped.source_version, 200);
        assert!(mapped.map.introduced.is_empty());
        assert_eq!(serde_json::to_value(mapped.card).unwrap(), expected);
    }
}
