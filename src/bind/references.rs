//! A target becomes a referent for the following words. Its own filter can
//! mention earlier objects without accidentally naming the target being defined.
use super::{Found, Scope, Step, at_path, filters::Context, symbol_name};
use serde_json::Value;

pub(super) type TargetIntroduction = ((u64, String), usize);
type Introduced = Vec<TargetIntroduction>;

/// Read already assigned target addresses without changing pending specs.
/// The actor may contain an object target inside ControllerOf/OwnerOf.
pub(super) fn bound_reference(
    value: &Value,
    introduced: &[TargetIntroduction],
    base: usize,
) -> Result<Value, String> {
    fn walk(
        value: &mut Value,
        introduced: &[TargetIntroduction],
        base: usize,
        work: &mut usize,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 128 || *work == 0 {
            return Err("player amount reference work/depth limit".into());
        }
        *work -= 1;
        if symbol_name(value) == Some("target") {
            let key = (
                value["at"].as_u64().ok_or("target reference position")?,
                value["args"][0].to_string(),
            );
            let (_, slot) = introduced
                .iter()
                .find(|(seen, _)| *seen == key)
                .ok_or("player amount names an unavailable target")?;
            *value = serde_json::json!(base.checked_add(*slot).ok_or("target slot overflow")?);
            return Ok(());
        }
        match value {
            Value::Object(fields) => {
                for child in fields.values_mut() {
                    walk(child, introduced, base, work, depth + 1)?;
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(child, introduced, base, work, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut plain = value.clone();
    walk(&mut plain, introduced, base, &mut 131_072, 0)?;
    Ok(plain)
}

/// Future-trigger bodies must start above every target the creating effect
/// announces, even when an outer target is written in a later sentence.
pub(super) fn target_footprint(root: &Value) -> Result<(usize, usize), String> {
    fn walk(
        value: &Value,
        keys: &mut std::collections::HashSet<(u64, String)>,
        depth: usize,
        work: &mut usize,
    ) -> Result<(), String> {
        if depth > 128 || *work == 0 {
            return Err("target footprint work/depth limit".into());
        }
        *work -= 1;
        let symbol = symbol_name(value);
        if symbol == Some("target") {
            keys.insert((
                value["at"].as_u64().unwrap_or(0),
                value["args"][0].to_string(),
            ));
        }
        if matches!(
            crate::traverse::boundary(value),
            crate::traverse::Boundary::Lexical
                | crate::traverse::Boundary::Holder
                | crate::traverse::Boundary::Reflexive
                | crate::traverse::Boundary::Branch
                | crate::traverse::Boundary::Conditional
        ) {
            if symbol == Some("scope") && value["args"][0]["kind"] == "CreateTrigger" {
                for field in ["event", "duration"] {
                    walk(&value["args"][0]["args"][field], keys, depth + 1, work)?;
                }
            }
            return Ok(());
        }
        match value {
            Value::Object(fields) => {
                for child in fields.values() {
                    walk(child, keys, depth + 1, work)?;
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(child, keys, depth + 1, work)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut keys = std::collections::HashSet::new();
    walk(root, &mut keys, 0, &mut 131_072)?;
    let mut declared_end = 0;
    if let Some(targets) = root["args"]["announcement"]["targets"].as_array() {
        for target in targets {
            if let Some(slot) = target["slot"].as_u64() {
                let end = usize::try_from(slot)
                    .ok()
                    .and_then(|slot| slot.checked_add(1))
                    .ok_or("target address space exhausted")?;
                declared_end = declared_end.max(end);
            }
        }
    }
    Ok((keys.len(), declared_end))
}

/// Ordinary scopes keep their existing binding path; only a scope creating a
/// trigger needs to reserve outer targets before traversing its future body.
pub(super) fn reserve_targets(
    root: &mut Value,
    found: &[Found],
    scope: &mut Scope,
) -> Result<(), String> {
    if found.iter().any(|item| {
        at_path(root, &item.path).is_some_and(|node| {
            symbol_name(node) == Some("scope") && node["args"][0]["kind"] == "CreateTrigger"
        })
    }) {
        let (pending_targets, declared_end) = target_footprint(root)?;
        scope.target_floor = scope.target_floor.max(declared_end).max(
            scope
                .target_base
                .checked_add(scope.targets.len())
                .and_then(|end| end.checked_add(pending_targets))
                .ok_or("target address space exhausted")?,
        );
    }
    Ok(())
}

/// One textual target shared by coordinated verbs reserves only one slot.
pub(super) fn introduce(
    root: &mut Value,
    scope: &mut Scope,
    item: &Found,
    introduced: &mut Introduced,
    introductions: &mut Vec<(Vec<Step>, usize)>,
    contexts: &mut Contexts,
) -> Result<(), String> {
    let node = at_path(root, &item.path).ok_or("target introduction path")?;
    // Dependency scheduling may delay a target; its written occurrence still
    // owns identity, independently of when its specification becomes ready.
    let key = (
        node["at"].as_u64().unwrap_or(item.at),
        node["args"][0].to_string(),
    );
    if let Some((_, slot)) = introduced.iter().find(|(seen, _)| *seen == key) {
        *node = serde_json::json!(scope.target_base + slot);
        return Ok(());
    }
    let slot = scope.targets.len();
    introduced.push((key, slot));
    introductions.push((item.path.clone(), slot));
    contexts.remember(&item.path, scope, scope.target_base + slot);
    super::introduce(node, scope)
}

impl Scope {
    /// Explicit noun phrases keep their referent when a source mention changes
    /// the nearer pronoun. Mined generic references retain their own behavior.
    pub(super) fn preceding(&self) -> Option<Value> {
        self.others
            .last()
            .or(self.named_object.as_ref())
            .or(self.objects.last())
            .cloned()
    }
}

pub(super) fn collect(
    value: &Value,
    path: &mut Vec<Step>,
    out: &mut Vec<Found>,
    context: Context,
    variable_payment: &mut bool,
) {
    let symbol = symbol_name(value);
    // The owning generated definition replaces all uses before these bind.
    if matches!(symbol, Some("snapshot_ref" | "capture_ref")) {
        return;
    }
    if !*variable_payment {
        *variable_payment = super::value_snapshot::payment_node(value);
        if matches!(
            crate::traverse::boundary(value),
            crate::traverse::Boundary::Branch | crate::traverse::Boundary::Conditional
        ) {
            *variable_payment |= super::value_snapshot::variable_payments(value);
        }
    }
    let context = context.enter(value, symbol);
    if super::pump_counts::constant_marker(value) {
        out.push(Found {
            at: value["at"].as_u64().unwrap_or(0),
            order: 0,
            path: path.clone(),
            filter: context,
        });
        return;
    }
    if matches!(
        symbol,
        Some(
            "exile_followup"
                | "return_grant_set"
                | "move_after_current_type"
                | "move_after_actor"
                | "move_after_property"
                | "member_count_it"
                | "literal_damage_itself"
                | "literal_damage_itself_objects"
                | "indefinite_opponent_controller"
                | "damage_actor_value"
                | "its_stat"
                | "fight_prior_counterpart"
                | "distributive_pump_subject"
                | "pump_other_ref"
                | "pump_name_ref"
        )
    ) {
        // Typed lexical/context annotations are not separate references.
        if let Some(inner) = value["args"].get(0) {
            path.extend([Step::Key("args".into()), Step::Index(0)]);
            collect(inner, path, out, context, variable_payment);
            path.truncate(path.len() - 2);
        }
        return;
    }
    if symbol == Some("recipient_value") {
        // The first argument annotates an already written recipient. It is not
        // a second occurrence of that recipient's target introductions.
        if let Some(inner) = value["args"].get(1) {
            path.extend([Step::Key("args".into()), Step::Index(1)]);
            collect(inner, path, out, context, variable_payment);
            path.truncate(path.len() - 2);
        }
        return;
    }
    let remember = matches!(
        symbol,
        Some(
            "activation_creature_property"
                | "move_after_other"
                | "reflexive_moved_fight_object"
                | "remember_object"
                | "paid_sacrifice_definition"
                | "return_ref"
                | "enchanted_operand"
                | "last_exiled"
                | "last_exiled_creature"
                | "remember_operand"
                | "owner_return"
                | "counter_choice"
                | "sacrificed_result"
                | "sacrificed_damage_source"
                | "manifest_result"
                | "created_result"
                | "selected_result"
                | "drafted_result"
                | "exiled_card_result"
                | "search_move_result"
                | "seek_move_result"
                | "qualified_members"
                | "payment_cost"
                | "announced_cost"
                | "announced_x"
        )
    );
    let first_child = out.len();
    match value {
        Value::Object(object) => {
            if symbol.is_some() && symbol != Some("in_filter") && !remember {
                out.push(Found {
                    at: value["at"].as_u64().unwrap_or(0),
                    order: u8::from(symbol == Some("target")),
                    path: path.clone(),
                    filter: context,
                });
                // This argument is a noun-domain annotation, not a second
                // occurrence of the actual filter's contextual references.
                if matches!(symbol, Some("other_reference" | "another_target_reference")) {
                    return;
                }
                if matches!(
                    crate::traverse::boundary(value),
                    crate::traverse::Boundary::Lexical
                        | crate::traverse::Boundary::Holder
                        | crate::traverse::Boundary::Reflexive
                        | crate::traverse::Boundary::Branch
                        | crate::traverse::Boundary::Conditional
                ) {
                    return;
                }
            }
            for (key, child) in object {
                path.push(Step::Key(key.clone()));
                collect(
                    child,
                    path,
                    out,
                    context.child(value, key),
                    variable_payment,
                );
                path.pop();
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                path.push(Step::Index(index));
                collect(child, path, out, context, variable_payment);
                path.pop();
            }
        }
        _ => {}
    }
    if symbol == Some("target") && out[first_child + 1..].iter().any(|child| child.order == 1) {
        // A target can introduce another target in its filter. Bind that
        // dependency before registering the containing specification. Ordinary
        // target ordering remains unchanged, including prior-target snapshots.
        let mut owner = out.remove(first_child);
        owner.at = out[first_child..]
            .iter()
            .map(|child| child.at)
            .max()
            .unwrap_or(owner.at)
            .max(owner.at);
        out.push(owner);
    }
    if remember {
        // A transparent wrapper uses the fully resolved inner value. Its last
        // operand's token sets the position; equal-position children bind first.
        let at = out[first_child..]
            .iter()
            .map(|child| child.at)
            .max()
            .unwrap_or(0)
            .max(value["at"].as_u64().unwrap_or(0));
        out.push(Found {
            at,
            order: 2,
            path: path.clone(),
            filter: context,
        });
    }
}

/// Introduce dependent specifications before looking for the referenced
/// target's surface occurrences inside them. Independent scopes retain the
/// previous order. These are already bounded IR values; cap this extra walk.
pub(super) fn surface_order(specs: &[Value]) -> Option<Vec<usize>> {
    if specs.len() > 64 {
        return None;
    }
    let mut pending: Vec<_> = specs.iter().map(|spec| (spec, 0_usize)).collect();
    let mut work = 32_768_usize;
    let mut dependent = false;
    while let Some((node, depth)) = pending.pop() {
        if depth > 128 || work == 0 {
            return None;
        }
        work -= 1;
        dependent |= matches!(node["kind"].as_str(), Some("Target" | "Targets"))
            || node.get("DifferentFromTarget").is_some()
            || node.get("SharesLimitWith").is_some();
        if matches!(
            crate::traverse::boundary(node),
            crate::traverse::Boundary::Ability
                | crate::traverse::Boundary::Holder
                | crate::traverse::Boundary::Lexical
                | crate::traverse::Boundary::Reflexive
        ) {
            continue;
        }
        match node {
            Value::Object(fields) => pending.extend(fields.values().map(|v| (v, depth + 1))),
            Value::Array(items) => pending.extend(items.iter().map(|v| (v, depth + 1))),
            _ => {}
        }
        if pending.len() > work {
            return None;
        }
    }
    let mut order: Vec<_> = (0..specs.len()).collect();
    if dependent {
        order.reverse();
    }
    Some(order)
}

pub(super) fn bind_link(root: &mut Value, path: &[Step], name: &str) -> Result<bool, String> {
    if !matches!(name, "last_exiled" | "last_exiled_creature") {
        return Ok(false);
    }
    let proven = name != "last_exiled_creature" || creature_link(root, path);
    let node = at_path(root, path).ok_or("linked card reference path")?;
    if node["args"][0] != serde_json::json!({"kind":"Source"}) {
        return Err("the implicit last-exiled link needs this ability's holder".into());
    }
    if !proven {
        return Err("a filtered last-exiled creature needs a proving condition".into());
    }
    *node = serde_json::json!({"kind":"Earlier","args":{"what":"LastExiled","by":null}});
    Ok(true)
}

/// A filtered last creature is the native LastExiled only when this branch
/// establishes that the last exiled card is itself a creature.
fn creature_link(root: &mut Value, path: &[Step]) -> bool {
    for depth in 0..path.len().saturating_sub(1) {
        if matches!(&path[depth], Step::Key(k) if k == "args")
            && matches!(&path[depth + 1], Step::Key(k) if k == "then")
            && at_path(root, &path[..depth]).is_some_and(|v| {
                v["kind"] == "Conditional" && earlier_creature(&v["args"]["condition"])
            })
        {
            return true;
        }
    }
    false
}

fn earlier_creature(condition: &Value) -> bool {
    let args = &condition["args"];
    match condition["kind"].as_str() {
        Some("AllOf") => args
            .as_array()
            .is_some_and(|v| v.iter().any(earlier_creature)),
        Some("AnyOf") => args
            .as_array()
            .is_some_and(|v| !v.is_empty() && v.iter().all(earlier_creature)),
        Some("ObjectMatches") => {
            args["object"]
                == serde_json::json!({"kind":"Earlier","args":{"what":"LastExiled","by":null}})
                && args["filter"] == serde_json::json!({"kind":"CardType","args":"Creature"})
        }
        _ => false,
    }
}

#[derive(Default)]
pub(super) struct Contexts(Vec<(Vec<Step>, Scope, Option<usize>)>);

impl Contexts {
    /// A newly written fighter cannot shadow the earlier creature it fights.
    /// Require the target's introduction to lie inside this actual actor operand.
    pub(super) fn target_actor(&self, actor: &[Step], slot: usize) -> Option<&Scope> {
        self.0.iter().rev().find_map(|(path, scope, candidate)| {
            (candidate == &Some(slot) && path.starts_with(actor)).then_some(scope)
        })
    }

    pub(super) fn remember(&mut self, path: &[Step], scope: &Scope, slot: usize) {
        self.0.push((path.to_vec(), scope.nested(), Some(slot)));
    }

    /// The predicate can name the previous object; following instructions can
    /// name the subject just tested. The predicate is not its own antecedent.
    pub(super) fn condition(&mut self, path: &[Step], root: &mut Value, scope: &Scope) {
        if let Some(mut parent) = object_test(path, root) {
            parent.extend([Step::Key("args".into()), Step::Key("filter".into())]);
            self.0.push((parent, scope.nested(), None));
        }
    }

    /// The attachment being moved is not the antecedent of its host pronoun.
    /// Retain the earlier context until the complete host operand is bound.
    pub(super) fn attachment_host(&mut self, path: &[Step], root: &mut Value, scope: &Scope) {
        for depth in 0..path.len() {
            if at_path(root, &path[..depth]).is_some_and(|v| v["kind"] == "Attach") {
                let mut host = path[..depth].to_vec();
                host.extend(["args", "host"].map(|key| Step::Key(key.into())));
                if !self.0.iter().any(|(parent, _, _)| *parent == host) {
                    self.0.push((host, scope.nested(), None));
                    // The token-host clause prepares its one-object choice in
                    // the immediately preceding instruction. Its pronoun has
                    // the same earlier context as the physical attachment.
                    if let Some(choice) = created_host_choice(root, &path[..depth]) {
                        self.0.push((choice, scope.nested(), None));
                    }
                }
            }
        }
    }

    pub(super) fn before<'a>(&'a self, path: &[Step], current: &'a Scope) -> &'a Scope {
        self.0
            .iter()
            .rev()
            .find(|(parent, _, _)| path.len() > parent.len() && path.starts_with(parent))
            .map_or(current, |(_, scope, _)| scope)
    }

    pub(super) fn candidate(&self, path: &[Step]) -> Option<Value> {
        let (_, _, slot) = self.0.iter().rev().find(|(parent, _, slot)| {
            slot.is_some() && path.len() > parent.len() && path.starts_with(parent)
        })?;
        let slot = slot.as_ref()?;
        Some(serde_json::json!({"kind":"Target","args":{"slot":slot,"index":0}}))
    }
}

/// A source acted on or explicitly tested becomes the nearer pronoun referent.
pub(super) fn acted_on(path: &[Step], root: &mut Value) -> bool {
    if path
        .iter()
        .any(|step| matches!(step, Step::Key(key) if key == "cost"))
    {
        return false;
    }
    let Some((_, parent)) = path.split_last() else {
        return false;
    };
    if at_path(root, parent).is_some_and(|node| node["kind"] == "One") {
        return true;
    }
    if path.len() >= 2
        && matches!(path.last(), Some(Step::Index(0)))
        && at_path(root, &path[..path.len() - 2])
            .is_some_and(|node| symbol_name(node) == Some("qualified_members"))
    {
        return true;
    }
    object_test(path, root).is_some()
}

fn object_test(path: &[Step], root: &mut Value) -> Option<Vec<Step>> {
    let (Step::Key(object), parent) = path.split_last()? else {
        return None;
    };
    let (Step::Key(args), outer) = parent.split_last()? else {
        return None;
    };
    (object == "object"
        && args == "args"
        && at_path(root, outer).is_some_and(|node| node["kind"] == "ObjectMatches"))
    .then(|| outer.to_vec())
}

/// Recognize only the grammar's adjacent created-host preparation, never an
/// unrelated earlier choice or a choice from another instruction scope.
fn created_host_choice(root: &mut Value, attach: &[Step]) -> Option<Vec<Step>> {
    let host = at_path(root, attach)?["args"]["host"].clone();
    if host["kind"] != "Result" || host["args"]["phase"] != "After" || host["args"]["index"] != 0 {
        return None;
    }
    for depth in (1..attach.len()).rev() {
        let Step::Index(index) = attach[depth] else {
            continue;
        };
        if !matches!(&attach[depth - 1], Step::Key(key) if key == "instructions") {
            return None;
        }
        let previous = index.checked_sub(1)?;
        let mut path = attach[..depth].to_vec();
        path.push(Step::Index(previous));
        let instruction = at_path(root, &path)?;
        let operation = &instruction["args"]["operation"];
        let result = &host["args"]["result"];
        if instruction["kind"] != "Effect"
            || operation["kind"] != "ChooseObjects"
            || symbol_name(&operation["args"]["objects"]) != Some("created_reference_single")
            || (symbol_name(result) != Some("prior") && *result != instruction["args"]["result"])
        {
            return None;
        }
        path.extend(["args", "operation", "args"].map(|key| Step::Key(key.into())));
        return Some(path);
    }
    None
}

#[cfg(test)]
mod token_attachment_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn created_host_context_requires_adjacent_choice_and_matching_host() {
        let choice = json!({"kind":"Effect","args":{"result":1,"operation":{
            "kind":"ChooseObjects","args":{"objects":{"$sym":"created_reference_single"}}
        }}});
        let attach = json!({"kind":"Effect","args":{"operation":{
            "kind":"Attach","args":{"host":{"kind":"Result","args":{
                "result":1,"phase":"After","index":0
            }}}
        }}});
        let path = [
            Step::Key("instructions".into()),
            Step::Index(1),
            Step::Key("args".into()),
            Step::Key("operation".into()),
        ];
        let mut root = json!({"instructions":[choice,attach]});
        assert!(created_host_choice(&mut root, &path).is_some());
        root["instructions"][1]["args"]["operation"]["args"]["host"]["args"]["result"] = json!(7);
        assert!(created_host_choice(&mut root, &path).is_none());
        root["instructions"][1]["args"]["operation"]["args"]["host"]["args"]["result"] = json!(1);
        root["instructions"][0]["args"]["operation"]["args"]["objects"] =
            json!({"kind":"Created","args":0});
        assert!(created_host_choice(&mut root, &path).is_none());
        let mut wrong_array = json!({"other":root["instructions"]});
        let mut wrong_path = path;
        wrong_path[0] = Step::Key("other".into());
        assert!(created_host_choice(&mut wrong_array, &wrong_path).is_none());
    }
}

#[cfg(test)]
mod nested_target_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nested_target_registers_before_its_containing_specification() {
        let value = json!({"$sym":"target","at":2,"args":[{
            "kind":"Object","filter":{"$sym":"target","at":7,"args":[{"kind":"Player"}]}
        }]});
        let mut found = Vec::new();
        collect(
            &value,
            &mut Vec::new(),
            &mut found,
            Context::Outside,
            &mut false,
        );
        found.sort_by_key(|item| (item.at, item.order));
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].at, 7);
        assert!(!found[0].path.is_empty());
        assert_eq!(found[1].at, 7);
        assert!(found[1].path.is_empty());
    }

    #[test]
    fn scheduling_does_not_merge_distinct_written_target_occurrences() {
        let mut root = json!({
            "left":{"$sym":"target","at":2,"args":[{"kind":"Player"}]},
            "right":{"$sym":"target","at":3,"args":[{"kind":"Player"}]}
        });
        let mut scope = Scope::default();
        let (mut introduced, mut introductions) = (Vec::new(), Vec::new());
        let mut contexts = Contexts::default();
        for key in ["left", "right"] {
            let item = Found {
                at: 7,
                order: 1,
                path: vec![Step::Key(key.into())],
                filter: Context::Outside,
            };
            introduce(
                &mut root,
                &mut scope,
                &item,
                &mut introduced,
                &mut introductions,
                &mut contexts,
            )
            .unwrap();
        }
        assert_eq!(scope.targets.len(), 2);
    }

    #[test]
    fn inverse_dependency_order_is_bounded_and_independent_targets_keep_their_order() {
        let independent = json!({"slot":0,"kind":"Player"});
        let dependent = json!({"slot":1,"filter":{"kind":"ControlledBy","args":{
            "kind":"One","args":{"kind":"Target","args":{"slot":0,"index":0}}
        }}});
        assert_eq!(
            surface_order(&[independent.clone(), dependent]),
            Some(vec![1, 0])
        );
        assert_eq!(
            surface_order(&[independent.clone(), independent.clone()]),
            Some(vec![0, 1])
        );
        assert!(surface_order(&vec![independent; 65]).is_none());
    }
}
