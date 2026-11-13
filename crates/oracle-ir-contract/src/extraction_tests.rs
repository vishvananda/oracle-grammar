//! Frozen outputs from the independently built MTGish extraction baseline.
use crate::{canonical, corpus, human, ir::UnifiedCard, migrate, validate};
use serde_json::Value;

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
            let card: UnifiedCard = serde_json::from_value(record["card"].clone()).unwrap();
            assert_eq!(
                serde_json::to_value(&card).unwrap(),
                record["card"],
                "{name}: JSON"
            );
            let bytes = bincode::encode_to_vec(&card, bincode::config::standard()).unwrap();
            let (decoded, consumed): (UnifiedCard, usize) =
                bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
            assert_eq!(consumed, bytes.len());
            assert_eq!(
                serde_json::to_value(decoded).unwrap(),
                record["card"],
                "{name}: binary"
            );
            // Keep historical JSON, binary and authoring fixtures untouched.
            // Only the explicit migration boundary advances their epoch.
            let card = migrate::read_json(&serde_json::to_string(&card).unwrap()).unwrap();
            validate::unified(&card).unwrap_or_else(|e| panic!("{name}: {e}"));
            for face in &card.faces {
                validate::card(face).unwrap_or_else(|e| panic!("{name}: {e}"));
            }
            let expected = migrate::read_json(&record["canonical"].to_string()).unwrap();
            let expected = serde_json::to_value(expected).unwrap();
            let normalized = canonical::unified(&card).unwrap();
            assert_eq!(
                serde_json::to_value(&normalized).unwrap(),
                expected,
                "{name}: canonical"
            );
            let code = human::write(&card).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(code, record["code"], "{name}: authoring code");
            let reparsed = human::read(&code).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                serde_json::to_value(canonical::unified(&reparsed).unwrap()).unwrap(),
                expected,
                "{name}: edited-code path"
            );
        }
    });
}
