//! Linked stat variables use every current matching card, with independent properties.
use super::*;

fn linked() -> Value {
    compositional(
        "This creature gets +X/+Y, where X is the exiled creature card's power and Y is its toughness.",
    )
}
fn operands(reading: &mut Value) -> &mut Value {
    &mut reading[0]["args"]["changes"][0]["args"]
}
fn anchor(value: &mut Value, replacement: &Value) {
    if value["kind"] == "ExiledWith" {
        value["args"] = replacement.clone();
    } else if let Some(fields) = value.as_object_mut() {
        fields.values_mut().for_each(|v| anchor(v, replacement));
    } else if let Some(items) = value.as_array_mut() {
        items.iter_mut().for_each(|v| anchor(v, replacement));
    }
}
fn artifacts(value: &mut Value) {
    if *value == json!({"kind":"CardType","args":"Creature"}) {
        value["args"] = json!("Artifact");
    } else if let Some(fields) = value.as_object_mut() {
        fields.values_mut().for_each(artifacts);
    } else if let Some(items) = value.as_array_mut() {
        items.iter_mut().for_each(artifacts);
    }
}

#[test]
fn linked_stat_variables_sum_one_current_creature_population() {
    let mut reading = linked();
    let stats = operands(&mut reading);
    assert_eq!(stats["power"]["kind"], "Aggregate");
    assert_eq!(stats["toughness"]["kind"], "Aggregate");
    assert_eq!(stats["power"]["args"]["property"], "Power");
    assert_eq!(stats["toughness"]["args"]["property"], "Toughness");
    assert_eq!(stats["power"]["args"]["aggregation"], "Sum");
    assert_eq!(stats["toughness"]["args"]["aggregation"], "Sum");
    let objects = &stats["power"]["args"]["objects"];
    assert_eq!(*objects, stats["toughness"]["args"]["objects"]);
    assert_eq!(objects["kind"], "Zone");
    assert_eq!(objects["args"]["scope"], json!({"Shared":"Exile"}));
    assert_eq!(objects["args"]["kind"], "Card");
    let filter = objects["args"]["filter"]["args"].as_array().unwrap();
    assert_eq!(filter.len(), 2);
    assert!(filter.contains(&json!({"kind":"CardType","args":"Creature"})));
    assert!(filter.contains(&json!({"kind":"ExiledWith","args":{"kind":"Source"}})));
    validate_abilities(&reading).unwrap();
    assert_eq!(canonical_abilities(&reading).unwrap(), reading);
    printed(&reading);
}

#[test]
fn a_linked_population_cannot_canonicalize_to_an_arbitrary_first_card() {
    let reading = linked();
    let mut first = reading.clone();
    for field in ["power", "toughness"] {
        let aggregate = operands(&mut first)[field].clone();
        operands(&mut first)[field] = json!({"kind":"ObjectProperty","args":{
            "object":{"kind":"Query","args":{"objects":aggregate["args"]["objects"],"index":0}},
            "property":aggregate["args"]["property"],"information":"CurrentOrLastKnown"}});
    }
    assert_ne!(canonical_abilities(&first).unwrap(), reading);
    let mut captured = reading.clone();
    operands(&mut captured)["toughness"] = json!({"kind":"Earlier","args":{
        "what":"Exiled","measure":{"Of":"Toughness"},"result":null,"filter":null,"by":null}});
    assert_ne!(canonical_abilities(&captured).unwrap(), reading);
}

#[test]
fn edited_linked_filters_properties_and_hosts_print_current_operands() {
    let mut reading = linked();
    let host = json!({"kind":"AttachedTo","args":{"kind":"Source"}});
    reading[0]["args"]["affected"]["Objects"] = json!({"kind":"One","args":host});
    let stats = operands(&mut reading);
    artifacts(stats);
    anchor(stats, &host);
    stats["power"]["args"]["property"] = json!("ManaValue");
    stats["toughness"]["args"]["property"] = json!("Power");
    let text = printed_types(&reading, &["Enchantment", "Aura"]);
    assert!(text.contains("artifact cards exiled with"), "{text}");
    assert!(text.contains("mana value"), "{text}");
    assert!(text.to_lowercase().contains("enchanted creature"), "{text}");
}

#[test]
fn independent_linked_anchors_do_not_reuse_the_first_definition() {
    let mut reading = linked();
    let host = json!({"kind":"AttachedTo","args":{"kind":"Source"}});
    reading[0]["args"]["affected"]["Objects"] = json!({"kind":"One","args":host});
    anchor(&mut operands(&mut reading)["power"], &host);
    let text = printed_types(&reading, &["Enchantment", "Aura"]);
    assert!(text.contains("exiled with"), "{text}");
    let normalized = canonical_abilities(&reading).unwrap();
    assert_ne!(
        normalized[0]["args"]["changes"][0]["args"]["power"]["args"]["objects"],
        normalized[0]["args"]["changes"][0]["args"]["toughness"]["args"]["objects"]
    );
}
