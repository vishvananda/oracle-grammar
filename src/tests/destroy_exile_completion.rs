//! Original targets, legal direct choices and actual arrivals remain distinct.
use super::*;

fn fixtures() -> Vec<Value> {
    let value: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/final-semantics-corrections.json"
    ))
    .unwrap();
    value["cases"].as_array().unwrap().clone()
}
fn frame(name: &str) -> Value {
    canonical_abilities(&fixtures().into_iter().find(|f| f["name"] == name).unwrap()["expected"])
        .unwrap()
}
fn body(value: &mut Value) -> &mut Vec<Value> {
    value[0]["args"]["body"]["instructions"]
        .as_array_mut()
        .unwrap()
}

#[test]
fn break_expectations_requires_exile_arrival_and_reveals_the_draft() {
    let name = "Break Expectations";
    let text = "Target player reveals all cards with mana value 2 or greater in their hand. You choose a card from among those cards. Exile that card. If a card was exiled this way, that player drafts a card from Break Expectations's spellbook and reveals it.";
    let readings = engine().parse(name, text).unwrap().readings;
    assert_eq!(readings.len(), 1);
    let reading = &readings[0];
    let mut effects = Vec::new();
    operations(reading, "Effect", &mut effects);
    let moved = effects
        .iter()
        .find(|effect| effect["operation"]["kind"] == "Move")
        .unwrap();
    let mut branches = Vec::new();
    operations(reading, "If", &mut branches);
    let [branch] = branches.as_slice() else {
        panic!("one guarded draft, got {branches:?}");
    };
    assert_eq!(moved["operation"]["kind"], "Move");
    assert_eq!(
        branch["condition"],
        json!({"kind":"ObjectMatches","args":{
            "object":{"kind":"Result","args":{"result":moved["result"],"phase":"After","index":0}},
            "filter":{"kind":"InZone","args":{"Shared":"Exile"}}
        }})
    );
    let draft = &branch["then"]["instructions"][0]["args"];
    let reveal = &branch["then"]["instructions"][1]["args"];
    assert_eq!(draft["operation"]["kind"], "DraftFromSpellbook");
    assert_eq!(
        reveal["operation"]["args"]["objects"],
        json!({"kind":"Recorded","args":draft["result"]})
    );
    let trail = engine().trail_of(name, reading, text).unwrap().unwrap();
    assert_eq!(
        engine()
            .print_trail(name, reading, &trail)
            .unwrap()
            .as_deref(),
        Some(text)
    );
}
fn operations(value: &Value, name: &str, out: &mut Vec<Value>) {
    if value["kind"] == name {
        out.push(value["args"].clone());
    }
    match value {
        Value::Object(fields) => fields.values().for_each(|v| operations(v, name, out)),
        Value::Array(items) => items.iter().for_each(|v| operations(v, name, out)),
        _ => {}
    }
}

#[test]
fn remaining_reported_programs_have_their_correct_meaning_alone_and_print() {
    for case in fixtures() {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        validate_abilities(&case["expected"]).unwrap();
        let expected = canonical_abilities(&case["expected"]).unwrap();
        let context = serde_json::from_value(case["context"].clone()).unwrap();
        let parsed = engine().parse_card(name, text, &context).unwrap();
        assert_eq!(parsed.readings.len(), 1, "{name}");
        let reading = &parsed.readings[0];
        assert_eq!(reading.abilities, expected, "{name}");
        let trail = engine()
            .card_key_of(name, reading, &context, text, true)
            .unwrap()
            .unwrap();
        assert_eq!(
            engine()
                .print_card_trail(name, reading, &context, &trail)
                .unwrap(),
            Some(text.to_string()),
            "{name}"
        );
        assert!(
            engine()
                .print_card_keyed(name, reading, &context, &[])
                .unwrap()
                .is_some(),
            "{name}"
        );
    }
}

#[test]
fn post_destroy_characteristics_name_the_target_without_requiring_a_receipt() {
    for name in ["Thermokarst", "Extinguish the Light"] {
        let mut reading = frame(name);
        let items = body(&mut reading);
        let original = items[0]["args"]["operation"]["args"]["objects"]["args"].clone();
        assert_eq!(original["kind"], "Target");
        assert_eq!(items[1]["args"]["condition"]["args"]["object"], original);
        assert_eq!(items[1]["args"]["condition"]["kind"], "ObjectMatches");
        let reward = &mut items[1]["args"]["then"]["instructions"][0]["args"]["operation"]["args"];
        reward["amount"] = json!({"kind":"Constant","args":5});
        assert!(printed(&reading).contains("5 life"));
    }
}

#[test]
fn direct_destroy_legality_is_after_the_minimum_and_independent_choice_is_unchanged() {
    for name in ["Drop of Honey", "Porphyry Nodes"] {
        let reading = frame(name);
        let mut choices = Vec::new();
        operations(&reading, "ChooseObjects", &mut choices);
        assert_eq!(choices.len(), 1);
        let objects = &choices[0]["objects"];
        assert_eq!(objects["kind"], "Matching");
        assert_eq!(
            objects["args"]["filter"],
            json!({"kind":"Not","args":{"kind":"HasKeyword","args":{"kind":"Indestructible"}}})
        );
        let minimum_pool = &objects["args"]["objects"];
        assert_eq!(minimum_pool["kind"], "Zone");
        let mut aggregates = Vec::new();
        operations(minimum_pool, "Aggregate", &mut aggregates);
        assert_eq!(aggregates.len(), 1);
        assert_eq!(aggregates[0]["aggregation"], "Minimum");
        assert_eq!(aggregates[0]["property"], "Power");
        assert_eq!(
            aggregates[0]["objects"]["args"]["filter"],
            json!({"kind":"CardType","args":"Creature"})
        );
        let mut destruction = Vec::new();
        operations(&reading, "Destroy", &mut destruction);
        assert_eq!(destruction.len(), 1);
        assert_eq!(destruction[0]["regeneration"], false);
    }
    let independent = frame("Imperial Edict");
    let mut choices = Vec::new();
    operations(&independent, "ChooseObjects", &mut choices);
    assert_eq!(choices.len(), 1);
    assert!(!choices[0]["objects"].to_string().contains("Indestructible"));
    let mut destruction = Vec::new();
    operations(&independent, "Destroy", &mut destruction);
    assert_eq!(destruction[0]["regeneration"], true);
}

#[test]
fn entry_exile_is_partial_and_both_counts_filter_actual_recorded_arrivals() {
    let mut reading = frame("Frankenstein's Monster");
    let mut costs = Vec::new();
    operations(&reading, "PayCost", &mut costs);
    assert!(costs.is_empty());
    let items = body(&mut reading);
    // The choice, then the move of exactly what was chosen; later counts read the
    // move's own receipt.
    let choice = &items[0]["args"];
    assert_eq!(choice["operation"]["kind"], "ChooseObjects");
    assert_eq!(
        choice["operation"]["args"]["count"],
        json!({"AsManyAsCan":{"kind":"CastX","args":{"kind":"Source"}}})
    );
    assert_eq!(
        choice["operation"]["args"]["player"],
        json!({"kind":"AbilityController"})
    );
    let moved = &items[1]["args"];
    assert_eq!(moved["operation"]["kind"], "Move");
    assert_eq!(
        moved["operation"]["args"]["objects"],
        json!({"kind":"Selected","args":choice["result"]})
    );
    assert!(
        moved["operation"]["args"]["destination"]
            .get("Exile")
            .is_some()
    );
    let receipt = moved["result"].clone();
    let condition = &items[2]["args"]["condition"]["args"];
    let count = &condition["left"];
    assert_eq!(count["kind"], "CountObjects");
    assert_eq!(count["args"]["kind"], "Matching");
    assert_eq!(
        count["args"]["args"]["filter"],
        json!({"kind":"InZone","args":{"Shared":"Exile"}})
    );
    assert_eq!(
        count["args"]["args"]["objects"],
        json!({"kind":"Transition","args":{"result":receipt,"phase":"After"}})
    );
    assert_eq!(
        condition["test"],
        json!({"kind":"Compare","args":{"comparison":"Equal","value":{"kind":"CastX","args":{"kind":"Source"}}}})
    );
    let amount = &items[2]["args"]["then"]["instructions"][0]["args"]["operation"]["args"]["change"]
        ["Entry"]["CountersOfChoice"]["amount"];
    assert_eq!(amount["args"]["kind"], "Matching");
    assert_eq!(
        amount["args"]["args"]["filter"],
        json!({"kind":"AllOf","args":[{"kind":"CardType","args":"Creature"},
            {"kind":"InZone","args":{"Shared":"Exile"}}]})
    );
    assert_eq!(
        amount["args"]["args"]["objects"],
        json!({"kind":"Transition","args":{"result":receipt,"phase":"After"}})
    );
    assert_eq!(
        items[2]["args"]["otherwise"]["instructions"][0]["args"]["operation"]["args"]["change"],
        json!({"SetDestination":"OwnersGraveyard"})
    );
    items[0]["args"]["operation"]["args"]["count"] =
        json!({"AsManyAsCan":{"kind":"Constant","args":2}});
    items[2]["args"]["condition"]["args"]["test"]["args"]["value"] =
        json!({"kind":"Constant","args":2});
    let text = printed(&reading);
    assert!(!text.contains('X'), "{text}");
    assert!(text.contains("two") || text.contains('2'), "{text}");
    let cost = engine()
        .parse(
            "Context example",
            "Exile two creature cards from your graveyard: Draw a card.",
        )
        .unwrap()
        .readings
        .remove(0);
    assert_eq!(
        cost[0]["args"]["cost"]["parts"][0]["Move"]["selections"][0]["choose"],
        json!({"Exactly":{"kind":"Constant","args":2}})
    );
}
