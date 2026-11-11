//! Plural object references keep the current target slot or matched event batch.
use super::{ObjectKind, Scope, Value, contains, json};

pub(in crate::bind) fn reference(node: &Value, scope: &Scope) -> Result<Value, String> {
    let args = node["args"]
        .as_array()
        .filter(|args| args.len() == 2)
        .ok_or("group reference needs a kind and noun filter")?;
    let kind: ObjectKind = serde_json::from_value(args[0].clone()).map_err(|e| e.to_string())?;
    let focus = scope.preceding().ok_or("no preceding object group")?;
    if focus["kind"] == "EventSubject"
        && matches!(
            scope.event_quantity,
            Some(super::super::events::Quantity::Objects)
        )
        && kind == ObjectKind::Any
        && args[1]["kind"] == "Any"
    {
        return Ok(json!({"kind":"EventGroup"}));
    }
    let slot = focus["args"]["slot"]
        .as_u64()
        .and_then(|slot| usize::try_from(slot).ok())
        .filter(|slot| {
            focus["kind"] == "Target"
                && focus["args"]["index"] == 0
                && *slot >= scope.target_base
                && *slot - scope.target_base < scope.targets.len()
        })
        .ok_or("the current focus is not a local object target group")?;
    let entry = scope
        .references
        .iter()
        .find(|entry| entry.reference == focus)
        .ok_or("the current target supplies no object group")?;
    if entry.single {
        return Err("a singular object target does not introduce a plural group".into());
    }
    let noun = &args[1];
    let proven = (kind == ObjectKind::Any || entry.kinds.contains(&kind))
        && match noun["kind"].as_str() {
            Some("Any") => true,
            Some("CardType") => serde_json::from_value(noun["args"].clone())
                .is_ok_and(|kind| contains(&entry.types, &kind)),
            Some("Subtype") => serde_json::from_value(noun["args"].clone())
                .is_ok_and(|kind| contains(&entry.subtypes, &kind)),
            _ => false,
        };
    if !proven {
        return Err("the current object group does not prove this noun".into());
    }
    Ok(json!({"kind":"Targets","args":slot}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(kind: &str, filter: Value) -> Value {
        json!({"args":[kind,filter]})
    }

    fn focused_target(filter: Value) -> Scope {
        let mut scope = Scope::default();
        let focus = json!({"kind":"Target","args":{"slot":1,"index":0}});
        super::super::target(
            &mut scope,
            focus.clone(),
            &json!({"kind":{"Object":{"kind":"Permanent"}},"filter":filter,
                "count":{"UpTo":{"kind":"Constant","args":3}}}),
        )
        .unwrap();
        scope.targets = vec![Value::Null, Value::Null];
        scope.objects.push(focus.clone());
        scope.named_object = Some(focus);
        scope
    }

    #[test]
    fn full_slot_preserves_type_and_does_not_select_index_zero() {
        let creature = json!({"kind":"CardType","args":"Creature"});
        let scope = focused_target(creature.clone());
        assert_eq!(
            reference(&node("Permanent", creature), &scope).unwrap(),
            json!({"kind":"Targets","args":1})
        );
        assert!(
            reference(
                &node("Permanent", json!({"kind":"CardType","args":"Artifact"})),
                &scope
            )
            .is_err()
        );
    }

    #[test]
    fn mixed_types_and_inherited_slots_do_not_prove_a_creature_group() {
        let creature = json!({"kind":"CardType","args":"Creature"});
        let mut scope = focused_target(json!({"kind":"AnyOf","args":[
            creature,{"kind":"CardType","args":"Artifact"}
        ]}));
        assert!(reference(&node("Permanent", creature), &scope).is_err());
        assert!(reference(&node("Any", json!({"kind":"Any"})), &scope).is_ok());
        scope.targets.clear();
        assert!(reference(&node("Any", json!({"kind":"Any"})), &scope).is_err());
    }

    #[test]
    fn later_creation_and_unknown_focus_never_fall_back_to_an_old_slot() {
        let mut scope = focused_target(json!({"kind":"CardType","args":"Creature"}));
        let node = node("Any", json!({"kind":"Any"}));
        for focus in [
            json!({"kind":"Result","args":{"result":4,"phase":"After","index":0}}),
            json!({"kind":"Source"}),
        ] {
            scope.named_object = Some(focus);
            assert!(reference(&node, &scope).is_err());
        }
    }

    #[test]
    fn only_a_current_object_batch_supplies_an_event_group() {
        let mut scope = Scope {
            named_object: Some(json!({"kind":"EventSubject"})),
            event_quantity: Some(super::super::super::events::Quantity::Objects),
            ..Scope::default()
        };
        let node = node("Any", json!({"kind":"Any"}));
        assert_eq!(
            reference(&node, &scope).unwrap(),
            json!({"kind":"EventGroup"})
        );
        scope.event_quantity = Some(super::super::super::events::Quantity::Amount);
        assert!(reference(&node, &scope).is_err());
        scope.event_quantity = None;
        assert!(reference(&node, &scope).is_err());
    }
}
