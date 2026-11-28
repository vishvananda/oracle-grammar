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

#[test]
fn schema_193_appends_receipt_and_spending_variants_without_moving_wire_ordinals() {
    let old: serde_json::Value = serde_json::from_str(super::frozen_json(192).unwrap()).unwrap();
    let old: Schema = serde_json::from_value(old["schema"].clone()).unwrap();
    let published: serde_json::Value =
        serde_json::from_str(super::frozen_json(193).unwrap()).unwrap();
    let published: Schema = serde_json::from_value(published["schema"].clone()).unwrap();
    for name in ["ObjectSet", "Expr", "PlayerRule"] {
        let super::TypeDef::Enum {
            variants: before, ..
        } = &old.types[name]
        else {
            panic!("{name} must remain an enum")
        };
        let super::TypeDef::Enum {
            variants: after, ..
        } = &published.types[name]
        else {
            panic!("{name} must remain an enum")
        };
        assert_eq!(
            serde_json::to_value(&after[..before.len()]).unwrap(),
            serde_json::to_value(before).unwrap(),
            "{name}: existing wire ordinal changed"
        );
    }
}

#[test]
fn schema_195_preserves_published_ordinals_and_reviews_existing_payload_changes() {
    use super::{Fields, Ty, TypeDef};

    let previous: serde_json::Value =
        serde_json::from_str(super::frozen_json(194).unwrap()).unwrap();
    let mut previous: Schema = serde_json::from_value(previous["schema"].clone()).unwrap();
    let current = Schema::of_ir().unwrap();
    for name in ["Operation", "PlayerRule", "ObjectSet"] {
        let Some(TypeDef::Enum {
            variants: before, ..
        }) = previous.types.get_mut(name)
        else {
            panic!("{name} must be an enum");
        };
        let TypeDef::Enum {
            variants: after, ..
        } = &current.types[name]
        else {
            panic!("{name} must remain an enum");
        };
        if name == "PlayerRule" {
            let (_, Fields::Named(fields)) = before
                .iter_mut()
                .find(|(variant, _)| variant == "PlayPermission")
                .unwrap()
            else {
                panic!("published PlayPermission fields");
            };
            let (field, old_type) = fields
                .iter_mut()
                .find(|(field, _)| field == "from")
                .unwrap();
            assert_eq!(*old_type, Ty::Named("EventZone".into()));
            *field = "origins".into();
            *old_type = Ty::Named("CardPlayOrigins".into());
        } else if name == "Operation" {
            // Schema 201 adds a default-empty presentation operand at the end.
            let (_, Fields::Named(fields)) = before
                .iter_mut()
                .find(|(variant, _)| variant == "SeparatePiles")
                .unwrap()
            else {
                panic!("published SeparatePiles fields");
            };
            fields.push((
                "visibility".into(),
                Ty::Option(Box::new(Ty::Named("PileOrientations".into()))),
            ));
        }
        assert_eq!(
            serde_json::to_value(&after[..before.len()]).unwrap(),
            serde_json::to_value(before).unwrap(),
            "{name}: unreviewed existing ordinal or payload change"
        );
    }

    let Some(TypeDef::Struct(Fields::Named(terms))) = previous.types.get_mut("PlayTerms") else {
        panic!("published PlayTerms fields");
    };
    terms.push((
        "cast_filter".into(),
        Ty::Option(Box::new(Ty::Named("ObjectFilter".into()))),
    ));
    assert_eq!(
        serde_json::to_value(&current.types["PlayTerms"]).unwrap(),
        serde_json::to_value(&previous.types["PlayTerms"]).unwrap()
    );
}

#[test]
fn schema_203_preserves_object_set_ordinals() {
    let before: serde_json::Value = serde_json::from_str(super::frozen_json(202).unwrap()).unwrap();
    let before: Schema = serde_json::from_value(before["schema"].clone()).unwrap();
    let after: serde_json::Value = serde_json::from_str(super::frozen_json(203).unwrap()).unwrap();
    let after: Schema = serde_json::from_value(after["schema"].clone()).unwrap();
    let super::TypeDef::Enum { variants: old, .. } = &before.types["ObjectSet"] else {
        panic!("ObjectSet must remain an enum")
    };
    let super::TypeDef::Enum { variants: new, .. } = &after.types["ObjectSet"] else {
        panic!("ObjectSet must remain an enum")
    };
    assert_eq!(
        serde_json::to_value(&new[..old.len()]).unwrap(),
        serde_json::to_value(old).unwrap()
    );
    assert_eq!(new.len(), old.len() + 1);
    assert_eq!(new.last().unwrap().0, "EventGroupDeparture");
}

#[test]
fn schema_204_appends_sequential_exile_variants() {
    let before: serde_json::Value = serde_json::from_str(super::frozen_json(203).unwrap()).unwrap();
    let before: Schema = serde_json::from_value(before["schema"].clone()).unwrap();
    let after: serde_json::Value = serde_json::from_str(super::frozen_json(204).unwrap()).unwrap();
    let after: Schema = serde_json::from_value(after["schema"].clone()).unwrap();
    for (name, added) in [
        ("ObjectSet", "ExileUntilMatches"),
        ("Operation", "ExileLibraryUntil"),
    ] {
        let super::TypeDef::Enum { variants: old, .. } = &before.types[name] else {
            panic!("{name} must be an enum")
        };
        let super::TypeDef::Enum { variants: new, .. } = &after.types[name] else {
            panic!("{name} must be an enum")
        };
        assert_eq!(
            serde_json::to_value(&new[..old.len()]).unwrap(),
            serde_json::to_value(old).unwrap()
        );
        assert_eq!(new.len(), old.len() + 1);
        assert_eq!(new.last().unwrap().0, added);
    }
}

#[test]
fn schema_205_appends_shared_modal_variants() {
    let before: serde_json::Value = serde_json::from_str(super::frozen_json(204).unwrap()).unwrap();
    let before: Schema = serde_json::from_value(before["schema"].clone()).unwrap();
    let after = Schema::of_ir().unwrap();
    for (name, added) in [
        ("ObjectSet", "ChosenModeTargets"),
        ("ModeMechanism", "AnnouncedShared"),
    ] {
        let super::TypeDef::Enum { variants: old, .. } = &before.types[name] else {
            panic!("{name} must be an enum")
        };
        let super::TypeDef::Enum { variants: new, .. } = &after.types[name] else {
            panic!("{name} must be an enum")
        };
        assert_eq!(
            serde_json::to_value(&new[..old.len()]).unwrap(),
            serde_json::to_value(old).unwrap()
        );
        assert_eq!(new.len(), old.len() + 1);
        assert_eq!(new.last().unwrap().0, added);
    }
}
