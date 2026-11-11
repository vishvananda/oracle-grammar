//! A leading bare literal possessive stat names the written damage actor.
use super::{Found, Scope, Step, references::TargetIntroduction, symbol_name};
use serde_json::Value;

impl Scope {
    pub(super) fn damage_pronoun(
        &self,
        name: &str,
        root: &Value,
        item: &Found,
        introduced: &[TargetIntroduction],
        prior_targets: &super::references::Contexts,
    ) -> Result<Value, String> {
        if name == "it"
            && let Some(actor) = reference(root, &item.path, introduced, self.target_base)?
        {
            return Ok(actor);
        }
        if name == "it"
            && let Some(actor) = super::damage_relative::possessive_amount(root, &item.path)?
        {
            return Ok(actor);
        }
        prior_targets.before(&item.path, self).object_pronoun(
            matches!(item.path.get(1), Some(Step::Key(key)) if key == "intervening_if"),
        )
    }
}

pub(super) fn reference(
    root: &Value,
    path: &[Step],
    introduced: &[TargetIntroduction],
    target_base: usize,
) -> Result<Option<Value>, String> {
    // Exact wrappers retain word provenance. Shape alone cannot distinguish
    // literal its power from the power of it or that creature's power.
    let [
        annotation_path @ ..,
        Step::Key(a),
        Step::Index(0),
        Step::Key(b),
        Step::Index(0),
        Step::Key(c),
        Step::Key(object),
    ] = path
    else {
        return Ok(None);
    };
    if a != "args" || b != "args" || c != "args" || object != "object" {
        return Ok(None);
    }
    let [operation_path @ .., Step::Key(args), Step::Key(amount)] = annotation_path else {
        return Ok(None);
    };
    if args != "args" || amount != "amount" {
        return Ok(None);
    }
    let Some(operation) = at(root, operation_path) else {
        return Ok(None);
    };
    if operation["kind"] != "DealDamage" {
        return Ok(None);
    }
    let amount = &operation["args"]["amount"];
    if symbol_name(amount) != Some("damage_actor_value") {
        return Ok(None);
    }
    let literal = &amount["args"][0];
    if symbol_name(literal) != Some("its_stat") {
        return Ok(None);
    }
    let property = &literal["args"][0];
    if property["kind"] != "ObjectProperty"
        || !matches!(
            property["args"]["property"].as_str(),
            Some("Power" | "Toughness")
        )
        || symbol_name(&property["args"]["object"]) != Some("it")
    {
        return Ok(None);
    }
    // Read the actual bound operand, never a duplicated target introduction.
    let mut actor =
        super::references::bound_reference(&operation["args"]["source"], introduced, target_base)?;
    super::filters::strip(&mut actor);
    let typed = serde_json::from_value::<crate::model::ir::ObjectRef>(actor.clone())
        .map_err(|why| format!("damage actor must already be bound: {why}"))?;
    // A moved object's current incarnation and the announced target's LKI
    // require a separate contract. Keep legacy binding for result actors.
    if matches!(typed, crate::model::ir::ObjectRef::Result { .. })
        && !super::move_after::actor(root, operation_path, introduced, target_base, &actor)?
    {
        return Ok(None);
    }
    // This bound belongs only to the new, proven annotation intersection.
    // Unrelated legacy pronouns keep their original refusal behavior.
    if path.len() > 128 {
        return Err("damage actor reference depth exceeds128".into());
    }
    Ok(Some(actor))
}

fn at<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(k) => value.get(k)?,
            Step::Index(i) => value.get(*i)?,
        };
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input(source: Value, amount: Value) -> (Value, Vec<Step>) {
        fn find(value: &Value, path: &mut Vec<Step>) -> Option<Vec<Step>> {
            if symbol_name(value) == Some("it") {
                return Some(path.clone());
            }
            match value {
                Value::Object(fields) => {
                    for (key, child) in fields {
                        path.push(Step::Key(key.clone()));
                        if let Some(found) = find(child, path) {
                            return Some(found);
                        }
                        path.pop();
                    }
                }
                Value::Array(values) => {
                    for (index, child) in values.iter().enumerate() {
                        path.push(Step::Index(index));
                        if let Some(found) = find(child, path) {
                            return Some(found);
                        }
                        path.pop();
                    }
                }
                _ => {}
            }
            None
        }
        let root = json!({"kind":"DealDamage","args":{"source":source,"amount":amount}});
        let path = find(
            &root["args"]["amount"],
            &mut vec![Step::Key("args".into()), Step::Key("amount".into())],
        )
        .unwrap();
        (root, path)
    }
    fn property() -> Value {
        json!({"kind":"ObjectProperty","args":{"object":{"$sym":"it","args":[]},
            "property":"Power","information":"CurrentOrLastKnown"}})
    }
    fn literal(value: Value) -> Value {
        json!({"$sym":"its_stat","args":[value]})
    }
    fn leading(value: Value) -> Value {
        json!({"$sym":"damage_actor_value","args":[value]})
    }

    #[test]
    fn actual_source_holder_granter_and_shifted_target_are_preserved() {
        for actor in [
            json!({"kind":"Source"}),
            json!({"kind":"GrantingObject"}),
            json!({"kind":"EventSubject"}),
            json!({"kind":"Target","args":{"slot":1,"index":0}}),
        ] {
            let (root, path) = input(actor.clone(), leading(literal(property())));
            assert_eq!(reference(&root, &path, &[], 0).unwrap(), Some(actor));
        }
    }

    #[test]
    fn movement_result_actors_keep_existing_pronoun_binding() {
        for phase in ["Before", "After"] {
            let actor = json!({"kind":"Result","args":{
                "result":0,"phase":phase,"index":0}});
            let (root, path) = input(actor, leading(literal(property())));
            assert_eq!(reference(&root, &path, &[], 0).unwrap(), None);
        }
    }

    #[test]
    fn pending_source_target_uses_its_assigned_shifted_address_once() {
        let spec = json!({"slot":{"$sym":"slot","args":[]},"filter":{"kind":"Any"}});
        let source = json!({"kind":"Target","args":{"slot":{"$sym":"target","at":4,"args":[spec]},"index":0}});
        let introduced = vec![((4, spec.to_string()), 0)];
        let (root, path) = input(source, leading(literal(property())));
        assert_eq!(
            reference(&root, &path, &introduced, 1).unwrap(),
            Some(json!({"kind":"Target","args":{"slot":1,"index":0}}))
        );
    }

    #[test]
    fn exact_word_and_scalar_scope_are_required() {
        for amount in [
            leading(property()),
            literal(property()),
            leading(
                json!({"kind":"Add","args":[literal(property()),{"kind":"Constant","args":1}]}),
            ),
            leading(json!({"kind":"CountObjects","args":{"filter":literal(property())}})),
        ] {
            let (root, path) = input(json!({"kind":"Source"}), amount);
            assert_eq!(reference(&root, &path, &[], 0).unwrap(), None);
        }
    }

    #[test]
    fn unresolved_actor_is_refused_instead_of_defaulting_to_source() {
        let (root, path) = input(
            json!({"$sym":"self","args":[]}),
            leading(literal(property())),
        );
        assert!(
            reference(&root, &path, &[], 0)
                .unwrap_err()
                .contains("already be bound")
        );
    }
    fn deep(mut root: Value, mut path: Vec<Step>) -> (Value, Vec<Step>) {
        for _ in 0..130 {
            root = json!({"wrapper":root});
            path.insert(0, Step::Key("wrapper".into()));
        }
        assert!(path.len() > 128);
        (root, path)
    }

    #[test]
    fn deep_unrelated_pronoun_does_not_gain_an_actor_depth_refusal() {
        let path = vec![Step::Key("unrelated".into()); 129];
        assert_eq!(reference(&Value::Null, &path, &[], 0).unwrap(), None);
    }

    #[test]
    fn deep_non_damage_composite_with_the_same_suffix_is_unchanged() {
        let amount =
            json!({"kind":"Add","args":[literal(property()),{"kind":"Constant","args":1}]});
        let (mut root, path) = input(json!({"kind":"Source"}), amount);
        root["kind"] = json!("GainLife");
        let (root, path) = deep(root, path);
        assert_eq!(reference(&root, &path, &[], 0).unwrap(), None);
    }

    #[test]
    fn depth_bound_still_applies_to_the_exact_actor_annotation_intersection() {
        let (root, path) = input(json!({"kind":"Source"}), leading(literal(property())));
        let (root, path) = deep(root, path);
        assert!(
            reference(&root, &path, &[], 0)
                .unwrap_err()
                .contains("depth exceeds128")
        );
    }
}
