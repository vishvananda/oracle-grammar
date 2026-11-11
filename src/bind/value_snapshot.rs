//! Allocate synthetic definitions without changing Oracle's lexical backrefs.
use super::{Scope, Step, at_path};
use serde_json::{Value, json};

pub(super) fn marker(
    root: &mut Value,
    scope: &mut Scope,
    path: &[Step],
    name: &str,
) -> Result<bool, String> {
    if name == "paid_sacrifice_definition" {
        let value = at_path(root, path).ok_or("paid definition path")?["args"][0].clone();
        let value = super::sacrificed::snapshot_definition(root, path, scope, value)?;
        *at_path(root, path).ok_or("paid definition path")? = value;
        return Ok(true);
    }
    if !matches!(name, "snapshot_next" | "capture_next") {
        return Ok(false);
    }
    if name == "snapshot_next" && (scope.defined_x || scope.snapshot_payment) {
        return Err("snapshot is outside the initial variable-payment contract".into());
    }
    let key = at_path(root, path).ok_or("snapshot definition path")?["args"][0].clone();
    let id = scope.results.allocate_generated()?;
    let reference_name = if name == "capture_next" {
        "capture_ref"
    } else {
        "snapshot_ref"
    };
    let reference = json!({"$sym":reference_name,"args":[key.clone()],"at":key});
    crate::traverse::replace(root, &reference, &json!({"kind":"BoundValue","args":id}));
    *at_path(root, path).ok_or("snapshot result path")? = json!(id);
    Ok(true)
}

/// Cheap recognition at nodes the existing symbol collector already visits.
pub(super) fn payment_node(value: &Value) -> bool {
    if matches!(value["kind"].as_str(), Some("PayCost" | "PayRepeatedly")) {
        return super::payments::declared(&value["args"]["cost"]);
    }
    matches!(
        value["$sym"].as_str(),
        Some("payment_cost" | "announced_cost")
    ) && super::payments::declared(&value["args"][0])
}

/// Inspect this lexical owner only. Created/granted abilities have their own
/// payment definitions; their binder computes a fresh guard at its own entry.
pub(super) fn variable_payments(root: &Value) -> bool {
    fn visit(value: &Value) -> bool {
        use crate::traverse::{Boundary, boundary};
        if matches!(
            boundary(value),
            Boundary::Ability
                | Boundary::Holder
                | Boundary::Lexical
                | Boundary::Reflexive
                | Boundary::Mode
        ) {
            return false;
        }
        if payment_node(value) {
            return true;
        }
        match value {
            Value::Object(fields) => fields.values().any(visit),
            Value::Array(items) => items.iter().any(visit),
            _ => false,
        }
    }
    match root {
        Value::Object(fields) => fields.values().any(visit),
        Value::Array(items) => items.iter().any(visit),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payment_guard_is_structural_and_stops_at_independent_abilities() {
        let payment = json!({"kind":"PayCost","args":{"cost":{"parts":[{"Mana":["X"]}]}}});
        assert!(variable_payments(
            &json!({"instructions":[payment.clone()]})
        ));
        let announced = json!({"$sym":"announced_cost","args":[{"parts":[{"Mana":["X"]}]},{"kind":"AnnouncedX"}]});
        assert!(variable_payments(&json!({"instructions":[announced]})));
        assert!(!variable_payments(
            &json!({"instructions":[{"kind":"GrantAbility","args":{
                "kind":"Activated","args":{"body":{"instructions":[payment]}}
            }}]})
        ));
    }

    fn collected_guard(mut root: Value) -> bool {
        super::super::holder::mark(&mut root, 0);
        let mut guard = false;
        super::super::references::collect(
            &root,
            &mut Vec::new(),
            &mut Vec::new(),
            super::super::filters::Context::Outside,
            &mut guard,
        );
        assert_eq!(guard, variable_payments(&root));
        guard
    }

    #[test]
    fn collector_keeps_conditional_and_inherited_payments_but_resets_grants() {
        let payment = json!({"kind":"PayCost","args":{"cost":{"parts":[{"Mana":["X"]}]}}});
        let conditional = json!({"$sym":"conditional_ability","at":0,"args":[
            {"kind":"Constant","args":1},{"instructions":[payment.clone()]}
        ]});
        assert!(collected_guard(json!({"instructions":[conditional]})));
        let branch = json!({"$sym":"kicker_branch","at":0,"args":[
            {"parts":[]},{"instructions":[payment.clone()]}
        ]});
        assert!(collected_guard(json!({"instructions":[branch]})));
        let parent = Scope {
            snapshot_payment: collected_guard(json!({"instructions":[payment.clone()]})),
            ..Scope::default()
        };
        let mut child = parent.nested();
        let mut marker_value = json!({"$sym":"snapshot_next","args":[3],"at":3});
        assert!(marker(&mut marker_value, &mut child, &[], "snapshot_next").is_err());
        let granted = json!({"instructions":[{"kind":"GrantAbility","args":{
            "kind":"Activated","args":{"body":{"instructions":[payment]}}
        }}]});
        assert!(!collected_guard(granted));
        let mut fresh = Scope::default();
        assert!(marker(&mut marker_value, &mut fresh, &[], "snapshot_next").unwrap());
        assert_eq!(marker_value, json!(0));
    }

    #[test]
    fn local_and_announced_definitions_reject_before_allocating_results() {
        for announced in [false, true] {
            let mut scope = Scope {
                defined_x: announced,
                snapshot_payment: !announced,
                ..Scope::default()
            };
            let mut root = json!({"$sym":"snapshot_next","args":[3],"at":3});
            assert!(marker(&mut root, &mut scope, &[], "snapshot_next").is_err());
            assert_eq!(scope.results.lexical_issued(), 0);
            assert_eq!(
                serde_json::to_value(scope.results.allocate().unwrap()).unwrap(),
                json!(0)
            );
        }
    }
    #[test]
    fn paid_property_proof_runs_after_children_and_checks_receipt_identity() {
        let property = json!({"kind":"ObjectProperty","args":{"object":{"$sym":"named_object","args":[],"at":30},"property":"Power","information":"CurrentOrLastKnown"}});
        let amounts = json!({"instructions":[
            {"kind":"Effect","args":{"result":{"$sym":"next","args":[],"at":10},"operation":{"kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"AnnouncedX"}}}}},
            {"kind":"Effect","args":{"result":{"$sym":"next","args":[],"at":20},"operation":{"kind":"Mill","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":{"kind":"AnnouncedX"}}}}}
        ]});
        let then = crate::functions::value_snapshot::snapshot(&amounts, &property, 10).unwrap();
        let mut root = json!({"kind":"Spell","args":{"announcement":{"targets":[],"modes":null},"body":{"instructions":[
            {"kind":"Effect","args":{"result":{"$sym":"next","args":[],"at":0},"operation":{"kind":"PayCost","args":{"player":{"kind":"AbilityController"},"cost":{"parts":[{"Sacrifice":{"objects":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"CardType","args":"Creature"}}},"choose":{"Exactly":{"kind":"Constant","args":1}}}}]},"optional":true}}}},
            {"kind":"If","args":{"condition":{"kind":"PaymentChosen","args":{"$sym":"last_result","args":[],"at":1}},"then":then,"otherwise":{"instructions":[]}}}
        ]}}});
        crate::bind::bind_ability(&mut root, "Receipt proof").unwrap();
        let definition = &root["args"]["body"]["instructions"][1]["args"]["then"]["instructions"]
            [0]["args"]["operation"]["args"]["value"];
        assert_eq!(
            definition["args"]["object"]["args"]["objects"]["args"]["payment"],
            json!({"Resolution":0})
        );
        assert_eq!(
            definition["args"]["object"]["args"]["objects"]["args"]["phase"],
            "Before"
        );
        let path = vec![
            Step::Key("args".into()),
            Step::Key("body".into()),
            Step::Key("instructions".into()),
            Step::Index(1),
            Step::Key("args".into()),
            Step::Key("then".into()),
        ];
        let scope = Scope {
            sacrificed_result: Some(super::super::sacrificed::Information::resolution(
                crate::model::ir::ResultId(0),
                0,
                true,
                1,
            )),
            ..Scope::default()
        };
        let wrong = json!({"kind":"ObjectProperty","args":{"object":{"kind":"Source"},"property":"Power","information":"CurrentOrLastKnown"}});
        assert!(
            super::super::sacrificed::snapshot_definition(&root, &path, &scope, wrong).is_err()
        );
        let mut wrong_guard = root.clone();
        wrong_guard["args"]["body"]["instructions"][1]["args"]["condition"]["args"] = json!(7);
        assert!(
            super::super::sacrificed::snapshot_definition(
                &wrong_guard,
                &path,
                &scope,
                definition.clone()
            )
            .is_err()
        );
    }
}
