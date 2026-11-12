//! Lexical distributive subjects establish the current member only inside a Pump.
use super::{Step, at_path, symbol_name};
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

const SUBJECT: &str = "distributive_pump_subject";
const OTHER: &str = "pump_other_ref";
const NAME: &str = "pump_name_ref";
/// Contextual `other` from an ordinary filter; it names the member only here.
const CONTEXTUAL: &str = "other_reference";

fn source(value: &Value) -> bool {
    value.as_object().is_some_and(|fields| fields.len() == 1) && value["kind"] == "Source"
}

/// A learned literal Source fallback must not become a leaving-event pronoun.
pub(super) fn constant_marker(value: &Value) -> bool {
    symbol_name(value) == Some(OTHER)
        && value["args"]
            .as_array()
            .is_some_and(|args| args.len() == 1 && source(&args[0]))
}

/// Resolve annotated references; a constant fallback remains literally Source
/// outside this exact Pump context, even when the enclosing scope is leaving.
pub(super) fn marker(root: &mut Value, path: &[Step], name: &str) -> Result<bool, String> {
    if name == OTHER && at(root, path).is_some_and(constant_marker) {
        if eligible(root, path, name)? {
            *at_path(root, path).ok_or("distributive count marker path")? =
                json!({"kind":"Affected"});
        }
        return Ok(true);
    }
    if name == CONTEXTUAL {
        if !eligible(root, path, name)? {
            return Ok(false);
        }
        *at_path(root, path).ok_or("distributive contextual other path")? =
            json!({"kind":"Affected"});
        return Ok(true);
    }
    // A `where X is` definition renames its pronouns; the member is the same.
    if !matches!(
        name,
        "it" | "named_object" | "defined_it" | "defined_named_object"
    ) || !eligible(root, path, name)?
    {
        return Ok(false);
    }
    *at_path(root, path).ok_or("distributive count reference path")? = json!({"kind":"Affected"});
    Ok(true)
}

fn step_key(step: &Step, key: &str) -> bool {
    matches!(step, Step::Key(actual) if actual == key)
}
fn at<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get(key)?,
            Step::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}

fn positive(filter: &Value, kind: &str, name: &str, work: &mut usize, depth: usize) -> bool {
    if *work == 0 {
        return false;
    }
    *work -= 1;
    if depth > 32 {
        return false;
    }
    if filter["kind"] == kind && filter["args"] == name {
        return true;
    }
    if filter["kind"] == "AllOf"
        && let Some(parts) = filter["args"].as_array()
    {
        for part in parts {
            if *work == 0 {
                return false;
            }
            if positive(part, kind, name, work, depth + 1) {
                return true;
            }
        }
    }
    false
}

fn shared_subtype(subject: &Value, count: &Value, work: &mut usize, depth: usize) -> bool {
    if *work == 0 {
        return false;
    }
    *work -= 1;
    if depth > 32 {
        return false;
    }
    if count["kind"] == "Subtype" {
        return count["args"]
            .as_str()
            .is_some_and(|name| positive(subject, "Subtype", name, work, 0));
    }
    if count["kind"] == "AllOf"
        && let Some(parts) = count["args"].as_array()
    {
        for part in parts {
            if *work == 0 {
                return false;
            }
            if shared_subtype(subject, part, work, depth + 1) {
                return true;
            }
        }
    }
    false
}

/// The count names the subject's own kind: both are creatures, or both name
/// one subtype, as in `each Skeleton ... the number of other Skeletons`.
fn compatible_creatures(subject: &Value, count: &Value) -> bool {
    shared_subtype(subject, count, &mut 256, 0)
        || (positive(subject, "CardType", "Creature", &mut 256, 0)
            && positive(count, "CardType", "Creature", &mut 256, 0))
}

/// Only arithmetic expression edges lead to this count. A count nested inside
/// another query's filter, a predicate, or @in_filter cannot capture Affected.
fn counted_filter<'a>(mut value: &'a Value, mut path: &[Step], marker: &str) -> Option<&'a Value> {
    let [Step::Key(args), Step::Key(amount), rest @ ..] = path else {
        return None;
    };
    if args != "args" || !matches!(amount.as_str(), "power" | "toughness") {
        return None;
    }
    value = value.get("args")?.get(amount)?;
    path = rest;
    loop {
        match value["kind"].as_str()? {
            "CountObjects" => break,
            "Add" | "Multiply" => {
                let [Step::Key(args), Step::Index(index), rest @ ..] = path else {
                    return None;
                };
                if args != "args" {
                    return None;
                }
                value = value.get("args")?.get(*index)?;
                path = rest;
            }
            _ => return None,
        }
    }
    let [Step::Key(args), rest @ ..] = path else {
        return None;
    };
    let mut rest = rest;
    if args != "args" {
        return None;
    }
    value = &value["args"];
    // A learned count can use the same lexical object-set production as its
    // subject. This wrapper is transparent here and never establishes an owner.
    if symbol_name(value) == Some(SUBJECT) {
        let [Step::Key(args), Step::Index(0), tail @ ..] = rest else {
            return None;
        };
        if args != "args" {
            return None;
        }
        value = &value["args"][0];
        rest = tail;
    }
    if value["kind"] != "Zone"
        || value["args"]["scope"]["Shared"] != "Battlefield"
        || value["args"]["kind"] != "Permanent"
    {
        return None;
    }
    let [Step::Key(args), Step::Key(filter), rest @ ..] = rest else {
        return None;
    };
    if args != "args" || filter != "filter" {
        return None;
    }
    let filter = &value["args"]["filter"];
    relation_path(filter, rest, marker).then_some(filter)
}

fn relation_path(mut filter: &Value, mut path: &[Step], marker: &str) -> bool {
    let mut negated = false;
    loop {
        match filter["kind"].as_str() {
            Some("AllOf" | "AnyOf") => {
                let [Step::Key(args), Step::Index(index), rest @ ..] = path else {
                    return false;
                };
                if args != "args" {
                    return false;
                }
                let Some(child) = filter["args"].get(*index) else {
                    return false;
                };
                filter = child;
                path = rest;
                negated = false;
            }
            Some("Not") => {
                let [Step::Key(args), rest @ ..] = path else {
                    return false;
                };
                if args != "args" {
                    return false;
                }
                filter = &filter["args"];
                path = rest;
                negated = true;
            }
            Some(kind)
                if (matches!(marker, OTHER | CONTEXTUAL) && kind == "SameObject" && negated)
                    || (marker == NAME && kind == "SharesNameWith") =>
            {
                return matches!(path, [Step::Key(argument)] if argument == "args")
                    && symbol_name(&filter["args"]) == Some(marker);
            }
            _ => return false,
        }
    }
}

/// Does the path from the Pump to the marker pass through the subject's own
/// object-set production, read at the same token as the subject?
fn repeats_subject(mut node: &Value, path: &[Step], subject: &Value) -> bool {
    for step in path {
        if symbol_name(node) == Some(SUBJECT) && node["at"] == subject["at"] {
            return true;
        }
        node = match step {
            Step::Key(key) => &node[key.as_str()],
            Step::Index(index) => &node[*index],
        };
    }
    false
}

fn eligible(root: &Value, path: &[Step], name: &str) -> Result<bool, String> {
    let annotation = if name == OTHER {
        if !at(root, path).is_some_and(constant_marker) {
            return Ok(false);
        }
        path
    } else if name == CONTEXTUAL {
        path
    } else {
        let [annotation @ .., Step::Key(args), Step::Index(0)] = path else {
            return Ok(false);
        };
        if args != "args" {
            return Ok(false);
        }
        annotation
    };
    let expected = match name {
        "named_object" | "defined_named_object" | NAME => NAME,
        CONTEXTUAL => CONTEXTUAL,
        _ => OTHER,
    };
    if at(root, annotation).and_then(symbol_name) != Some(expected) {
        return Ok(false);
    }
    let mut node = root;
    let mut owner = None;
    for position in 0..=annotation.len() {
        if matches!(
            node["kind"].as_str(),
            Some("Static" | "CreateContinuousEffect")
        ) {
            owner = Some((position, node));
        }
        if node["kind"] == "Pump"
            && let Some((start, owner)) = owner
            && position == start + 3
            && step_key(&annotation[start], "args")
            && step_key(&annotation[start + 1], "changes")
            && matches!(annotation[start + 2], Step::Index(_))
        {
            let subject = &owner["args"]["affected"]["Objects"];
            let inner = &subject["args"][0];
            let Some(filter) = counted_filter(node, &annotation[position..], expected) else {
                return Ok(false);
            };
            if symbol_name(subject) != Some(SUBJECT)
                || inner["kind"] != "Zone"
                || inner["args"]["scope"]["Shared"] != "Battlefield"
                || inner["args"]["kind"] != "Permanent"
                || !compatible_creatures(&inner["args"]["filter"], filter)
            {
                return Ok(false);
            }
            // A count that repeats the subject's words keeps the subject's `other`.
            if name == CONTEXTUAL && repeats_subject(node, &annotation[position..], subject) {
                return Ok(false);
            }
            // Bound only this proven new context; unrelated legacy paths remain unchanged.
            if annotation.len() > 126 {
                return Err("distributive pump reference depth exceeds128".into());
            }
            return Ok(true);
        }
        if let Some(step) = annotation.get(position) {
            node = match step {
                Step::Key(key) => match node.get(key) {
                    Some(child) => child,
                    None => return Ok(false),
                },
                Step::Index(index) => match node.get(*index) {
                    Some(child) => child,
                    None => return Ok(false),
                },
            };
        }
    }
    Ok(false)
}

fn source_pattern(pattern: &Pattern) -> bool {
    match pattern {
        Pattern::Value(value) => source(value),
        Pattern::Object(fields) => {
            matches!(fields.as_slice(), [(key, Pattern::Value(Value::String(kind)))] if key == "kind" && kind == "Source")
        }
        _ => false,
    }
}

/// Keep ordinary candidates first. The only extra inverse case is the exact
/// literal reference to Affected; whole-IR verified reparse enforces its owner.
pub(crate) fn unify<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else {
        return Vec::new();
    };
    let mut ordinary = inner.unify(value, bindings.clone());
    let literal = match name {
        OTHER => "it",
        NAME => "named_object",
        _ => return ordinary,
    };
    if ordinary.is_empty()
        && value.as_object().is_some_and(|fields| fields.len() == 1)
        && value["kind"] == "Affected"
        && (matches!(inner, Pattern::Symbol(symbol, arguments) if symbol == literal && arguments.is_empty())
            || (name == OTHER && source_pattern(inner)))
    {
        ordinary.push(bindings);
    }
    ordinary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked(name: &str, value: Value) -> Value {
        json!({"$sym":name,"args":[value]})
    }
    fn input(owner: &str, relation: &str, distributive: bool) -> (Value, Vec<Step>) {
        let pronoun = if relation == OTHER {
            "it"
        } else {
            "named_object"
        };
        let reference = marked(relation, json!({"$sym":pronoun,"args":[]}));
        let filter = if relation == OTHER {
            json!({"kind":"Not","args":{"kind":"SameObject","args":reference}})
        } else {
            json!({"kind":"SharesNameWith","args":reference})
        };
        let subject = json!({"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},
            "kind":"Permanent","filter":{"kind":"CardType","args":"Creature"}}});
        let subject = if distributive {
            marked(SUBJECT, subject)
        } else {
            subject
        };
        let value = json!({"kind":owner,"args":{"source":{"kind":"GrantingObject"},"affected":{"Objects":subject},"changes":[{"kind":"Pump","args":{"power":{"kind":"CountObjects","args":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"AllOf","args":[{"kind":"CardType","args":"Creature"},filter]}}}},"toughness":{"kind":"Constant","args":0}}},
                {"kind":"GrantKeyword","args":{"kind":"Flying"}}]}});
        let path = vec![
            Step::Key("args".into()),
            Step::Key("changes".into()),
            Step::Index(0),
            Step::Key("args".into()),
            Step::Key("power".into()),
            Step::Key("args".into()),
            Step::Key("args".into()),
            Step::Key("filter".into()),
        ];
        let mut path = path;
        path.extend([Step::Key("args".into()), Step::Index(1)]);
        if relation == OTHER {
            path.extend([Step::Key("args".into()), Step::Key("args".into())]);
        } else {
            path.push(Step::Key("args".into()));
        }
        path.extend([Step::Key("args".into()), Step::Index(0)]);
        (value, path)
    }

    #[test]
    fn static_and_resolution_counts_use_current_member_only() {
        for owner in ["Static", "CreateContinuousEffect"] {
            for relation in [OTHER, NAME] {
                let (mut root, path) = input(owner, relation, true);
                let source = root["args"]["source"].clone();
                let sibling = root["args"]["changes"][1].clone();
                let pronoun = if relation == OTHER {
                    "it"
                } else {
                    "named_object"
                };
                assert!(marker(&mut root, &path, pronoun).unwrap());
                assert_eq!(at(&root, &path), Some(&json!({"kind":"Affected"})));
                assert_eq!(root["args"]["source"], source);
                assert_eq!(root["args"]["changes"][1], sibling);
            }
        }
    }

    #[test]
    fn scalar_unmarked_and_nonpump_owners_keep_legacy_references() {
        let (mut root, path) = input("Static", OTHER, false);
        let before = root.clone();
        assert!(!marker(&mut root, &path, "it").unwrap());
        assert_eq!(root, before);
        let (mut root, path) = input("Static", OTHER, true);
        root["args"]["changes"][0]["kind"] = json!("NotPump");
        assert!(!marker(&mut root, &path, "it").unwrap());
        let (mut root, path) = input("GainLife", OTHER, true);
        assert!(!marker(&mut root, &path, "it").unwrap());
    }

    #[test]
    fn provenance_and_battlefield_domain_are_required() {
        let (mut root, path) = input("Static", OTHER, true);
        root["args"]["changes"][0]["args"]["power"]["args"]["args"]["scope"] =
            json!({"Shared":"Exile"});
        assert!(!marker(&mut root, &path, "it").unwrap());
        let (mut root, path) = input("Static", OTHER, true);
        let annotation = &path[..path.len() - 2];
        *at_path(&mut root, annotation).unwrap() =
            json!({"$sym":"ordinary_reference","args":[{"$sym":"it","args":[]}]});
        assert!(!marker(&mut root, &path, "it").unwrap());
        let (mut root, path) = input("Static", NAME, true);
        assert!(!marker(&mut root, &path, "it").unwrap());
    }

    #[test]
    fn subject_filter_and_unrelated_object_properties_are_not_count_relations() {
        let (mut root, _) = input("Static", OTHER, true);
        root["args"]["affected"]["Objects"]["args"][0]["args"]["filter"] = json!({"kind":"Not","args":{"kind":"SameObject","args":marked(OTHER,json!({"$sym":"it","args":[]}))}});
        let path = vec![
            Step::Key("args".into()),
            Step::Key("affected".into()),
            Step::Key("Objects".into()),
            Step::Key("args".into()),
            Step::Index(0),
            Step::Key("args".into()),
            Step::Key("filter".into()),
            Step::Key("args".into()),
            Step::Key("args".into()),
            Step::Key("args".into()),
            Step::Index(0),
        ];
        assert!(!marker(&mut root, &path, "it").unwrap());
        let (mut root, path) = input("Static", OTHER, true);
        root["args"]["changes"][0]["args"]["power"]["kind"] = json!("ObjectProperty");
        assert!(!marker(&mut root, &path, "it").unwrap());
    }

    #[test]
    fn depth_bound_applies_only_to_matching_new_context() {
        for distributive in [false, true] {
            let (mut root, mut path) = input("Static", OTHER, distributive);
            for _ in 0..130 {
                root = json!({"outer":root});
                path.insert(0, Step::Key("outer".into()));
            }
            if distributive {
                assert!(marker(&mut root, &path, "it").is_err());
            } else {
                assert!(!marker(&mut root, &path, "it").unwrap());
            }
        }
    }

    #[test]
    fn inverse_preserves_ordinary_references_and_only_adds_literal_affected() {
        for (name, literal) in [(OTHER, "it"), (NAME, "named_object")] {
            let patterns = [Pattern::Symbol(literal.into(), vec![])];
            assert_eq!(
                unify(
                    name,
                    &patterns,
                    &json!({"kind":"Source"}),
                    Bindings::default()
                )
                .len(),
                1
            );
            assert_eq!(
                unify(
                    name,
                    &patterns,
                    &json!({"kind":"Affected"}),
                    Bindings::default()
                )
                .len(),
                1
            );
            let explicit = [Pattern::Symbol("named_source".into(), vec![])];
            assert!(
                unify(
                    name,
                    &explicit,
                    &json!({"kind":"Affected"}),
                    Bindings::default()
                )
                .is_empty()
            );
            let candidate = json!({"kind":"Candidate"});
            assert_eq!(
                unify(name, &patterns, &candidate, Bindings::default()).len(),
                patterns[0].unify(&candidate, Bindings::default()).len()
            );
        }
    }
    #[test]
    fn arithmetic_and_independent_compound_changes_keep_their_own_operands() {
        let (mut root, path) = input("Static", OTHER, true);
        let count = root["args"]["changes"][0]["args"]["power"].take();
        root["args"]["changes"][0]["args"]["power"] =
            json!({"kind":"Multiply","args":[{"kind":"Constant","args":2},count]});
        let mut path = path;
        drop(path.splice(5..5, [Step::Key("args".into()), Step::Index(1)]));
        let independent = json!({"kind":"Pump","args":{"power":{"kind":"ObjectProperty",
            "args":{"object":{"kind":"GrantingObject"},"property":"Power","information":"CurrentOrLastKnown"}},
            "toughness":{"kind":"Constant","args":0}}});
        root["args"]["changes"]
            .as_array_mut()
            .unwrap()
            .push(independent.clone());
        assert!(marker(&mut root, &path, "it").unwrap());
        assert_eq!(root["args"]["changes"][2], independent);
        assert_eq!(
            root["args"]["changes"][0]["args"]["power"]["args"][0],
            json!({"kind":"Constant","args":2})
        );
    }

    #[test]
    fn nested_granted_owner_cannot_inherit_an_outer_distributive_subject() {
        for inner_distributive in [false, true] {
            let (inner, path) = input("Static", OTHER, inner_distributive);
            let (mut outer, _) = input("Static", OTHER, true);
            outer["args"]["changes"] = json!([{"kind":"GrantAbility","args":inner}]);
            let mut nested = vec![
                Step::Key("args".into()),
                Step::Key("changes".into()),
                Step::Index(0),
                Step::Key("args".into()),
            ];
            nested.extend(path);
            assert_eq!(
                marker(&mut outer, &nested, "it").unwrap(),
                inner_distributive
            );
            assert_eq!(outer["args"]["source"], json!({"kind":"GrantingObject"}));
        }
    }

    #[test]
    fn nested_predicate_counts_and_filter_candidates_are_not_current_members() {
        for in_filter in [false, true] {
            let (mut root, path) = input("Static", OTHER, true);
            let inner = root["args"]["changes"][0]["args"]["power"].take();
            let inner = if in_filter {
                marked("in_filter", inner)
            } else {
                inner
            };
            root["args"]["changes"][0]["args"]["power"] = json!({"kind":"CountObjects","args":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"AllOf","args":[{"kind":"CardType","args":"Creature"},
                {"kind":"Property","args":{"property":"Power","test":{"kind":"Compare","args":{"comparison":"Greater","value":inner}}}}]}}}});
            let mut nested = path[..5].to_vec();
            nested.extend([
                Step::Key("args".into()),
                Step::Key("args".into()),
                Step::Key("filter".into()),
                Step::Key("args".into()),
                Step::Index(1),
                Step::Key("args".into()),
                Step::Key("value".into()),
            ]);
            if in_filter {
                nested.extend([Step::Key("args".into()), Step::Index(0)]);
            }
            nested.extend_from_slice(&path[5..]);
            let before = root.clone();
            assert!(!marker(&mut root, &nested, "it").unwrap());
            assert_eq!(root, before);
        }
        let (mut root, path) = input("Static", OTHER, true);
        let candidate = json!({"kind":"Property","args":{"property":"Power","test":{"kind":"Compare","args":{"comparison":"Equal","value":{"kind":"ObjectProperty","args":{"object":{"kind":"Candidate"},"property":"Power","information":"CurrentOrLastKnown"}}}}}});
        root["args"]["changes"][0]["args"]["power"]["args"]["args"]["filter"]["args"]
            .as_array_mut()
            .unwrap()
            .push(candidate.clone());
        assert!(marker(&mut root, &path, "it").unwrap());
        assert_eq!(
            root["args"]["changes"][0]["args"]["power"]["args"]["args"]["filter"]["args"][2],
            candidate
        );
    }

    #[test]
    fn cross_domain_subjects_and_explicit_name_operands_are_not_rebound() {
        let (mut root, path) = input("Static", OTHER, true);
        root["args"]["affected"]["Objects"]["args"][0]["args"]["filter"]["args"] =
            json!("Artifact");
        assert!(!marker(&mut root, &path, "it").unwrap());
        let (mut root, path) = input("Static", NAME, true);
        let annotation = &path[..path.len() - 2];
        *at_path(&mut root, annotation).unwrap() = json!({"kind":"GrantingObject"});
        let before = root.clone();
        assert!(!marker(&mut root, &path, "named_object").unwrap());
        assert_eq!(root, before);
    }
    #[test]
    fn learned_constant_markers_bind_only_the_proven_current_member() {
        let relation = OTHER;
        for distributive in [false, true] {
            let (mut root, path) = input("Static", relation, distributive);
            let annotation = &path[..path.len() - 2];
            *at_path(&mut root, &path).unwrap() = json!({"kind":"Source"});
            assert!(marker(&mut root, annotation, relation).unwrap());
            let expected = if distributive {
                json!({"kind":"Affected"})
            } else {
                marked(relation, json!({"kind":"Source"}))
            };
            assert_eq!(at(&root, annotation), Some(&expected));
        }
    }

    #[test]
    fn constant_source_fallback_survives_leaving_scope_and_collection() {
        let relation = OTHER;
        let mut root = json!({"args":marked(relation,json!({"kind":"Source"}))});
        let mut found = Vec::new();
        super::super::references::collect(
            &root,
            &mut Vec::new(),
            &mut found,
            super::super::filters::Context::Outside,
            &mut false,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, vec![Step::Key("args".into())]);
        let mut scope = super::super::Scope {
            leaving: true,
            ..Default::default()
        };
        assert!(
            super::super::bind_marker(&mut root, &mut scope, &found[0].path, relation, &[])
                .unwrap()
        );
        super::super::filters::strip(&mut root["args"]);
        assert_eq!(root["args"], json!({"kind":"Source"}));
    }

    #[test]
    fn inverse_constant_source_is_exact_and_preserves_ordinary_candidates() {
        let relation = OTHER;
        for literal in [
            Pattern::Value(json!({"kind":"Source"})),
            Pattern::Object(vec![("kind".into(), Pattern::Value(json!("Source")))]),
        ] {
            assert_eq!(
                unify(
                    relation,
                    std::slice::from_ref(&literal),
                    &json!({"kind":"Source"}),
                    Bindings::default()
                )
                .len(),
                1
            );
            assert_eq!(
                unify(
                    relation,
                    &[literal],
                    &json!({"kind":"Affected"}),
                    Bindings::default()
                )
                .len(),
                1
            );
        }
        let wrong = [Pattern::Value(json!({"kind":"GrantingObject"}))];
        assert!(
            unify(
                relation,
                &wrong,
                &json!({"kind":"Affected"}),
                Bindings::default()
            )
            .is_empty()
        );
    }
    #[test]
    fn count_set_wrapper_is_transparent_but_never_establishes_ownership() {
        for owner_distributive in [false, true] {
            let (mut root, mut path) = input("Static", OTHER, owner_distributive);
            let set = root["args"]["changes"][0]["args"]["power"]["args"].take();
            root["args"]["changes"][0]["args"]["power"]["args"] = marked(SUBJECT, set);
            drop(path.splice(6..6, [Step::Key("args".into()), Step::Index(0)]));
            assert_eq!(marker(&mut root, &path, "it").unwrap(), owner_distributive);
        }
    }

    #[test]
    fn counted_subtype_requires_positive_creature_and_same_subject_subtype() {
        for name in ["Sliver", "Goblin"] {
            let (mut root, path) = input("Static", OTHER, true);
            root["args"]["affected"]["Objects"]["args"][0]["args"]["filter"] = json!({"kind":"AllOf","args":[{"kind":"CardType","args":"Creature"},
                    {"kind":"Subtype","args":name}]});
            root["args"]["changes"][0]["args"]["power"]["args"]["args"]["filter"]["args"][0] =
                json!({"kind":"Subtype","args":"Sliver"});
            assert_eq!(marker(&mut root, &path, "it").unwrap(), name == "Sliver");
        }
    }
    #[test]
    fn learned_nested_all_proves_domain_without_crossing_boolean_negation() {
        let (mut root, path) = input("Static", OTHER, true);
        root["args"]["changes"][0]["args"]["power"]["args"]["args"]["filter"]["args"][0] =
            json!({"kind":"AllOf","args":[{"kind":"CardType","args":"Creature"}]});
        assert!(marker(&mut root, &path, "it").unwrap());
        let (mut root, path) = input("Static", OTHER, true);
        root["args"]["changes"][0]["args"]["power"]["args"]["args"]["filter"]["args"][0] =
            json!({"kind":"Not","args":{"kind":"CardType","args":"Creature"}});
        assert!(!marker(&mut root, &path, "it").unwrap());
    }
}
