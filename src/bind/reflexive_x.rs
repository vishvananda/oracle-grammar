//! A sacrifice callback cannot borrow an announced X that this scope never had.
use crate::model::ir::{CostPart, Count, Expr, Operation};
use serde_json::Value;

fn announced(count: &Option<Count>) -> bool {
    matches!(
        count,
        Some(
            Count::Exactly(Expr::AnnouncedX)
                | Count::UpTo(Expr::AnnouncedX)
                | Count::AtLeast(Expr::AnnouncedX)
                | Count::AsManyAsCan(Expr::AnnouncedX)
        )
    )
}

fn sacrifice(operation: &Operation) -> bool {
    match operation {
        Operation::Sacrifice { choose, .. } => announced(choose),
        Operation::PayCost { cost, .. } => crate::model::cost_history::components(cost)
            .into_iter()
            .any(|(_, path)| {
                matches!(crate::model::cost_history::component(cost, &path),
                Some(CostPart::Sacrifice { choose, .. }) if announced(choose))
            }),
        _ => false,
    }
}

fn real_x_payment(operation: &Operation) -> bool {
    matches!(operation, Operation::PayCost { cost, .. }
        if cost.parts.iter().any(|part| matches!(part, CostPart::ManaWithX { x:None, .. })))
}

fn bound_operation(
    raw: &Value,
    scope: &super::Scope,
    introduced: &[super::references::TargetIntroduction],
) -> Result<Option<Operation>, String> {
    crate::functions::branch_bounds(raw)?;
    let mut plain = super::references::bound_reference(raw, introduced, scope.target_base)?;
    super::filters::strip(&mut plain);
    Ok(serde_json::from_value(plain).ok())
}

fn prior_x_payment(
    root: &mut Value,
    path: &[super::Step],
    scope: &super::Scope,
    introduced: &[super::references::TargetIntroduction],
) -> Result<bool, String> {
    for (depth, step) in path.iter().enumerate() {
        let super::Step::Index(index) = step else {
            continue;
        };
        if !matches!(depth.checked_sub(1).and_then(|n| path.get(n)),
            Some(super::Step::Key(k)) if k == "instructions")
        {
            continue;
        }
        let prefix = super::at_path(root, &path[..depth])
            .and_then(|v| v.as_array())
            .ok_or("reflexive X instruction path")?;
        if prefix.len() > 4096 || *index > prefix.len() {
            return Err("reflexive X instruction limit".into());
        }
        for item in &prefix[..*index] {
            if item["kind"] == "Effect"
                && item["args"]["operation"]["kind"] == "PayCost"
                && bound_operation(&item["args"]["operation"], scope, introduced)?
                    .as_ref()
                    .is_some_and(real_x_payment)
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Inspect only the immediate lexical producer of this callback. No whole-card
/// walk or change to operands is needed for an unrelated reflexive trigger.
pub(super) fn check(
    root: &mut Value,
    path: &[super::Step],
    scope: &super::Scope,
    introduced: &[super::references::TargetIntroduction],
) -> Result<(), String> {
    if scope.defined_x || scope.possible_announced_x {
        return Ok(());
    }
    let Some((depth, index)) = path.iter().enumerate().rev().find_map(|(depth, step)| {
        if let super::Step::Index(index) = step
            && matches!(depth.checked_sub(1).and_then(|n| path.get(n)),
                Some(super::Step::Key(k)) if k == "instructions")
        {
            Some((depth, *index))
        } else {
            None
        }
    }) else {
        return Ok(());
    };
    let Some(previous) = index
        .checked_sub(1)
        .and_then(|i| super::at_path(root, &path[..depth])?.get(i))
        .cloned()
    else {
        return Ok(());
    };
    if previous["kind"] != "Effect" {
        return Ok(());
    }
    let raw = &previous["args"]["operation"];
    let Some(operation) = bound_operation(raw, scope, introduced)? else {
        return Ok(());
    };
    if !sacrifice(&operation) {
        return Ok(());
    }
    if real_x_payment(&operation) {
        return Ok(());
    }
    // A real earlier resolving mana choice can supply X. The broad snapshot
    // flag alone is insufficient: the unknown sacrifice count also sets it.
    if prior_x_payment(root, path, scope, introduced)? {
        return Ok(());
    }
    Err("reflexive sacrifice has no announced X in this scope".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(payment: bool) -> Value {
        let count = json!({"Exactly":{"kind":"AnnouncedX"}});
        let objects = json!({"kind":"One","args":{"kind":"Source"}});
        let op = if payment {
            json!({"kind":"PayCost","args":{"player":{"kind":"AbilityController"},
                "cost":{"parts":[{"Sacrifice":{"objects":objects,"choose":count}}]},"optional":false}})
        } else {
            json!({"kind":"Sacrifice","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"objects":objects,"choose":count}})
        };
        json!({"instructions":[{"kind":"Effect","args":{"result":7,"operation":op}},
            {"kind":"Effect","args":{"result":8,"operation":{"kind":"ReflexiveTrigger","args":{"performed":7}}}}]})
    }

    #[test]
    fn ordinary_and_cost_sacrifice_scopes_need_a_real_announced_x() {
        use super::super::{Scope, Step};
        let path = vec![
            Step::Key("instructions".into()),
            Step::Index(1),
            Step::Key("args".into()),
            Step::Key("operation".into()),
        ];
        for payment in [false, true] {
            let mut value = fixture(payment);
            let operation = value["instructions"][1]["args"]["operation"].take();
            value["instructions"][1]["args"]["operation"] =
                json!({"$sym":"reflexive","args":[operation],"at":9});
            assert!(check(&mut value, &path, &Scope::default(), &[]).is_err());
            let scope = Scope {
                defined_x: true,
                ..Scope::default()
            };
            assert!(check(&mut value, &path, &scope, &[]).is_ok());
            value["instructions"][1]["args"]["operation"]["args"][0]["args"]["performed"] =
                json!(6);
            assert!(check(&mut value, &path, &Scope::default(), &[]).is_err());
        }
    }

    #[test]
    fn sacrifice_defined_x_scope_uses_the_performed_count_for_later_targets() {
        let case: Vec<Value> =
            serde_json::from_str(include_str!("../../tests/fixtures/shared-clauses-17.json"))
                .unwrap();
        let case = &case[0];
        let engine = crate::test_support::engine();
        let actual = engine
            .parse(
                case["name"].as_str().unwrap(),
                case["text"].as_str().unwrap(),
            )
            .unwrap();
        assert_eq!(actual.readings.len(), 1);
        assert_eq!(
            actual.readings[0],
            crate::engine::canonical_abilities(&case["expected"]).unwrap()
        );
    }

    #[test]
    fn resolving_mana_x_scope_is_proved_by_a_real_earlier_payment() {
        use super::super::{Scope, Step};
        let mut value = fixture(false);
        let op = value["instructions"][1]["args"]["operation"].take();
        value["instructions"][1]["args"]["operation"] =
            json!({"$sym":"reflexive","args":[op],"at":9});
        let scope = Scope {
            snapshot_payment: true,
            ..Scope::default()
        };
        let path = vec![
            Step::Key("instructions".into()),
            Step::Index(1),
            Step::Key("args".into()),
            Step::Key("operation".into()),
        ];
        assert!(check(&mut value, &path, &scope, &[]).is_err());
        let payment = json!({"kind":"Effect","args":{"result":2,"operation":{"kind":"PayCost","args":{
            "player":{"kind":"AbilityController"},"cost":{"parts":[{"ManaWithX":{"mana":["X"],"x":null}}]},"optional":true
        }}}});
        value["instructions"]
            .as_array_mut()
            .unwrap()
            .insert(0, payment);
        let mut path = path;
        path[1] = Step::Index(2);
        assert!(check(&mut value, &path, &scope, &[]).is_ok());
    }

    #[test]
    fn choice_mark_scope_does_not_shadow_an_existing_x_contract() {
        use super::super::{Scope, addresses};
        for mut scope in [
            Scope {
                defined_x: true,
                ..Scope::default()
            },
            Scope {
                possible_announced_x: true,
                ..Scope::default()
            },
            Scope {
                snapshot_payment: true,
                ..Scope::default()
            },
        ] {
            assert!(addresses::result("choice_mark", &json!({}), &mut scope).is_err());
            assert!(scope.marked.is_none());
        }
        let mut scope = Scope::default();
        assert_eq!(
            addresses::result("choice_mark", &json!({}), &mut scope).unwrap(),
            json!(0)
        );
        assert!(scope.marked.is_some());
    }
}
