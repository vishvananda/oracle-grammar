//! Property anaphora after a positively chosen resolution sacrifice.
use super::{Scope, Step, sacrificed::Information};
use crate::model::ir::{Condition, CostPart, Count, Expr, ResultId};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
enum SacrificeProbe {
    Sacrifice { choose: Option<Count> },
}

fn get<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get(key),
            Step::Index(index) => value.get(*index),
        }?;
    }
    Some(value)
}

/// Called only after a native result address has actually been allocated.
/// The cost's object filters may still contain symbols; cardinality and all
/// other component kinds are checked independently without guessing objects.
pub(super) fn record(
    root: &Value,
    path: &[Step],
    value: &Value,
    scope: &mut Scope,
) -> Result<(), String> {
    if !matches!(path.last(), Some(Step::Key(key)) if key == "result") || path.len() < 2 {
        return Ok(());
    }
    let Some(effect) = get(root, &path[..path.len() - 2]) else {
        return Ok(());
    };
    let operation = &effect["args"]["operation"];
    if effect["kind"] != "Effect"
        || operation["kind"] != "PayCost"
        || operation["args"]["optional"] != true
    {
        return Ok(());
    }
    let Some(parts) = operation["args"]["cost"]["parts"].as_array() else {
        return Ok(());
    };
    let mut sacrifice = None;
    for (index, part) in parts.iter().enumerate() {
        if let Ok(SacrificeProbe::Sacrifice { choose }) = serde_json::from_value(part.clone()) {
            if sacrifice.is_some() {
                forget_resolution(scope);
                return Ok(());
            }
            // An explicitly named object already carries its own identity;
            // only a cost selection establishes this new information subject.
            if !matches!(choose, Some(Count::Exactly(Expr::Constant(1)))) {
                forget_resolution(scope);
                return Ok(());
            }
            sacrifice = Some(index);
        } else if !matches!(
            serde_json::from_value::<CostPart>(part.clone()),
            Ok(CostPart::Mana(_) | CostPart::Life(_) | CostPart::TapSymbol)
        ) {
            forget_resolution(scope);
            return Ok(());
        }
    }
    if let Some(component) = sacrifice {
        let result: ResultId = serde_json::from_value(value.clone())
            .map_err(|why| format!("resolution sacrifice result: {why}"))?;
        let component = u32::try_from(component)
            .map_err(|_| "resolution sacrifice component address exhausted")?;
        scope.sacrificed_result = Some(Information::resolution(
            result,
            component,
            true,
            scope.objects.len(),
        ));
    }
    Ok(())
}

/// An optional payment introduces its inputs only in the exact positive
/// PaymentChosen branch, never its condition, else-branch or a future scope.
pub(super) fn paid_guard(root: &Value, path: &[Step], result: u32) -> bool {
    let mut parent = root;
    let mut paid = false;
    for (depth, step) in path.iter().enumerate() {
        if paid && independent(parent) {
            return false;
        }
        if parent["kind"] == "If"
            && matches!(path.get(depth), Some(Step::Key(key)) if key == "args")
            && matches!(path.get(depth + 1), Some(Step::Key(key)) if key == "then")
            && matches!(serde_json::from_value::<Condition>(parent["args"]["condition"].clone()),
                Ok(Condition::PaymentChosen(id)) if id.0 == result)
        {
            paid = true;
        }
        let Some(next) = get(parent, std::slice::from_ref(step)) else {
            return false;
        };
        parent = next;
    }
    paid
}

fn independent(value: &Value) -> bool {
    use crate::traverse::{Boundary, boundary};
    matches!(
        boundary(value),
        Boundary::Ability
            | Boundary::Holder
            | Boundary::Lexical
            | Boundary::Reflexive
            | Boundary::Mode
    ) || matches!(
        value["kind"].as_str(),
        Some(
            "CreateDelayedTrigger"
                | "CreateTrigger"
                | "CreateReplacement"
                | "ReflexiveTrigger"
                | "GrantAbility"
        )
    )
}

fn forget_resolution(scope: &mut Scope) {
    if scope
        .sacrificed_result
        .as_ref()
        .is_some_and(Information::is_resolution)
    {
        scope.sacrificed_result = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(count: i64) -> (Value, Vec<Step>, Vec<Step>, Scope) {
        let property = json!({"kind":"ObjectProperty","args":{"object":{"kind":"Source"},"property":"Power","information":"CurrentOrLastKnown"}});
        let root = json!({"kind":"Spell","args":{"body":{"instructions":[
            {"kind":"Effect","args":{"result":17,"operation":{"kind":"PayCost","args":{"player":{"kind":"AbilityController"},"optional":true,"cost":{"parts":[{"Mana":[{"Generic":2}]},{"Sacrifice":{"objects":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"CardType","args":"Creature"}}},"choose":{"Exactly":{"kind":"Constant","args":count}}}}]}}}}},
            {"kind":"If","args":{"condition":{"kind":"PaymentChosen","args":17},"then":{"instructions":[{"kind":"Effect","args":{"result":18,"operation":{"kind":"Draw","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"amount":property}}}}]},"otherwise":{"instructions":[]}}}
        ]}}});
        let keys = |items: &[&str]| {
            items
                .iter()
                .map(|key| Step::Key((*key).into()))
                .collect::<Vec<_>>()
        };
        let mut effect = keys(&["args", "body", "instructions"]);
        effect.push(Step::Index(0));
        effect.extend(keys(&["args", "result"]));
        let mut property = keys(&["args", "body", "instructions"]);
        property.push(Step::Index(1));
        property.extend(keys(&["args", "then", "instructions"]));
        property.push(Step::Index(0));
        property.extend(keys(&[
            "args",
            "operation",
            "args",
            "amount",
            "args",
            "object",
        ]));
        let mut scope = Scope::default();
        scope.objects.push(json!({"kind":"Source"}));
        (root, effect, property, scope)
    }

    #[test]
    fn actual_result_and_component_address_pre_action_inputs() {
        let (root, effect, property, mut scope) = fixture(1);
        record(&root, &effect, &json!(17), &mut scope).unwrap();
        let reference = super::super::sacrificed::reference(
            "defined_named_object",
            json!({"kind":"Source"}),
            &property,
            &root,
            &mut scope,
        )
        .unwrap();
        assert_eq!(
            reference,
            json!({"kind":"Query","args":{"objects":{"kind":"CostObjects","args":{"payment":{"Resolution":17},"component":[1],"view":"Inputs","phase":"Before"}},"index":0}})
        );
        assert_eq!(
            super::super::sacrificed::reference(
                "self",
                json!({"kind":"Source"}),
                &property,
                &root,
                &mut scope
            )
            .unwrap(),
            json!({"kind":"Source"})
        );
    }

    #[test]
    fn exact_positive_guard_and_scalar_cardinality_are_required() {
        let (mut root, effect, property, mut scope) = fixture(1);
        record(&root, &effect, &json!(17), &mut scope).unwrap();
        root["args"]["body"]["instructions"][1]["args"]["condition"]["args"] = json!(9);
        assert_eq!(
            super::super::sacrificed::reference(
                "defined_named_object",
                json!({"kind":"Source"}),
                &property,
                &root,
                &mut scope
            )
            .unwrap(),
            json!({"kind":"Source"})
        );
        let (root, effect, _, mut scope) = fixture(2);
        record(&root, &effect, &json!(17), &mut scope).unwrap();
        assert!(scope.sacrificed_result.is_none());
    }

    #[test]
    fn alternatives_and_explicit_source_costs_do_not_invent_selection_receipts() {
        let (mut root, effect, _, mut scope) = fixture(1);
        root["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"] =
            json!([{"Choice":[{"parts":[]},{"parts":[]}]}]);
        record(&root, &effect, &json!(17), &mut scope).unwrap();
        assert!(scope.sacrificed_result.is_none());
        root["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"] = json!([{"Sacrifice":{"objects":{"kind":"One","args":{"kind":"Source"}},"choose":null}}]);
        record(&root, &effect, &json!(17), &mut scope).unwrap();
        assert!(scope.sacrificed_result.is_none());
    }
    #[test]
    fn outer_payment_guard_never_crosses_a_future_or_granted_scope() {
        for kind in [
            "CreateDelayedTrigger",
            "CreateReplacement",
            "ReflexiveTrigger",
            "CreateTrigger",
            "Activated",
        ] {
            let (mut root, _, property, _) = fixture(1);
            let instruction =
                root["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0].take();
            root["args"]["body"]["instructions"][1]["args"]["then"]["instructions"][0] = json!({"kind":"Effect","args":{"result":19,"operation":{"kind":kind,"args":{"body":{"instructions":[instruction]}}}}});
            let mut nested = property[..10].to_vec();
            nested.extend([
                Step::Key("args".into()),
                Step::Key("body".into()),
                Step::Key("instructions".into()),
                Step::Index(0),
                Step::Key("args".into()),
                Step::Key("operation".into()),
            ]);
            nested.extend_from_slice(&property[10..]);
            assert!(!paid_guard(&root, &nested, 17), "{kind}");
        }
        assert!(independent(
            &json!({"weight":1,"announcement":{},"body":{}})
        ));
    }

    #[test]
    fn later_payment_cannot_borrow_the_previous_positive_guard() {
        let (mut root, effect, property, mut scope) = fixture(1);
        record(&root, &effect, &json!(17), &mut scope).unwrap();
        let mut child = scope.nested();
        record(&root, &effect, &json!(23), &mut child).unwrap();
        let source = json!({"kind":"Source"});
        assert_eq!(
            super::super::sacrificed::reference(
                "named_object",
                source.clone(),
                &property,
                &root,
                &mut child
            )
            .unwrap(),
            source
        );
        let outer = super::super::sacrificed::reference(
            "named_object",
            source.clone(),
            &property,
            &root,
            &mut scope,
        )
        .unwrap();
        assert_eq!(
            outer["args"]["objects"]["args"]["payment"],
            json!({"Resolution":17})
        );
        root["args"]["body"]["instructions"][1]["args"]["condition"]["args"] = json!(23);
        let inner = super::super::sacrificed::reference(
            "named_object",
            source,
            &property,
            &root,
            &mut child,
        )
        .unwrap();
        assert_eq!(
            inner["args"]["objects"]["args"]["payment"],
            json!({"Resolution":23})
        );
        root["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["cost"]["parts"] =
            json!([{"Choice":[{"parts":[]},{"parts":[]}]}]);
        record(&root, &effect, &json!(24), &mut child).unwrap();
        assert!(child.sacrificed_result.is_none());
    }
}
