//! Result focus for single object choices and whole token batches.
use super::{Scope, Step, at_path, objects, qualified, selections, transitions};
use crate::model::ir::{Count, Expr};
use serde_json::{Value, json};

pub(super) fn bind_marker(
    root: &mut Value,
    scope: &mut Scope,
    path: &[Step],
    name: &str,
) -> Result<bool, String> {
    if !matches!(
        name,
        "manifest_result" | "counter_choice" | "created_result" | "selected_result"
    ) {
        return Ok(false);
    }
    let result = transitions::effect_result(root, path)
        .map_err(|why| format!("@{name}: {why}; path {path:?}"))?;
    let node = at_path(root, path).ok_or("result marker path")?;
    match name {
        "manifest_result" => objects::manifest_result(node, scope, result)?,
        "counter_choice" => *node = objects::counter_choice(node, scope, result)?,
        "selected_result" => mark_selected(node, scope, result)?,
        _ => mark(node, scope, result)?,
    }
    Ok(true)
}

fn mark(node: &Value, scope: &mut Scope, result: u64) -> Result<(), String> {
    let operation = node["args"]
        .get(0)
        .ok_or("token result needs its operation")?;
    let operation = selections::action(operation);
    let tokens = operation["args"]["tokens"]
        .as_array()
        .ok_or("token batch")?;
    if operation["kind"] != "CreateTokens" {
        return Err("token result needs CreateTokens".into());
    }
    let single = operation["args"]["players"]["kind"] == "One"
        && tokens.len() == 1
        && tokens[0]["amount"]["kind"] == "Constant"
        && tokens[0]["amount"]["args"].as_u64().is_some_and(|n| n <= 1);
    qualified::created(scope, operation, result, single)?;
    scope.created_result = Some((result, single));
    let reference = scalar(result);
    scope.objects.push(reference.clone());
    scope.named_object = Some(reference);
    scope.others.clear();
    scope.pronoun = None;
    Ok(())
}

/// A singular choice supersedes the event subject as the following pronoun.
/// Never silently select index zero from a potentially plural result.
fn mark_selected(node: &Value, scope: &mut Scope, result: u64) -> Result<(), String> {
    let operation = node["args"]
        .get(0)
        .ok_or("selected result needs its operation")?;
    let operation = selections::action(operation);
    if operation["kind"] != "ChooseObjects" {
        return Err("selected result needs ChooseObjects".into());
    }
    // Object filters may still contain binding markers at this point. Only the
    // already evaluated count is needed to prove the result is singular.
    let count: Count = serde_json::from_value(operation["args"]["count"].clone())
        .map_err(|why| format!("selected result count: {why}"))?;
    if !matches!(
        count,
        Count::Exactly(Expr::Constant(0..=1))
            | Count::UpTo(Expr::Constant(0..=1))
            | Count::AsManyAsCan(Expr::Constant(0..=1))
    ) {
        return Err("selected result needs at most one chosen object".into());
    }
    let reference = scalar(result);
    scope.objects.push(reference.clone());
    scope.named_object = Some(reference);
    scope.others.clear();
    scope.pronoun = None;
    Ok(())
}

fn scalar(result: u64) -> Value {
    json!({"kind":"Result","args":{"result":result,"phase":"After","index":0}})
}

pub(super) fn reference(scope: &Scope, singular: bool, explicit: bool) -> Result<Value, String> {
    let (result, single) = scope.created_result.ok_or("no preceding token batch")?;
    let focus = if singular {
        scope.object()
    } else {
        scope.preceding()
    };
    if !explicit && focus != Some(scalar(result)) {
        return Err("the token batch is not the current referent".into());
    }
    if singular && !single {
        return Err("a plural token batch is not a singular referent".into());
    }
    Ok(json!({"kind":"Created","args":result}))
}

/// Scalar roles can name one token, but recipients and characteristic totals
/// apply to the produced batch even when one token was requested.
/// A One(Result(..., index=0)) recipient would select only one created object.
pub(super) fn check_scalar(
    root: &mut Value,
    path: &[Step],
    value: &Value,
    scope: &Scope,
) -> Result<(), String> {
    let Some((result, single)) = scope.created_result else {
        return Ok(());
    };
    if *value != scalar(result) {
        return Ok(());
    }
    if !single {
        return Err("a plural token batch is not one object".into());
    }
    for depth in 0..path.len() {
        if at_path(root, &path[..depth]).is_some_and(|parent| parent["kind"] == "Attach")
            && matches!(path.get(depth + 1), Some(Step::Key(key)) if key == "host")
        {
            return Err("a token attachment host needs a produced-object choice".into());
        }
    }
    for depth in (0..path.len()).rev() {
        let parent = at_path(root, &path[..depth]).ok_or("token reference path")?;
        if let Some(kind) = parent["kind"].as_str() {
            return match kind {
                "One" => Err("a token recipient names the created batch".into()),
                "ObjectProperty" => Err("a token characteristic measures the created batch".into()),
                _ => Ok(()),
            };
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(count: Value) -> Value {
        json!({"symbol":"selected_result", "args":[{
            "kind":"ChooseObjects", "args":{
                "player":{"kind":"AbilityController"},
                "objects":{"kind":"One","args":{"kind":"Source"}},
                "count":count
            }
        }]})
    }

    #[test]
    fn singular_choices_replace_event_focus_even_when_optional() {
        for count in [
            json!({"Exactly":{"kind":"Constant","args":1}}),
            json!({"AsManyAsCan":{"kind":"Constant","args":1}}),
            json!({"UpTo":{"kind":"Constant","args":1}}),
        ] {
            let mut scope = Scope::default();
            scope.objects.push(json!({"kind":"Source"}));
            scope.pronoun = Some(json!({"kind":"EventSubject"}));
            mark_selected(&choice(count), &mut scope, 7).unwrap();
            assert_eq!(scope.object_pronoun(false).unwrap(), scalar(7));
            assert_eq!(scope.named_object, Some(scalar(7)));
            assert_eq!(scope.created_result, None);
        }
    }

    #[test]
    fn plural_or_unbounded_choices_cannot_supply_a_singular_result() {
        for count in [
            json!("AnyNumber"),
            json!({"Exactly":{"kind":"Constant","args":2}}),
            json!({"UpTo":{"kind":"Constant","args":2}}),
            json!({"AtLeast":{"kind":"Constant","args":1}}),
        ] {
            assert!(mark_selected(&choice(count), &mut Scope::default(), 7).is_err());
        }
        let wrong = json!({"args":[{"kind":"Draw", "args":{
            "players":{"kind":"One","args":{"kind":"AbilityController"}},
            "amount":{"kind":"Constant","args":1}
        }}]});
        assert!(mark_selected(&wrong, &mut Scope::default(), 7).is_err());
    }
}
