//! An explicit owner possessive introduces that owner for following player words.
use super::{Scope, Step, at_path, symbol_name};
use crate::model::ir::{ObjectRef, PlayerRef};
use serde_json::{Value, json};

pub(super) fn remember(
    node: &Value,
    scope: &mut Scope,
    path: &[Step],
    introductions: &[(Vec<Step>, usize)],
) -> Result<Value, String> {
    let operation = node["args"]
        .get(0)
        .ok_or("owner return needs an operation")?;
    if operation["kind"] != "Move" || operation["args"]["destination"].get("OwnersHand").is_none() {
        return Err("owner return needs a move to owners' hands".into());
    }
    let mut plain = operation["args"]["objects"].clone();
    let mut operand = path.to_vec();
    operand.extend([
        Step::Key("args".into()),
        Step::Index(0),
        Step::Key("args".into()),
        Step::Key("objects".into()),
    ]);
    // A target's filter may acquire later lexical player references after its
    // introduction. Its recorded path still proves the same slot; its changing
    // serialized specification is not an identity key for this projection.
    for (target, slot) in introductions {
        if let Some(relative) = target.strip_prefix(operand.as_slice()) {
            let value = at_path(&mut plain, relative).ok_or("owner return target projection")?;
            *value = json!(scope.target_base + slot);
        }
    }
    match scalar(&plain, 0)? {
        Some(object) => {
            scope
                .players
                .push(json!(PlayerRef::OwnerOf(Box::new(object))));
            scope.ambiguous_owner = None;
        }
        None => scope.ambiguous_owner = Some(scope.players.len()),
    }
    // Pending target paths and movement annotations belong to their own binders.
    Ok(node.clone())
}

fn scalar(objects: &Value, depth: usize) -> Result<Option<ObjectRef>, String> {
    if depth > 128 {
        return Err("owner return reference depth exceeds128".into());
    }
    match symbol_name(objects) {
        Some("zone_members" | "exile_followup") => {
            return scalar(&objects["args"][0], depth + 1);
        }
        Some(_) => return Ok(None),
        None => {}
    }
    match objects["kind"].as_str() {
        Some("One") => serde_json::from_value(objects["args"].clone())
            .map(Some)
            .map_err(|why| format!("owner return object: {why}")),
        Some("InZone") => scalar(&objects["args"]["objects"], depth + 1),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;

    fn engine() -> &'static Engine {
        crate::test_support::engine()
    }

    #[test]
    fn owner_return_binds_the_returned_target_after_an_own_entry_event() {
        let parsed = engine()
            .parse("Owner Study", "When this creature enters, return target permanent to its owner's hand, then that player discards a card.")
            .unwrap();
        assert_eq!(parsed.readings.len(), 1);
        assert_eq!(
            parsed.readings[0][0]["args"]["body"]["instructions"][1]["args"]["operation"]["args"]["players"],
            json!({"kind":"One","args":{"kind":"OwnerOf","args":{
                "kind":"Target","args":{"slot":0,"index":0}}}})
        );
    }

    #[test]
    fn plural_owners_do_not_select_a_player_but_a_later_target_can() {
        let mut scope = Scope::default();
        scope.players.push(json!({"kind":"AbilityController"}));
        let operation = json!({"kind":"Move","args":{"objects":{"kind":"Targets","args":0},
            "destination":{"OwnersHand":{"counters":[],"states":[]}},"qualifiers":[]}});
        remember(&json!({"args":[operation]}), &mut scope, &[], &[]).unwrap();
        assert!(scope.player().is_none());
        scope
            .players
            .push(json!({"kind":"Target","args":{"slot":1,"index":0}}));
        assert_eq!(scope.player(), scope.players.last().cloned());
    }

    #[test]
    fn object_return_does_not_rebind_explicit_you_or_its_controller() {
        let text = "Return target creature to its owner's hand. You draw a card. Its controller loses 2 life.";
        let parsed = engine().parse("Owner Study", text).unwrap();
        assert_eq!(parsed.readings.len(), 1);
        let items = &parsed.readings[0][0]["args"]["body"]["instructions"];
        assert_eq!(
            items[1]["args"]["operation"]["args"]["players"],
            json!({"kind":"One","args":{"kind":"AbilityController"}})
        );
        assert_eq!(
            items[2]["args"]["operation"]["args"]["players"],
            json!({"kind":"One","args":{"kind":"ControllerOf","args":{
                "kind":"Target","args":{"slot":0,"index":0}}}})
        );
    }

    #[test]
    fn native_owner_returns_keep_late_target_filters_and_linked_card_provenance() {
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/owner-return-player-native.json"
        ))
        .unwrap();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            assert_eq!(
                engine()
                    .parse(name, case["text"].as_str().unwrap())
                    .unwrap_or_else(|why| panic!("{name}: {why:?}"))
                    .readings,
                vec![crate::engine::canonical_abilities(&case["expected"]).unwrap()],
                "{name}"
            );
        }
    }
}
