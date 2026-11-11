//! Bare lexical “it” can name the current member of a distributive static pump.
use super::{Step, at_path, symbol_name};
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

const MARKER: &str = "member_count_it";
const SUBJECT: &str = "distributive_pump_subject";

fn at<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get(key)?,
            Step::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}

fn keys(path: &[Step], names: &[&str]) -> bool {
    path.len() == names.len()
        && path
            .iter()
            .zip(names)
            .all(|(step, name)| matches!(step, Step::Key(key) if key == name))
}

/// Positive membership proof does not descend through arbitrary predicates.
fn positive(filter: &Value, kind: &str, name: &str, work: &mut usize, depth: usize) -> bool {
    if *work == 0 || depth > 32 {
        return false;
    }
    *work -= 1;
    if filter["kind"] == kind && filter["args"] == name {
        return true;
    }
    let Some(parts) = filter["args"].as_array() else {
        return false;
    };
    match filter["kind"].as_str() {
        Some("AllOf") => parts
            .iter()
            .any(|part| positive(part, kind, name, work, depth + 1)),
        Some("AnyOf") => {
            !parts.is_empty()
                && parts
                    .iter()
                    .all(|part| positive(part, kind, name, work, depth + 1))
        }
        _ => false,
    }
}

fn subject(owner: &Value) -> bool {
    let subject = &owner["args"]["affected"]["Objects"];
    let value = &subject["args"][0];
    symbol_name(subject) == Some(SUBJECT)
        && value["kind"] == "Zone"
        && value["args"]["scope"]["Shared"] == "Battlefield"
        && value["args"]["kind"] == "Permanent"
        && positive(
            &value["args"]["filter"],
            "CardType",
            "Creature",
            &mut 256,
            0,
        )
}

/// The relation stays at the counted Equipment filter's boolean level. A
/// nested object predicate/query never names this pump's current member.
fn attachment(mut node: &Value, mut path: &[Step]) -> bool {
    loop {
        match node["kind"].as_str() {
            Some("AllOf" | "AnyOf") => {
                let [Step::Key(args), Step::Index(index), rest @ ..] = path else {
                    return false;
                };
                if args != "args" {
                    return false;
                }
                let Some(child) = node["args"].get(*index) else {
                    return false;
                };
                node = child;
                path = rest;
            }
            Some("Not") => {
                let [Step::Key(args), rest @ ..] = path else {
                    return false;
                };
                if args != "args" {
                    return false;
                }
                node = &node["args"];
                path = rest;
            }
            Some("AttachedTo") => {
                return keys(path, &["args", "args"])
                    && node["args"]["kind"] == "SameObject"
                    && symbol_name(&node["args"]["args"]) == Some(MARKER);
            }
            _ => return false,
        }
    }
}

/// Only the pump's direct arithmetic operands lead to either supported count.
fn count(mut expr: &Value, mut path: &[Step]) -> bool {
    loop {
        match expr["kind"].as_str() {
            Some("Add" | "Multiply") => {
                let [Step::Key(args), Step::Index(index), rest @ ..] = path else {
                    return false;
                };
                if args != "args" {
                    return false;
                }
                let Some(child) = expr["args"].get(*index) else {
                    return false;
                };
                expr = child;
                path = rest;
            }
            Some("CountCounters") => {
                return keys(path, &["args", "recipients", "args", "args"])
                    && expr["args"]["recipients"]["kind"] == "Objects"
                    && expr["args"]["recipients"]["args"]["kind"] == "One"
                    && symbol_name(&expr["args"]["recipients"]["args"]["args"]) == Some(MARKER);
            }
            Some("CountObjects") => {
                let [
                    Step::Key(args),
                    Step::Key(zone_args),
                    Step::Key(filter),
                    rest @ ..,
                ] = path
                else {
                    return false;
                };
                let zone = &expr["args"];
                return args == "args"
                    && zone_args == "args"
                    && filter == "filter"
                    && zone["kind"] == "Zone"
                    && zone["args"]["scope"]["Shared"] == "Battlefield"
                    && zone["args"]["kind"] == "Permanent"
                    && positive(&zone["args"]["filter"], "Subtype", "Equipment", &mut 256, 0)
                    && attachment(&zone["args"]["filter"], rest);
            }
            _ => return false,
        }
    }
}

fn eligible(root: &Value, path: &[Step]) -> Result<bool, String> {
    let [annotation @ .., Step::Key(args), Step::Index(0)] = path else {
        return Ok(false);
    };
    if args != "args" || at(root, annotation).and_then(symbol_name) != Some(MARKER) {
        return Ok(false);
    }
    let mut node = root;
    let mut owner = None;
    for position in 0..=annotation.len() {
        // A new owner resets the preceding one, including snapshot effects and
        // nonstatic abilities. Nested statics establish their own subject.
        if matches!(
            node["kind"].as_str(),
            Some(
                "Static"
                    | "CreateContinuousEffect"
                    | "Spell"
                    | "Activated"
                    | "Triggered"
                    | "CreateTrigger"
            )
        ) {
            owner = Some((position, node));
        }
        if node["kind"] == "Pump"
            && let Some((start, owner)) = owner
            && owner["kind"] == "Static"
            && owner["args"]["kind"] == "Ordinary"
            && position == start + 3
            && keys(&annotation[start..start + 2], &["args", "changes"])
            && matches!(annotation[start + 2], Step::Index(_))
            && subject(owner)
        {
            let [Step::Key(args), Step::Key(stat), rest @ ..] = &annotation[position..] else {
                return Ok(false);
            };
            if args != "args"
                || !matches!(stat.as_str(), "power" | "toughness")
                || !count(&node["args"][stat], rest)
            {
                return Ok(false);
            }
            if path.len() > 128 {
                return Err("static member count reference depth exceeds128".into());
            }
            return Ok(true);
        }
        if let Some(step) = annotation.get(position) {
            let next = match step {
                Step::Key(key) => node.get(key),
                Step::Index(index) => node.get(*index),
            };
            let Some(next) = next else {
                return Ok(false);
            };
            node = next;
        }
    }
    Ok(false)
}

/// Restore every ordinary prototype's exact historical shape before any holder,
/// receipt, context or target-identity helper observes it. Only proven static
/// member references keep the annotation until finish_scope.
pub(super) fn prepare(root: &mut Value) -> Result<(), String> {
    fn collect(
        value: &Value,
        path: &mut Vec<Step>,
        paths: &mut Vec<Vec<Step>>,
        work: &mut usize,
    ) -> Result<(), String> {
        if *work == 0 || path.len() > 1024 {
            return Err("member provenance preparation work/depth limit".into());
        }
        *work -= 1;
        if symbol_name(value) == Some(MARKER) {
            paths.push(path.clone());
        }
        match value {
            Value::Object(fields) => {
                for (key, child) in fields {
                    path.push(Step::Key(key.clone()));
                    collect(child, path, paths, work)?;
                    path.pop();
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    path.push(Step::Index(index));
                    collect(child, path, paths, work)?;
                    path.pop();
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut annotations = Vec::new();
    collect(root, &mut Vec::new(), &mut annotations, &mut 262_144)?;
    let mut ordinary = Vec::new();
    for mut path in annotations {
        let annotation = at(root, &path).ok_or("member provenance preparation path")?;
        if !matches!(annotation["args"].as_array().map(Vec::as_slice), Some([_])) {
            return Err("member provenance annotation arity".into());
        }
        path.extend([Step::Key("args".into()), Step::Index(0)]);
        if !eligible(root, &path)? {
            path.truncate(path.len() - 2);
            ordinary.push(path);
        }
    }
    ordinary.sort_by_key(|path| std::cmp::Reverse(path.len()));
    for path in ordinary {
        let node = at_path(root, &path).ok_or("ordinary member provenance path")?;
        *node = node["args"][0].take();
    }
    Ok(())
}

pub(super) fn marker(root: &mut Value, path: &[Step], name: &str) -> Result<bool, String> {
    if name != "it" || !eligible(root, path)? {
        return Ok(false);
    }
    *at_path(root, path).ok_or("static member count marker path")? = json!({"kind":"Affected"});
    Ok(true)
}

/// The literal lexical marker alone may propose Affected. Ordinary inverse
/// candidates remain first; verified whole-IR reparse proves the owner/path.
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else {
        return Vec::new();
    };
    let mut ordinary = inner.unify(value, bindings.clone());
    if ordinary.is_empty()
        && value.as_object().is_some_and(|fields| fields.len() == 1)
        && value["kind"] == "Affected"
        && matches!(inner, Pattern::Symbol(name, args) if name == "it" && args.is_empty())
    {
        ordinary.push(bindings);
    }
    ordinary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked() -> Value {
        json!({"$sym":MARKER,"args":[{"$sym":"it","args":[],"at":7}],"at":7})
    }
    fn counters() -> Value {
        json!({"kind":"CountCounters","args":{"recipients":{"kind":"Objects","args":{"kind":"One","args":marked()}},
            "kind":{"Named":"SporeCounter"},"information":"CurrentOrLastKnown"}})
    }
    fn equipment() -> Value {
        json!({"kind":"CountObjects","args":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"AllOf","args":[
            {"kind":"Subtype","args":"Equipment"},
            {"kind":"AttachedTo","args":{"kind":"SameObject","args":marked()}}
        ]}}}})
    }
    fn input(expr: Value) -> Value {
        json!({"kind":"Static","args":{"kind":"Ordinary","affected":{"Objects":{
            "$sym":SUBJECT,"args":[{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},
                "kind":"Permanent","filter":{"kind":"CardType","args":"Creature"}}}]}},
            "changes":[{"kind":"Pump","args":{"power":expr,"toughness":{"kind":"Constant","args":0}}}]}})
    }
    fn path(value: &Value) -> Vec<Step> {
        fn find(value: &Value, path: &mut Vec<Step>) -> bool {
            if symbol_name(value) == Some("it") {
                return true;
            }
            match value {
                Value::Object(fields) => {
                    for (key, child) in fields {
                        path.push(Step::Key(key.clone()));
                        if find(child, path) {
                            return true;
                        }
                        path.pop();
                    }
                }
                Value::Array(items) => {
                    for (index, child) in items.iter().enumerate() {
                        path.push(Step::Index(index));
                        if find(child, path) {
                            return true;
                        }
                        path.pop();
                    }
                }
                _ => {}
            }
            false
        }
        let mut out = Vec::new();
        assert!(find(value, &mut out));
        out
    }

    #[test]
    fn direct_counter_recipient_and_equipment_relation_use_current_member() {
        for expr in [counters(), equipment()] {
            for factor in [0, 2] {
                let expr =
                    json!({"kind":"Multiply","args":[{"kind":"Constant","args":factor},expr]});
                let mut root = input(expr);
                let location = path(&root);
                let before = root.clone();
                assert!(marker(&mut root, &location, "it").unwrap());
                let mut expected = before;
                *at_path(&mut expected, &location).unwrap() = json!({"kind":"Affected"});
                assert_eq!(root, expected, "only the lexical recipient changes");
                assert_eq!(
                    root["args"]["changes"][0]["args"]["power"]["args"][0]["args"],
                    factor
                );
            }
        }
    }

    #[test]
    fn explicit_source_holder_and_granter_are_not_lexical_bare_pronouns() {
        for name in [
            "self",
            "named_source",
            "counter_reference_single",
            "defined_it",
        ] {
            let mut root = input(counters());
            let location = path(&root);
            at_path(&mut root, &location).unwrap()["$sym"] = json!(name);
            let before = root.clone();
            assert!(!marker(&mut root, &location, name).unwrap());
            assert_eq!(root, before);
        }
    }

    #[test]
    fn owner_and_subject_proof_are_required() {
        let original = input(counters());
        let mut cases = vec![original.clone(); 5];
        cases[0]["kind"] = json!("CreateContinuousEffect");
        cases[1]["args"]["kind"] = json!("SpellRule");
        cases[2]["args"]["affected"]["Objects"] =
            original["args"]["affected"]["Objects"]["args"][0].clone();
        cases[3]["args"]["affected"]["Objects"]["args"][0]["args"]["filter"] =
            json!({"kind":"Subtype","args":"Equipment"});
        cases[4]["args"]["affected"]["Objects"]["args"][0]["args"]["scope"] =
            json!({"Shared":"Graveyard"});
        for mut root in cases {
            let location = path(&root);
            let before = root.clone();
            assert!(!marker(&mut root, &location, "it").unwrap());
            assert_eq!(root, before);
        }
    }

    #[test]
    fn nested_query_predicate_wrong_relation_and_other_count_domains_are_excluded() {
        let mut cases = vec![input(equipment()); 4];
        cases[0]["args"]["changes"][0]["args"]["power"]["args"]["args"]["filter"]["args"][0]["args"] =
            json!("Aura");
        cases[1]["args"]["changes"][0]["args"]["power"]["args"]["args"]["filter"]["args"][1]["kind"] =
            json!("HasAttachment");
        cases[2]["args"]["changes"][0]["args"]["power"] = json!({"kind":"ObjectProperty","args":{"object":{"kind":"Query","args":{"objects":counters(),"index":0}},"property":"Power"}});
        cases[3]["args"]["changes"][0]["args"]["power"] = json!({"kind":"CountObjects","args":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"ManaValue","args":counters()}}}});
        for mut root in cases {
            let location = path(&root);
            assert!(!marker(&mut root, &location, "it").unwrap());
        }
    }

    #[test]
    fn unrelated_sibling_change_cannot_capture_the_static_member() {
        let mut root = input(counters());
        let pump = root["args"]["changes"][0].clone();
        root["args"]["changes"][0] = json!({"kind":"GrantAbility","args":{"kind":"Triggered","args":{"event":null,"body":pump}}});
        let location = path(&root);
        assert!(!marker(&mut root, &location, "it").unwrap());
    }

    #[test]
    fn bounded_proof_and_only_proven_deep_paths_raise_the_new_depth_error() {
        assert!(!positive(
            &json!({"kind":"CardType","args":"Creature"}),
            "CardType",
            "Creature",
            &mut 0,
            0
        ));
        assert!(!positive(
            &json!({"kind":"CardType","args":"Creature"}),
            "CardType",
            "Creature",
            &mut 256,
            33
        ));
        let mut expr = counters();
        for _ in 0..64 {
            expr = json!({"kind":"Add","args":[expr]});
        }
        let mut root = input(expr);
        let location = path(&root);
        assert!(marker(&mut root, &location, "it").is_err());
        root["kind"] = json!("CreateContinuousEffect");
        assert!(!marker(&mut root, &location, "it").unwrap());
    }

    #[test]
    fn inverse_extra_case_is_only_the_literal_marked_bare_it_constructor() {
        let affected = json!({"kind":"Affected"});
        let literal = [Pattern::Symbol("it".into(), vec![])];
        assert_eq!(unify(&literal, &affected, Bindings::default()).len(), 1);
        for name in [
            "self",
            "named_source",
            "defined_it",
            "counter_reference_single",
        ] {
            let patterns = [Pattern::Symbol(name.into(), vec![])];
            assert!(unify(&patterns, &affected, Bindings::default()).is_empty());
        }
        let source = json!({"kind":"Source"});
        assert_eq!(
            unify(&literal, &source, Bindings::default()).len(),
            literal[0].unify(&source, Bindings::default()).len()
        );
        let explicit = [Pattern::Value(source)];
        assert!(unify(&explicit, &affected, Bindings::default()).is_empty());
    }

    #[test]
    fn transparent_metadata_is_not_a_second_reference_and_strips_only_at_finish() {
        let mut root = input(counters());
        let before = root.clone();
        let mut found = Vec::new();
        super::super::references::collect(
            &root,
            &mut Vec::new(),
            &mut found,
            super::super::filters::Context::Outside,
            &mut false,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(at(&root, &found[0].path).and_then(symbol_name), Some("it"));
        assert_eq!(root, before);
        let location = found[0].path.clone();
        assert!(marker(&mut root, &location, "it").unwrap());
        assert!(root.to_string().contains(MARKER));
        super::super::filters::strip(&mut root);
        assert!(!root.to_string().contains(MARKER));
        assert_eq!(
            root["args"]["changes"][0]["args"]["power"]["args"]["recipients"]["args"]["args"],
            json!({"kind":"Affected"})
        );
    }

    #[test]
    fn preparation_restores_nonmember_cost_and_damage_source_exact_shapes() {
        let ordinary = json!({"kind":"Activated","args":{
            "cost":{"parts":[{"Sacrifice":{"objects":{"kind":"One","args":{"$sym":"it","args":[],"at":1}}}}]},
            "body":{"source":{"$sym":"sacrificed_damage_source","args":[{"$sym":"it","args":[],"at":2}],"at":2}}
        }});
        let mut annotated = ordinary.clone();
        annotated["args"]["cost"]["parts"][0]["Sacrifice"]["objects"]["args"] = json!({"$sym":MARKER,"args":[ordinary["args"]["cost"]["parts"][0]["Sacrifice"]["objects"]["args"]]});
        annotated["args"]["body"]["source"]["args"][0] =
            json!({"$sym":MARKER,"args":[ordinary["args"]["body"]["source"]["args"][0]]});
        prepare(&mut annotated).unwrap();
        assert_eq!(annotated, ordinary);
        let mut member = input(counters());
        let before = member.clone();
        prepare(&mut member).unwrap();
        assert_eq!(
            member, before,
            "proven member annotation survives prebinding"
        );
    }

    #[test]
    fn ordinary_metadata_normalizes_before_target_identity_capture() {
        let reference = json!({"kind":"Source"});
        let spec = json!({"kind":"Object","filter":{"kind":"AttachedTo","args":{"kind":"SameObject","args":{"$sym":MARKER,"args":[reference]}}}});
        let mut root = json!({"independent":{"$sym":"target","at":1,"args":[{"kind":"Player"}]},
            "later":{"$sym":"target","at":2,"args":[spec]}});
        prepare(&mut root).unwrap();
        assert_eq!(
            root["later"]["args"][0]["filter"]["args"]["args"],
            reference
        );
        let mut found = Vec::new();
        super::super::references::collect(
            &root,
            &mut Vec::new(),
            &mut found,
            super::super::filters::Context::Outside,
            &mut false,
        );
        found.sort_by_key(|item| (item.at, item.order));
        let mut scope = super::super::Scope::default();
        let mut introduced = Vec::new();
        let mut introductions = Vec::new();
        let mut contexts = super::super::references::Contexts::default();
        for item in &found {
            super::super::references::introduce(
                &mut root,
                &mut scope,
                item,
                &mut introduced,
                &mut introductions,
                &mut contexts,
            )
            .unwrap();
        }
        let stable = root.clone();
        prepare(&mut root).unwrap();
        assert_eq!(root, stable);
        assert_eq!(
            super::super::references::bound_reference(&root["later"], &introduced, 0).unwrap(),
            json!(1)
        );
    }

    #[test]
    fn end_to_end_native_members_and_both_paid_and_ordinary_goblin_bomb_sources() {
        use crate::engine::{Engine, canonical_abilities};
        let engine = Engine::load().unwrap();
        let fixtures: Vec<Value> =
            serde_json::from_str(include_str!("../../tests/fixtures/member-focus.json")).unwrap();
        for case in fixtures.into_iter().take(3) {
            let name = case["name"].as_str().unwrap();
            let actual = engine
                .parse(name, case["text"].as_str().unwrap())
                .unwrap()
                .readings;
            assert_eq!(
                actual,
                vec![canonical_abilities(&case["expected"]).unwrap()],
                "{name}"
            );
        }
        let fixtures: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/member-focus-nonmember.json"
        ))
        .unwrap();
        for case in fixtures {
            let mut actual = engine
                .parse(
                    case["name"].as_str().unwrap(),
                    case["text"].as_str().unwrap(),
                )
                .unwrap()
                .readings;
            let mut expected: Vec<_> = case["expected_readings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|reading| canonical_abilities(reading).unwrap())
                .collect();
            actual.sort_by_key(Value::to_string);
            expected.sort_by_key(Value::to_string);
            expected.dedup();
            assert_eq!(expected.len(), 2);
            assert_eq!(
                actual, expected,
                "Goblin Bomb must keep both distinct source meanings"
            );
        }
    }
}
