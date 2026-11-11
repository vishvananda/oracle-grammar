//! Entry X and independent ability definitions are distinct lexical values.
use super::{ability_kind, events, holder, payments};
use crate::model::ir::EventSpec;
use serde_json::{Value, json};

pub(super) fn entry(mut ability: &Value) -> bool {
    while matches!(ability_kind(ability), Some("Scoped" | "WithPresentation")) {
        ability = &ability["args"]["ability"];
    }
    if ability["args"]["active"] == "SourceEntering" {
        return true;
    }
    if ability_kind(ability) != Some("Triggered") {
        return false;
    }
    let mut event = &ability["args"]["event"];
    while let Some(inner) = events::inner(event) {
        event = inner;
    }
    event["ZoneChange"]["to"] == "Battlefield" && holder::self_event(event, 0)
}

/// Published event binding semantics prove that every matched event exposes
/// the X chosen for its cycling activation. Unknown/mixed events do not prove it.
pub(super) fn activation(ability: &Value) -> bool {
    let mut ability = ability;
    while matches!(ability_kind(ability), Some("Scoped" | "WithPresentation")) {
        ability = &ability["args"]["ability"];
    }
    if ability_kind(ability) != Some("Triggered") {
        return false;
    }
    let event = &ability["args"]["event"];
    if crate::functions::branch_bounds(event).is_err() {
        return false;
    }
    serde_json::from_value::<EventSpec>(event.clone())
        .is_ok_and(|event| crate::model::semantics::event_bindings(&event).activation_x)
}

pub(super) fn bind(ability: &mut Value) -> Result<(), String> {
    transform(ability, false)
}

pub(super) fn surface(ability: &mut Value) -> Result<(), String> {
    transform(ability, true)
}

fn transform(ability: &mut Value, inverse: bool) -> Result<(), String> {
    let origin = if entry(ability) {
        Origin::Cast
    } else {
        Origin::Unbound
    };
    walk(
        ability,
        Context {
            origin,
            abilities: false,
            activation_x: false,
        },
        inverse,
    )
}

#[derive(Clone, Copy)]
enum Origin {
    Unbound,
    Cast,
    /// CR 107.3j: gained abilities define their own X, otherwise it is zero.
    Gained,
}

#[derive(Clone, Copy)]
struct Context {
    origin: Origin,
    /// The nearest independently matched event owns this activation value.
    activation_x: bool,
    /// This object is the payload of a token recipe or CreateEmblem.
    abilities: bool,
}

fn number(origin: Origin) -> Option<Value> {
    match origin {
        Origin::Unbound => None,
        Origin::Cast => Some(json!({"kind":"CastX", "args":{"kind":"Source"}})),
        Origin::Gained => Some(json!({"kind":"Constant", "args":0})),
    }
}

fn shadowed(value: &Value) -> bool {
    matches!(
        ability_kind(value),
        Some("Activated" | "Spell" | "Mechanic" | "Casting")
    ) && payments::declared(&value["args"]["cost"])
}

/// Local payment choices have already become BoundValue before this pass.
/// Fresh definitions never inherit X just because their creator used X.
fn walk(value: &mut Value, mut context: Context, inverse: bool) -> Result<(), String> {
    match ability_kind(value) {
        Some("Triggered") => {
            context.activation_x = activation(value);
            if context.activation_x {
                crate::functions::branch_bounds(value)?;
            }
        }
        Some(
            "Spell"
            | "Activated"
            | "Static"
            | "Replacement"
            | "Casting"
            | "Mechanic"
            | "CreateTrigger"
            | "CreateDelayedTrigger"
            | "ReflexiveTrigger"
            | "CreateReplacement",
        ) => {
            context.activation_x = false;
        }
        _ => {}
    }
    if context.activation_x {
        if !inverse && crate::functions::is_x(value) {
            *value = json!({"kind":"EventActivationX"});
            return Ok(());
        }
        if inverse && *value == json!({"kind":"EventActivationX"}) {
            *value = json!({"kind":"AnnouncedX"});
            return Ok(());
        }
    }
    if shadowed(value) {
        context.origin = Origin::Unbound;
    }
    if !inverse && crate::functions::is_x(value) {
        if let Some(number) = number(context.origin) {
            *value = number;
        }
        return Ok(());
    }
    if inverse
        && matches!(context.origin, Origin::Cast)
        && *value == json!({"kind":"CastX", "args":{"kind":"Source"}})
    {
        *value = json!({"kind":"AnnouncedX"});
        return Ok(());
    }
    // Announced-cost readings spend an inherited value. Chosen payment costs
    // already use ManaWithX{x:null}, with BoundValue uses in their continuation.
    if !inverse
        && let Some(mana) = value.get("Mana").and_then(Value::as_array)
        && mana.iter().any(|symbol| symbol == "X")
        && let Some(number) = number(context.origin)
    {
        *value = if matches!(context.origin, Origin::Gained) {
            let mana: Vec<_> = mana
                .iter()
                .map(|symbol| {
                    if symbol == "X" {
                        json!({"Generic":0})
                    } else {
                        symbol.clone()
                    }
                })
                .collect();
            json!({"Mana":mana})
        } else {
            json!({"ManaWithX":{"mana":mana,"x":number}})
        };
        return Ok(());
    }
    if inverse
        && let Some(part) = value.get("ManaWithX")
        && matches!(context.origin, Origin::Cast)
        && part["x"] == json!({"kind":"CastX", "args":{"kind":"Source"}})
    {
        *value = json!({"Mana":part["mana"]});
        return Ok(());
    }
    let grant = matches!(ability_kind(value), Some("GrantAbility"));
    let emblem = ability_kind(value) == Some("CreateEmblem");
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                let mut next = Context {
                    abilities: key == "Defined" || emblem && key == "args",
                    ..context
                };
                if grant && key == "args" {
                    next.origin = Origin::Gained;
                    next.activation_x = false;
                } else if context.abilities && key == "abilities" {
                    next.origin = Origin::Unbound;
                    next.activation_x = false;
                }
                walk(child, next, inverse)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                walk(child, context, inverse)?;
            }
        }
        _ => {}
    }
    // References and X are now concrete. Reuse this existing postorder walk;
    // unmarked abilities need no additional tree traversal.
    if !inverse && ability_kind(value) == Some("Triggered") {
        crate::functions::trigger_batches::finish(value)?;
    }
    Ok(())
}

#[cfg(test)]
mod cycling_tests {
    use super::*;

    fn cycling() -> Value {
        json!({"Discard":{"players":{"kind":"One","args":{"kind":"AbilityController"}},
            "cards":{"kind":"Any"},"source_only":true,"kind":"Cycling","grouping":"Card"}})
    }
    fn trigger(event: Value) -> Value {
        json!({"kind":"Triggered","args":{"event":event,
            "announcement":{"targets":[{"count":{"UpTo":{"kind":"AnnouncedX"}}}]},
            "body":{"amount":{"kind":"AnnouncedX"},"local":{"kind":"BoundValue","args":7},
                "holder":{"kind":"Source"},"creator":{"kind":"GrantingObject"}}}})
    }

    #[test]
    fn typed_cycling_event_owns_announcement_filter_and_body_values() {
        let mut value = trigger(cycling());
        value["args"]["announcement"]["filter"] =
            json!({"kind":"ManaValue","args":{"value":{"kind":"AnnouncedX"}}});
        let original = value.clone();
        assert!(activation(&value));
        bind(&mut value).unwrap();
        assert_eq!(
            value["args"]["announcement"]["targets"][0]["count"]["UpTo"],
            json!({"kind":"EventActivationX"})
        );
        assert_eq!(
            value["args"]["announcement"]["filter"]["args"]["value"],
            json!({"kind":"EventActivationX"})
        );
        assert_eq!(
            value["args"]["body"]["amount"],
            json!({"kind":"EventActivationX"})
        );
        assert_eq!(
            value["args"]["body"]["local"],
            original["args"]["body"]["local"]
        );
        assert_eq!(
            value["args"]["body"]["holder"],
            original["args"]["body"]["holder"]
        );
        assert_eq!(
            value["args"]["body"]["creator"],
            original["args"]["body"]["creator"]
        );
        surface(&mut value).unwrap();
        assert_eq!(value, original);
    }

    #[test]
    fn ordinary_discard_and_mixed_event_alternatives_do_not_own_activation_x() {
        let mut discard = cycling();
        discard["Discard"]["kind"] = json!("Any");
        for event in [discard.clone(), json!({"AnyOf":[cycling(),discard]})] {
            let mut value = trigger(event);
            let old = value.clone();
            assert!(!activation(&value));
            bind(&mut value).unwrap();
            assert_eq!(value, old);
        }
        let mut value = trigger(json!({"FirstEachTurn":cycling()}));
        assert!(activation(&value));
        bind(&mut value).unwrap();
        assert_eq!(
            value["args"]["body"]["amount"],
            json!({"kind":"EventActivationX"})
        );
    }

    #[test]
    fn fresh_ability_and_replacement_frames_reset_the_enclosing_activation_value() {
        for kind in [
            "Spell",
            "Activated",
            "Static",
            "Replacement",
            "CreateTrigger",
            "CreateDelayedTrigger",
            "ReflexiveTrigger",
            "CreateReplacement",
        ] {
            let mut value = trigger(cycling());
            value["args"]["body"]["child"] = json!({"kind":kind,"args":{
                "cost":{"parts":[{"Mana":["X"]}]},"amount":{"kind":"AnnouncedX"}
            }});
            bind(&mut value).unwrap();
            assert_eq!(
                value["args"]["body"]["amount"],
                json!({"kind":"EventActivationX"})
            );
            assert_eq!(
                value["args"]["body"]["child"]["args"]["amount"],
                json!({"kind":"AnnouncedX"}),
                "{kind}"
            );
            assert_eq!(
                value["args"]["body"]["child"]["args"]["cost"]["parts"],
                json!([{"Mana":["X"]}])
            );
        }
    }

    #[test]
    fn gained_cycling_trigger_establishes_its_own_event_but_token_abilities_do_not_inherit_it() {
        let mut gained = json!({"kind":"GrantAbility","args":trigger(cycling())});
        bind(&mut gained).unwrap();
        assert_eq!(
            gained["args"]["args"]["body"]["amount"],
            json!({"kind":"EventActivationX"})
        );
        let mut value = trigger(cycling());
        value["args"]["body"]["token"] = json!({"Defined":{"power_toughness":[{"kind":"AnnouncedX"},{"kind":"AnnouncedX"}],
            "abilities":[{"kind":"Activated","args":{"cost":{"parts":[{"Mana":["X"]}]},"amount":{"kind":"AnnouncedX"}}}]}});
        bind(&mut value).unwrap();
        let defined = &value["args"]["body"]["token"]["Defined"];
        assert_eq!(
            defined["power_toughness"],
            json!([{"kind":"EventActivationX"},{"kind":"EventActivationX"}])
        );
        assert_eq!(
            defined["abilities"][0]["args"]["amount"],
            json!({"kind":"AnnouncedX"})
        );
    }

    #[test]
    fn activation_proof_and_owned_transform_are_bounded() {
        let mut event = cycling();
        for _ in 0..130 {
            event = json!({"FirstEachTurn":event});
        }
        assert!(!activation(&trigger(event)));
        let mut value = trigger(cycling());
        value["args"]["body"]["oversized"] = json!(vec![0; 16_384]);
        assert!(bind(&mut value).is_err());
    }

    #[test]
    fn end_to_end_native_cycling_x_and_independent_announcements_preflight() {
        use crate::engine::{Engine, canonical_abilities};
        let engine = Engine::load().unwrap();
        let fixtures: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/cycling-activation-x.json"
        ))
        .unwrap();
        for case in fixtures {
            let name = case["name"].as_str().unwrap();
            let parsed = engine.parse(name, case["text"].as_str().unwrap()).unwrap();
            assert_eq!(
                parsed.readings,
                vec![canonical_abilities(&case["expected"]).unwrap()],
                "{name}"
            );
            if let Some(old) = case.get("rejected_reading") {
                assert!(
                    !parsed.readings.contains(&canonical_abilities(old).unwrap()),
                    "{name}: old announced X is not the cycling activation"
                );
            }
        }
    }

    #[test]
    fn end_to_end_cycling_event_x_inverse_preflight() {
        use crate::engine::{Engine, canonical_abilities};
        let engine = Engine::load().unwrap();
        let family: Value =
            serde_json::from_str(include_str!("../../families/cycling-activation-x.json")).unwrap();
        for case in family["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let text = case["text"].as_str().unwrap();
            let expected = canonical_abilities(&case["expected"]).unwrap();
            let trail = engine
                .trail_of(name, &expected, text)
                .unwrap()
                .expect("cycling activation X prints as lexical X");
            let printed = engine
                .print_trail(name, &expected, &trail)
                .unwrap()
                .expect("verified cycling inverse");
            assert_eq!(printed, text);
            assert!(
                engine
                    .parse(name, &printed)
                    .unwrap()
                    .readings
                    .contains(&expected)
            );
        }
    }
}
