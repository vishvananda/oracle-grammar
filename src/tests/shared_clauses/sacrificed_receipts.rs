//! "The sacrificed <noun>" names the current resolution sacrifice receipt.
use super::*;

fn family(name: &str) -> Value {
    let family: Value =
        serde_json::from_str(include_str!("../../../families/sacrificed-receipts.json")).unwrap();
    family["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap()["expected"]
        .clone()
}

fn properties(value: &mut Value, edit: &mut dyn FnMut(&mut Value)) {
    match value {
        Value::Object(fields) => {
            if fields.get("kind") == Some(&json!("ObjectProperty")) {
                edit(value);
                return;
            }
            fields.values_mut().for_each(|v| properties(v, edit));
        }
        Value::Array(items) => items.iter_mut().for_each(|v| properties(v, edit)),
        _ => {}
    }
}

#[test]
fn reflexive_and_paid_receipts_print_an_edited_property() {
    for (name, from, to) in [
        ("Flinging Raider", "Power", "Toughness"),
        ("Grim Offering", "Toughness", "Power"),
        ("Scrap Diviner", "ManaValue", "Power"),
    ] {
        let mut frame = family(name);
        properties(&mut frame, &mut |property| {
            assert_eq!(property["args"]["property"], from, "{name}");
            property["args"]["property"] = json!(to);
        });
        let text = printed(&frame);
        assert!(text.contains(&to.to_lowercase()), "{text}");
    }
}

#[test]
fn a_paid_receipt_keeps_its_resolution_payment_and_reflexive_its_transition() {
    let mut reflexive = Vec::new();
    visit(&family("Flinging Hunter"), "Query", &mut reflexive);
    assert_eq!(
        reflexive[0]["objects"],
        json!({"kind":"Transition","args":{"result":0,"phase":"Before"}})
    );
    let mut paid = Vec::new();
    visit(&family("Scrap Diviner"), "Query", &mut paid);
    assert_eq!(
        paid[0]["objects"]["args"]["payment"],
        json!({"Resolution":0})
    );
}

#[test]
fn no_or_plural_sacrifice_does_not_name_a_sacrificed_object() {
    for text in [
        "When this creature enters, you gain life equal to the sacrificed creature's toughness.",
        "When this creature enters, sacrifice two creatures. When you do, you gain life equal to the sacrificed creature's toughness.",
    ] {
        assert!(
            matches!(
                engine().parse("Context example", text),
                Err(Refusal::Binding(_))
            ),
            "{text}"
        );
    }
}
