//! Freeze the typed shape that grammar declarations target, including wire order.
use super::Schema;

fn signature() -> String {
    let schema = Schema::of_ir().unwrap();
    let mut names: Vec<_> = schema.types.keys().collect();
    names.sort();
    let mut out = format!("schema_version={}\n", crate::ir::SCHEMA_VERSION);
    for name in names {
        out.push_str(&format!("{name}: {:?}\n", schema.types[name]));
    }
    out
}

#[test]
fn schema_shape_is_frozen() {
    let fixture = super::frozen_signature(crate::ir::SCHEMA_VERSION)
        .expect("current IR schema signature must be frozen");
    assert_eq!(
        signature(),
        fixture,
        "IR shape changed: bump the schema, review grammar patterns, and freeze a new contract"
    );
}

#[test]
#[ignore = "explicit contract generation; never updates the checked fixture"]
fn dump_schema_contract() {
    println!("BEGIN_SCHEMA_CONTRACT\n{}END_SCHEMA_CONTRACT", signature());
}

/// Migration origins must be interpreted using the schema at the saved epoch,
/// not a reconstruction from whichever types happen to compile today.
#[test]
#[ignore = "explicit frozen descriptor generation"]
fn dump_schema_descriptors() {
    println!("BEGIN_SCHEMA_DESCRIPTORS\n{}\nEND_SCHEMA_DESCRIPTORS", serde_json::to_string_pretty(&serde_json::json!({"schema_version":crate::ir::SCHEMA_VERSION,"schema":Schema::of_ir().unwrap()})).unwrap());
}

#[test]
fn frozen_descriptors_match_the_compiled_contract() {
    let fixture = super::frozen_json(crate::ir::SCHEMA_VERSION)
        .expect("current IR schema descriptors must be frozen");
    let value: serde_json::Value = serde_json::from_str(fixture).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"schema_version":crate::ir::SCHEMA_VERSION,"schema":Schema::of_ir().unwrap()})
    );
    let saved: Schema = serde_json::from_value(value["schema"].clone()).unwrap();
    assert_eq!(serde_json::to_value(saved).unwrap(), value["schema"]);
}

#[test]
fn schema_192_appends_variants_without_moving_existing_wire_ordinals() {
    let old: serde_json::Value = serde_json::from_str(super::frozen_json(191).unwrap()).unwrap();
    let old: Schema = serde_json::from_value(old["schema"].clone()).unwrap();
    let current = Schema::of_ir().unwrap();
    for name in ["ObjectSet", "ObjectFilter", "EventChange"] {
        let super::TypeDef::Enum {
            variants: before, ..
        } = &old.types[name]
        else {
            panic!("{name} must remain an enum")
        };
        let super::TypeDef::Enum {
            variants: after, ..
        } = &current.types[name]
        else {
            panic!("{name} must remain an enum")
        };
        assert_eq!(
            serde_json::to_value(&after[..before.len()]).unwrap(),
            serde_json::to_value(before).unwrap(),
            "{name}: existing ordinal or operand changed"
        );
    }
}
