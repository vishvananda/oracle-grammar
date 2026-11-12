//! An unqualified `other` contrasts with a prior compatible target, otherwise
//! with the holder. Ordinary event/pronoun focus does not establish that contrast.
use super::{Scope, Step, at_path, symbol_name};
use crate::model::ir::{DamageSubject, ObjectDomain, ObjectRef};
use serde_json::{Value, json};

// Shared through Arc in Referent: snapshots never clone this filter payload.
pub(super) struct Target {
    reference: ObjectRef,
    filter: Value,
    single: bool,
}

pub(super) fn target(reference: &Value, spec: &Value) -> Option<Target> {
    let domain: ObjectDomain = serde_json::from_value(spec["kind"]["Object"].clone()).ok()?;
    if domain != ObjectDomain::permanent() {
        return None;
    }
    let single = ["Exactly", "UpTo", "AsManyAsCan"].iter().any(|tag| {
        let count = &spec["count"][tag];
        count["kind"] == "Constant" && count["args"] == 1
    });
    Some(Target {
        reference: serde_json::from_value(reference.clone()).ok()?,
        filter: spec["filter"].clone(),
        single,
    })
}

/// Only positively stated noun domains participate; controller/color modifiers
/// do not change which earlier object the contrast names. Disjunctions require
/// proof in every target-filter branch rather than treating one arm as a fact.
fn implies(filter: &Value, noun: &Value) -> bool {
    if filter == noun {
        return true;
    }
    let Some(parts) = filter["args"].as_array() else {
        return false;
    };
    match filter["kind"].as_str() {
        Some("AllOf") => parts.iter().any(|part| implies(part, noun)),
        Some("AnyOf") => !parts.is_empty() && parts.iter().all(|part| implies(part, noun)),
        _ => false,
    }
}

fn compatible(filter: &Value, domain: &Value) -> bool {
    match domain["kind"].as_str() {
        Some("CardType" | "Subtype") => implies(filter, domain),
        Some("AllOf") => domain["args"]
            .as_array()
            .is_some_and(|parts| parts.iter().all(|part| compatible(filter, part))),
        Some("AnyOf") => domain["args"]
            .as_array()
            .is_some_and(|parts| parts.iter().any(|part| compatible(filter, part))),
        _ => true,
    }
}

pub(super) fn resolve_marker(marker: &str, scope: &Scope, domain: &Value) -> Result<Value, String> {
    match marker {
        "another_target_reference" => another_target(scope, domain),
        "other_reference" => resolve(scope, domain),
        _ => Err("unknown contextual-other marker".into()),
    }
}

pub(super) fn resolve(scope: &Scope, domain: &Value) -> Result<Value, String> {
    // An explicitly acted-on holder supersedes an earlier target contrast.
    // Event focus is deliberately absent from this test.
    if scope.object() == Some(json!({"kind":"Source"})) {
        return Ok(holder_contrast(scope, domain));
    }
    let mut eligible = scope.references.iter().filter_map(|entry| {
        let target = entry.other_target.as_ref()?;
        let ObjectRef::Target { slot, .. } = &target.reference else {
            return None;
        };
        // Independent generated triggers cannot acquire an outer target as an
        // implicit contrast; ordinary lexical continuations retain their floor.
        (usize::from(slot.0) >= scope.target_base && compatible(&target.filter, domain))
            .then_some(target)
    });
    let Some(target) = eligible.next() else {
        return Ok(holder_contrast(scope, domain));
    };
    if !target.single || eligible.next().is_some() {
        return Err("other needs one compatible prior target, not a target group".into());
    }
    serde_json::to_value(&target.reference).map_err(|error| error.to_string())
}

/// A directly preceding target establishes an explicit contrast even when its
/// subtype/union does not prove the following noun domain. Event focus does not.
pub(super) fn another_target(scope: &Scope, domain: &Value) -> Result<Value, String> {
    if let Some(reference) = scope.object() {
        if reference["kind"] == "Target" {
            let typed: ObjectRef =
                serde_json::from_value(reference.clone()).map_err(|error| error.to_string())?;
            let ObjectRef::Target { slot, index } = typed else {
                return Err("another target needs a target reference".into());
            };
            let target = scope
                .references
                .iter()
                .filter_map(|entry| entry.other_target.as_ref())
                .find(|target| {
                    matches!(&target.reference, ObjectRef::Target { slot: prior, index: at }
                        if prior.0 == slot.0 && *at == index)
                });
            if target.is_some_and(|target| target.single)
                && usize::from(slot.0) >= scope.target_base
            {
                return Ok(reference);
            }
            return Err("another target needs a singular prior battlefield target".into());
        }
        // Native normalization surfaces the distinguished Damage role as
        // EventSubject. A proof for one role never licenses its sibling.
        if scope
            .other_source_damage
            .is_some_and(|role| reference == json!({"kind":"Damage","args":role}))
        {
            return Ok(json!({"kind":"EventSubject"}));
        }
        if scope.other_source_event
            && matches!(
                reference["kind"].as_str(),
                Some("EventSubject" | "EventDestination")
            )
        {
            return Ok(reference);
        }
    }
    resolve(scope, domain)
}

/// Retain the existing self-event spelling only with a source-identity proof.
/// In particular a general land/creature event cannot supply this contrast.
pub(super) fn source_event(event: &Value) -> bool {
    if let Some(inner) = super::events::inner(event) {
        return source_event(inner);
    }
    if let Some(cases) = event["AnyOf"].as_array() {
        return !cases.is_empty() && cases.iter().all(source_event);
    }
    if event["Batch"]["per"] == "Subject" {
        return source_event(&event["Batch"]["event"]);
    }
    let source = json!({"kind":"SameObject","args":{"kind":"Source"}});
    if let Some(objects) = super::events::object_event(event) {
        return objects == &source;
    }
    if event["Counters"]["objects"] == source || event["Attack"]["attackers"] == source {
        return true;
    }
    source_damage(event).is_some()
}

/// Direct damage focus is a typed Damage role, not an EventSubject spelling.
/// Prove exactly the distinguished role against the holder before surfacing it.
pub(super) fn source_damage(event: &Value) -> Option<DamageSubject> {
    if let Some(inner) = super::events::inner(event) {
        return source_damage(inner);
    }
    let source = json!({"kind":"SameObject","args":{"kind":"Source"}});
    match event["Damage"]["subject"].as_str() {
        Some("Source") if event["Damage"]["sources"] == source => Some(DamageSubject::Source),
        Some("Recipient") if event["Damage"]["recipients"]["Objects"] == source => {
            Some(DamageSubject::Recipient)
        }
        _ => None,
    }
}

fn source(scope: &Scope) -> Value {
    // Preserve the existing holder spelling's proven source-departure role.
    json!({"kind":if scope.leaving {"EventSubject"} else {"Source"}})
}

fn creature() -> Value {
    json!({"kind":"CardType","args":"Creature"})
}

/// A noncreature attachment is never among "other creatures". An Aura's
/// static contrasts them with the creature it enchants; an attachment's
/// trigger with the creature its event dealt damage to.
fn holder_contrast(scope: &Scope, domain: &Value) -> Value {
    if implies(domain, &creature()) {
        if scope.attached_holder {
            return json!({"kind":"AttachedTo","args":{"kind":"Source"}});
        }
        if let Some(counterpart) = damaged_counterpart(scope) {
            return counterpart;
        }
    }
    source(scope)
}

/// The creature a noncreature attachment's damage event names besides its subject.
fn damaged_counterpart(scope: &Scope) -> Option<Value> {
    let recipient = json!({"kind":"Damage","args":"Recipient"});
    (scope.attached_trigger && scope.others.contains(&recipient)).then_some(recipient)
}

/// `each other creature`, spelled with a pronoun or a learned Source fallback:
/// in a noncreature attachment's damage trigger the contrast is the damaged
/// creature, as in Kusari-Gama. A distributive Pump count keeps its member.
pub(super) fn counterpart_marker(
    root: &mut Value,
    scope: &Scope,
    path: &[Step],
    name: &str,
) -> Result<bool, String> {
    let Some(counterpart) = damaged_counterpart(scope) else {
        return Ok(false);
    };
    let marker = match (name, path) {
        ("it", [annotation @ .., Step::Key(args), Step::Index(0)]) if args == "args" => annotation,
        ("pump_other_ref", _) => path,
        _ => return Ok(false),
    };
    if marker.len() > 128 {
        return Ok(false);
    }
    let mut node: &Value = root;
    let mut domain = None;
    for step in marker {
        match node["kind"].as_str() {
            Some("AllOf") => domain = Some(node),
            Some("Pump") => return Ok(false),
            _ => {}
        }
        node = match step {
            Step::Key(key) => &node[key.as_str()],
            Step::Index(index) => &node[*index],
        };
    }
    let constant = name != "it" && super::pump_counts::constant_marker(node);
    if !(constant || name == "it" && symbol_name(node) == Some("pump_other_ref"))
        || !domain.is_some_and(|filter| implies(filter, &creature()))
    {
        return Ok(false);
    }
    *at_path(root, path).ok_or("damaged counterpart path")? = counterpart;
    Ok(true)
}

/// What a card attaches to when it is not a creature itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Attachment {
    #[default]
    None,
    /// Enchants creatures, without a bestow creature form.
    Aura,
    /// Equips creatures, without a reconfigure creature form.
    Equipment,
}

pub(crate) fn attachment(abilities: &Value) -> Attachment {
    let Some(items) = abilities.as_array() else {
        return Attachment::None;
    };
    let mechanic = |name: &str| {
        items
            .iter()
            .any(|item| item["kind"] == "Mechanic" && item["args"]["mechanic"] == name)
    };
    if items.iter().any(|item| {
        item["kind"] == "Enchant" && implies(&item["args"]["hosts"]["Permanents"], &creature())
    }) && !mechanic("Bestow")
    {
        return Attachment::Aura;
    }
    if items
        .iter()
        .any(|item| item["kind"] == "Activated" && item["args"]["origin"] == "Equip")
        && !mechanic("Reconfigure")
    {
        return Attachment::Equipment;
    }
    Attachment::None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creature() -> Value {
        json!({"kind":"CardType","args":"Creature"})
    }

    fn specification(count: Value, filter: Value) -> Value {
        json!({"kind":{"Object":{"kind":"Permanent","zones":["Battlefield"]}},
            "count":count,"filter":filter})
    }

    fn with_target(scope: &mut Scope, slot: usize, count: Value, filter: Value) {
        let reference = json!({"kind":"Target","args":{"slot":slot,"index":0}});
        super::super::qualified::target(scope, reference, &specification(count, filter)).unwrap();
    }

    #[test]
    fn another_target_keeps_subtype_and_union_targets_without_domain_proof() {
        for filter in [
            json!({"kind":"Subtype","args":"Dinosaur"}),
            json!({"kind":"AnyOf","args":[creature(),
                {"kind":"CardType","args":"Artifact"}]}),
        ] {
            let mut scope = Scope::default();
            with_target(
                &mut scope,
                0,
                json!({"Exactly":{"kind":"Constant","args":1}}),
                filter,
            );
            scope
                .objects
                .push(json!({"kind":"Target","args":{"slot":0,"index":0}}));
            assert_eq!(
                another_target(&scope, &creature()).unwrap()["kind"],
                "Target"
            );
            assert_eq!(resolve(&scope, &creature()).unwrap()["kind"], "Source");
        }
    }

    #[test]
    fn another_target_rejects_groups_and_does_not_capture_unrelated_events() {
        let mut scope = Scope::default();
        with_target(
            &mut scope,
            0,
            json!({"Exactly":{"kind":"Constant","args":2}}),
            creature(),
        );
        scope
            .objects
            .push(json!({"kind":"Target","args":{"slot":0,"index":0}}));
        assert!(another_target(&scope, &creature()).is_err());
        let mut scope = Scope::default();
        scope.objects.push(json!({"kind":"EventSubject"}));
        assert_eq!(
            another_target(&scope, &creature()).unwrap()["kind"],
            "Source"
        );
        scope.other_source_event = true;
        assert_eq!(
            another_target(&scope, &creature()).unwrap()["kind"],
            "EventSubject"
        );
    }

    #[test]
    fn only_the_proven_damage_role_can_surface_as_event_subject() {
        let mut scope = Scope {
            other_source_event: true,
            other_source_damage: Some(DamageSubject::Source),
            ..Scope::default()
        };
        scope.objects.push(json!({"kind":"Damage","args":"Source"}));
        assert_eq!(
            another_target(&scope, &creature()).unwrap()["kind"],
            "EventSubject"
        );
        scope
            .objects
            .push(json!({"kind":"Damage","args":"Recipient"}));
        assert_eq!(
            another_target(&scope, &creature()).unwrap()["kind"],
            "Source"
        );
        scope.other_source_damage = Some(DamageSubject::Recipient);
        assert_eq!(
            another_target(&scope, &creature()).unwrap()["kind"],
            "EventSubject"
        );
    }

    #[test]
    fn source_event_proof_requires_every_alternative_and_the_distinguished_role() {
        let objects = json!({"kind":"SameObject","args":{"kind":"Source"}});
        let own = json!({"Object":{"event":"Attacks","objects":objects}});
        assert!(source_event(&json!({"AnyOf":[own.clone(),own.clone()]})));
        assert!(!source_event(
            &json!({"AnyOf":[own,{"Object":{"event":"Attacks","objects":creature()}}]})
        ));
        assert!(!source_event(&json!({"AnyOf":[]})));
        let damage = json!({"Damage":{"subject":"Source","sources":objects,
            "recipients":{"Objects":creature()}}});
        assert!(source_event(&damage));
        assert_eq!(source_damage(&damage), Some(DamageSubject::Source));
        let mut recipient = damage;
        recipient["Damage"]["subject"] = json!("Recipient");
        assert!(!source_event(&recipient));
        assert_eq!(source_damage(&recipient), None);
        assert!(source_event(&json!({"Batch":{"per":"Subject","event":{
            "Counters":{"objects":objects}}}})));
    }

    #[test]
    fn noun_metadata_does_not_collect_its_filter_references_twice() {
        for marker in ["other_reference", "another_target_reference"] {
            let source = json!({"$sym":"self","args":[],"at":20});
            let filter = json!({"kind":"AllOf","args":[creature(),
            {"kind":"MadeBy","args":source}]});
            let value = json!({"kind":"AllOf","args":[
                {"kind":"Not","args":{"kind":"SameObject","args":{"$sym":marker,"args":[filter],"at":10}}}, filter
            ]});
            let mut found = Vec::new();
            super::super::references::collect(
                &value,
                &mut Vec::new(),
                &mut found,
                super::super::filters::Context::Outside,
                &mut false,
            );
            assert_eq!(found.len(), 2);
            assert_eq!(found.iter().filter(|item| item.at == 20).count(), 1);
        }
    }

    #[test]
    fn event_focus_is_not_an_other_contrast() {
        let mut scope = Scope::default();
        scope.objects.push(json!({"kind":"EventSubject"}));
        assert_eq!(
            resolve(&scope, &creature()).unwrap(),
            json!({"kind":"Source"})
        );
    }

    #[test]
    fn singular_target_is_eligible_and_groups_refuse() {
        let mut scope = Scope::default();
        with_target(
            &mut scope,
            0,
            json!({"Exactly":{"kind":"Constant","args":1}}),
            creature(),
        );
        assert_eq!(resolve(&scope, &creature()).unwrap()["kind"], "Target");
        with_target(
            &mut scope,
            1,
            json!({"Exactly":{"kind":"Constant","args":1}}),
            creature(),
        );
        assert!(resolve(&scope, &creature()).is_err());
        let mut group = Scope::default();
        with_target(
            &mut group,
            0,
            json!({"Exactly":{"kind":"Constant","args":2}}),
            creature(),
        );
        assert!(resolve(&group, &creature()).is_err());
    }

    #[test]
    fn unrelated_target_domains_keep_the_holder_contrast() {
        let mut scope = Scope::default();
        with_target(
            &mut scope,
            0,
            json!({"Exactly":{"kind":"Constant","args":1}}),
            json!({"kind":"CardType","args":"Land"}),
        );
        assert_eq!(
            resolve(&scope, &creature()).unwrap(),
            json!({"kind":"Source"})
        );
    }

    #[test]
    fn disjunctions_do_not_prove_a_noun_or_capture_an_outer_target() {
        assert!(!compatible(
            &json!({"kind":"AnyOf","args":[creature(),
            {"kind":"CardType","args":"Artifact"}]}),
            &creature()
        ));
        let mut scope = Scope::default();
        with_target(
            &mut scope,
            0,
            json!({"Exactly":{"kind":"Constant","args":1}}),
            creature(),
        );
        scope.target_base = 1;
        assert_eq!(
            resolve(&scope, &creature()).unwrap(),
            json!({"kind":"Source"})
        );
    }
}
