//! A draft introduces cards in hand, separately from token creation or its holder.
use super::{Scope, Step, at_path, qualified, selections, transitions};
use crate::model::ir::Expr;
use serde_json::{Value, json};

fn scalar(result: u64) -> Value {
    json!({"kind":"Result","args":{"result":result,"phase":"After","index":0}})
}

pub(super) fn marker(
    root: &mut Value,
    scope: &mut Scope,
    path: &[Step],
    name: &str,
) -> Result<bool, String> {
    if name != "drafted_result" {
        return Ok(false);
    }
    let result = transitions::effect_result(root, path)?;
    let node = at_path(root, path).ok_or("draft result marker path")?;
    let wrapped = node["args"]
        .get(0)
        .ok_or("draft result needs its operation")?;
    let operation = selections::action(wrapped);
    if operation["kind"] != "DraftFromSpellbook" {
        return Err("draft result needs DraftFromSpellbook".into());
    }
    let count: Expr = serde_json::from_value(operation["args"]["times"].clone())
        .map_err(|why| format!("draft count: {why}"))?;
    let single = operation["args"]["actor"]["kind"] == "One" && matches!(count, Expr::Constant(1));
    qualified::drafted(scope, result, single)?;
    scope.drafted_result = Some((result, single));
    let reference = scalar(result);
    scope.objects.push(reference.clone());
    scope.named_object = Some(reference);
    scope.others.clear();
    scope.pronoun = None;
    // Preserve deferred target paths until finish_scope strips the marker.
    Ok(true)
}

/// This narrow priority prevents token metadata from returning an older card
/// after a new draft. Later targets and other focuses still take precedence.
pub(super) fn card(scope: &Scope) -> Option<Result<Value, String>> {
    let (result, single) = scope.drafted_result?;
    (scope.preceding() == Some(scalar(result))).then(|| {
        if single {
            Ok(scalar(result))
        } else {
            Err("a plural draft does not establish one card".into())
        }
    })
}

pub(super) fn check_scalar(
    root: &Value,
    path: &[Step],
    name: &str,
    reference: &Value,
    scope: &Scope,
) -> Result<(), String> {
    let Some((result, single)) = scope.drafted_result else {
        return Ok(());
    };
    if *reference != scalar(result) {
        return Ok(());
    }
    if !single {
        return Err("a plural draft does not establish one card".into());
    }
    current(root, path, result, name == "current_named_object")
}

/// Draft outputs are cards in hand. The native result's after-transition is the
/// complete batch, not a token-created set or an arbitrary first member.
pub(super) fn reference(
    root: &Value,
    path: &[Step],
    scope: &Scope,
    explicit: bool,
) -> Result<Value, String> {
    let (result, _) = scope.drafted_result.ok_or("no preceding draft")?;
    if !explicit && scope.preceding() != Some(scalar(result)) {
        return Err("the draft is not the current card group".into());
    }
    current(root, path, result, explicit)?;
    Ok(json!({"kind":"Transition","args":{"result":result,"phase":"After"}}))
}

/// A later search or creation cannot leave the bare reference on an earlier
/// draft merely because that operation has no richer noun binding yet. Public
/// moves of the same scalar remain handled by the shared incarnation binder.
fn current(root: &Value, path: &[Step], result: u64, card_noun: bool) -> Result<(), String> {
    let mut node = root;
    for (depth, step) in path.iter().enumerate() {
        if let Step::Index(index) = step
            && matches!(depth.checked_sub(1).and_then(|i| path.get(i)), Some(Step::Key(key)) if key == "instructions")
            && let Some(items) = node.as_array()
        {
            let mut seen = false;
            for item in items.iter().take(*index) {
                if item["kind"] != "Effect" {
                    continue;
                }
                if item["args"]["result"].as_u64() == Some(result) {
                    seen = true;
                    continue;
                }
                if seen
                    && !(card_noun
                        && selections::action(&item["args"]["operation"])["kind"] == "CreateTokens")
                    && matches!(
                        selections::action(&item["args"]["operation"])["kind"].as_str(),
                        Some(
                            "Search"
                                | "ChooseObjects"
                                | "Conjure"
                                | "Manifest"
                                | "CreateTokens"
                                | "DraftFromSpellbook"
                        )
                    )
                {
                    return Err(
                        "a later object action supersedes the earlier draft reference".into(),
                    );
                }
            }
        }
        node = match step {
            Step::Key(key) => node.get(key),
            Step::Index(index) => node.get(*index),
        }
        .ok_or("draft reference path")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plural_focus_is_neither_source_nor_an_arbitrary_member() {
        let mut scope = Scope {
            drafted_result: Some((7, false)),
            ..Scope::default()
        };
        scope.objects.push(scalar(7));
        scope.named_object = Some(scalar(7));
        assert!(card(&scope).unwrap().is_err());
        assert!(check_scalar(&Value::Null, &[], "it", &scalar(7), &scope).is_err());
        assert!(check_scalar(&Value::Null, &[], "self", &json!({"kind":"Source"}), &scope).is_ok());
    }

    #[test]
    fn a_later_search_invalidates_a_stale_draft_reference() {
        let root = json!({"instructions":[
            {"kind":"Effect","args":{"result":7,"operation":{"kind":"DraftFromSpellbook"}}},
            {"kind":"Effect","args":{"result":8,"operation":{"kind":"Search"}}},
            {"kind":"Effect","args":{"result":9,"operation":{"kind":"Move","args":{"objects":scalar(7)}}}}
        ]});
        let path = [
            Step::Key("instructions".into()),
            Step::Index(2),
            Step::Key("args".into()),
            Step::Key("operation".into()),
            Step::Key("args".into()),
            Step::Key("objects".into()),
        ];
        assert!(current(&root, &path, 7, false).is_err());
    }
}
