//! One two-sentence damage contrast can repeat its written actor with bare it.
use crate::model::ir::ObjectRef;
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

const LATER: &str = "/args/then/instructions/0/args/operation/args/source";
const EARLIER: &str = "/args/otherwise/instructions/0/args/operation/args/source";

fn sources(value: &Value) -> Option<(&Value, &Value)> {
    if value["kind"] != "If" {
        return None;
    }
    for branch in ["then", "otherwise"] {
        let [instruction] = value["args"][branch]["instructions"].as_array()?.as_slice() else {
            return None;
        };
        if instruction["kind"] != "Effect"
            || instruction["args"]["operation"]["kind"] != "DealDamage"
        {
            return None;
        }
    }
    Some((value.pointer(EARLIER)?, value.pointer(LATER)?))
}

fn bare_it(mut value: &Value) -> bool {
    // This alternate spells the same bare actor word, with a separate cost
    // proof for other contexts. Here the written repeated-actor relation owns it.
    if value["$sym"] == "sacrificed_damage_source" {
        let Some([inner]) = value["args"].as_array().map(Vec::as_slice) else {
            return false;
        };
        value = inner;
    }
    if value["$sym"] != "member_count_it" {
        return false;
    }
    let Some([inner]) = value["args"].as_array().map(Vec::as_slice) else {
        return false;
    };
    inner["$sym"] == "it" && inner["args"] == json!([])
}

/// Capture only the actor relation declared by instead_damage. Reusing the
/// complete lexical prototype preserves target-introduction identity, selected
/// result indices, holder/creator ownership and event-incarnation binding.
pub(crate) fn capture(mut value: Value) -> Result<Value, String> {
    super::branch_bounds(&value)?;
    if let Some((earlier, later)) = sources(&value)
        && bare_it(later)
    {
        let mut actor = earlier.clone();
        // Fixed holder/card names have the same identity at the repeated word,
        // but their source hook needs the later Effect's result already bound.
        // Retain contextual-pronoun positions and target-introduction keys;
        // replaying those at a later word could select a different operand.
        if matches!(actor["$sym"].as_str(), Some("self" | "named_source"))
            && actor["args"] == json!([])
            && let Some(at) = later["at"].as_u64()
        {
            actor["at"] = json!(at);
        }
        *value
            .pointer_mut(LATER)
            .ok_or("repeated damage actor path")? = actor;
    }
    Ok(value)
}

/// The inverse projects the bare spelling only when both current scalar actors
/// agree. Target introduction metadata may differ from its own later reference;
/// no source/event alias or arbitrary member projection participates.
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [argument] = args else {
        return Vec::new();
    };
    if super::branch_bounds(value).is_err() {
        return Vec::new();
    }
    let mut found = argument.unify(value, bindings.clone());
    if let Some((earlier, later)) = sources(value) {
        let actor = super::plain(earlier);
        if actor == super::plain(later) && serde_json::from_value::<ObjectRef>(actor).is_ok() {
            let mut projected = value.clone();
            *projected
                .pointer_mut(LATER)
                .expect("checked repeated damage actor path") =
                json!({"$sym":"member_count_it","args":[{"$sym":"it","args":[],"at":0}],"at":0});
            found.extend(argument.unify_made(&projected, &bindings));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ir_schema::Ty;

    fn it() -> Value {
        json!({"$sym":"member_count_it","args":[{"$sym":"it","args":[],"at":20}],"at":20})
    }
    fn damage(source: Value, at: u32) -> Value {
        json!({"kind":"Effect","args":{"result":{"$sym":"next","args":[],"at":at},
            "operation":{"kind":"DealDamage","args":{"source":source,
                "recipients":{"kind":"Players","args":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}}},
                "amount":{"kind":"Constant","args":1},"preventable":true}}}})
    }
    fn contrast(earlier: Value, later: Value) -> Value {
        json!({"kind":"If","args":{"condition":{"kind":"PlayerState","args":{"player":{"kind":"AbilityController"},"state":"FullParty"}},
            "then":{"instructions":[damage(later,20)]},"otherwise":{"instructions":[damage(earlier,5)]}}})
    }
    fn target(at: u32) -> Value {
        json!({"kind":"Target","args":{"slot":{"$sym":"target","at":at,"args":[{
            "slot":{"$sym":"slot","args":[],"at":at},"kind":{"Object":{"kind":"Permanent","zones":["Battlefield"]}},
            "count":{"Exactly":{"kind":"Constant","args":1}},"filter":{"kind":"CardType","args":"Creature"},
            "players":null,"distribute":null,"constraints":[],"chooser":null}]},"index":0}})
    }

    #[test]
    fn repeated_damage_actor_preserves_each_scalar_and_only_the_literal_relation() {
        for actor in [
            json!({"kind":"Source"}),
            json!({"kind":"GrantingObject"}),
            json!({"kind":"Target","args":{"slot":2,"index":1}}),
            json!({"kind":"Result","args":{"result":3,"phase":"After","index":4}}),
            json!({"kind":"EventSubject"}),
            json!({"kind":"EventStackObject"}),
        ] {
            for repeated in [
                it(),
                json!({"$sym":"sacrificed_damage_source","args":[it()],"at":20}),
            ] {
                let original = contrast(actor.clone(), repeated);
                let captured = capture(original.clone()).unwrap();
                assert_eq!(captured.pointer(EARLIER), Some(&actor));
                assert_eq!(captured.pointer(LATER), Some(&actor));
                let mut restored = captured.clone();
                *restored.pointer_mut(LATER).unwrap() = original.pointer(LATER).unwrap().clone();
                assert_eq!(restored, original);
            }
        }
        for second in [
            json!({"$sym":"named_source","args":[],"at":20}),
            json!({"$sym":"self","args":[],"at":20}),
            json!({"$sym":"qualified_object","args":["Creature",{"kind":"CardType","args":"Creature"}],"at":20}),
            json!({"$sym":"literal_damage_itself","args":[it()],"at":20}),
        ] {
            let original = contrast(json!({"kind":"GrantingObject"}), second);
            assert_eq!(capture(original.clone()).unwrap(), original);
        }
        let mut multiple = contrast(json!({"kind":"Source"}), it());
        multiple["args"]["otherwise"]["instructions"]
            .as_array_mut()
            .unwrap()
            .push(damage(json!({"kind":"GrantingObject"}), 9));
        assert_eq!(capture(multiple.clone()).unwrap(), multiple);
    }

    #[test]
    fn inverse_requires_the_same_current_actor_and_never_selects_a_group_member() {
        let argument = [Pattern::Hole(
            "instruction".into(),
            Ty::Named("Instruction".into()),
        )];
        let source = json!({"kind":"Result","args":{"result":3,"phase":"After","index":4}});
        let same = contrast(source.clone(), source.clone());
        let found = unify(&argument, &same, Bindings::new());
        assert!(found.iter().any(|b| {
            b.get("instruction")
                .is_some_and(|v| bare_it(v.pointer(LATER).unwrap()))
        }));
        let different = contrast(
            source,
            json!({"kind":"Result","args":{"result":3,"phase":"After","index":0}}),
        );
        assert_eq!(unify(&argument, &different, Bindings::new()).len(), 1);
        let group = json!({"kind":"Transition","args":{"result":3,"phase":"After"}});
        let batch = contrast(group.clone(), group);
        assert_eq!(unify(&argument, &batch, Bindings::new()).len(), 1);
        let mut deep = Value::Null;
        for _ in 0..130 {
            deep = json!([deep]);
        }
        assert!(capture(deep.clone()).is_err());
        assert!(unify(&argument, &deep, Bindings::new()).is_empty());
    }

    #[test]
    fn repeated_actor_target_is_announced_once_and_never_becomes_recipient_or_condition() {
        let mut instruction = contrast(target(4), it());
        instruction["args"]["otherwise"]["instructions"][0]["args"]["operation"]["args"]["recipients"] =
            json!({"kind":"Objects","args":{"kind":"One","args":target(7)}});
        instruction["args"]["condition"] = json!({"kind":"ObjectMatches","args":{"object":target(12),"filter":{"kind":"CardType","args":"Creature"}}});
        let captured = capture(instruction).unwrap();
        let mut ability = json!({"kind":"Spell","args":{"announcement":{"$sym":"announcement","args":[],"at":0},"body":{"instructions":[captured]}}});
        crate::bind::bind_ability(&mut ability, "Actor proof").unwrap();
        assert_eq!(
            ability["args"]["announcement"]["targets"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let instruction = &ability["args"]["body"]["instructions"][0];
        assert_eq!(instruction.pointer(EARLIER), instruction.pointer(LATER));
        assert_eq!(instruction.pointer(LATER).unwrap()["args"]["slot"], 0);
        assert_eq!(
            instruction["args"]["otherwise"]["instructions"][0]["args"]["operation"]["args"]["recipients"]
                ["args"]["args"]["args"]["slot"],
            1
        );
        assert_eq!(
            instruction["args"]["condition"]["args"]["object"]["args"]["slot"],
            2
        );
    }
    #[test]
    fn copied_fixed_names_bind_at_the_later_word_without_moving_effect_addresses() {
        for name in ["self", "named_source"] {
            let earlier = json!({"$sym":name,"args":[],"at":5});
            let original = contrast(earlier.clone(), it());
            let captured = capture(original.clone()).unwrap();
            assert_eq!(captured.pointer(EARLIER), Some(&earlier));
            assert_eq!(captured.pointer(LATER).unwrap()["at"], 20);
            for branch in ["then", "otherwise"] {
                assert_eq!(
                    captured["args"][branch]["instructions"][0]["args"]["result"],
                    original["args"][branch]["instructions"][0]["args"]["result"]
                );
            }
            let mut ability = json!({"kind":"Spell","args":{"announcement":{"$sym":"announcement","args":[],"at":0},"body":{"instructions":[captured]}}});
            crate::bind::bind_ability(&mut ability, "Actor proof").unwrap();
            let instruction = &ability["args"]["body"]["instructions"][0];
            assert_eq!(
                instruction.pointer(EARLIER),
                Some(&json!({"kind":"Source"}))
            );
            assert_eq!(instruction.pointer(LATER), instruction.pointer(EARLIER));
            assert_eq!(
                instruction["args"]["otherwise"]["instructions"][0]["args"]["result"],
                0
            );
            assert_eq!(
                instruction["args"]["then"]["instructions"][0]["args"]["result"],
                1
            );
        }
        let contextual = it();
        let copied = capture(contrast(
            contextual.clone(),
            json!({"$sym":"member_count_it","args":[{"$sym":"it","args":[],"at":40}],"at":40}),
        ))
        .unwrap();
        assert_eq!(copied.pointer(LATER), Some(&contextual));
        let first_target = target(4);
        let copied = capture(contrast(first_target.clone(), it())).unwrap();
        assert_eq!(copied.pointer(LATER), Some(&first_target));
        let unproved = json!({"$sym":"sacrificed_damage_source","args":[{"$sym":"named_source","args":[],"at":4}],"at":4});
        let copied = capture(contrast(unproved.clone(), it())).unwrap();
        assert_eq!(copied.pointer(LATER), Some(&unproved));
    }

    #[test]
    fn repeated_actor_three_native_frames_retain_every_complete_reviewed_reading() {
        use crate::engine::canonical_abilities;
        let engine = crate::test_support::engine();
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/repeated-damage-actor-native.json"
        ))
        .unwrap();
        for name in [
            "Canopy Surge",
            "Delayed Blast Fireball",
            "The Destined Black Mage",
        ] {
            let case = cases.iter().find(|case| case["name"] == name).unwrap();
            let mut expected: Vec<_> = case["expected_readings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| canonical_abilities(v).unwrap())
                .collect();
            expected.sort_by_key(Value::to_string);
            expected.dedup();
            if name == "Canopy Surge" {
                assert_eq!(expected.len(), 2);
            }
            let mut actual = engine
                .parse(name, case["text"].as_str().unwrap())
                .unwrap_or_else(|why| panic!("{name}: {why:?}"))
                .readings;
            actual.sort_by_key(Value::to_string);
            assert_eq!(
                actual, expected,
                "{name}: complete native/preservation frame"
            );
        }
    }
}
