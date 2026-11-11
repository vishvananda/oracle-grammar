//! A sacrifice result has separate pre-action information and resulting cards.
use super::{Scope, Step, at_path, selections, transitions};
use crate::model::ir::{
    Cost, CostObjectView, CostPart, ObjectPhase, ObjectRef, ObjectSet, PaymentRef, ResultId,
};
use serde_json::{Value, json};

#[derive(Clone)]
pub(super) struct Information {
    result: u64,
    single: bool,
    object_mentions: usize,
    explicit_actor: Option<(u64, Value)>,
    resolution: Option<(ResultId, u32)>,
}

impl Information {
    pub(super) fn resolution(
        result: ResultId,
        component: u32,
        single: bool,
        object_mentions: usize,
    ) -> Self {
        Self {
            result: u64::from(result.0),
            single,
            object_mentions,
            explicit_actor: None,
            resolution: Some((result, component)),
        }
    }

    pub(super) fn is_resolution(&self) -> bool {
        self.resolution.is_some()
    }

    fn reference(&self) -> Value {
        if let Some((result, component)) = &self.resolution {
            serde_json::to_value(ObjectRef::Query {
                objects: Box::new(ObjectSet::CostObjects {
                    payment: PaymentRef::Resolution(result.clone()),
                    component: vec![*component],
                    view: CostObjectView::Inputs,
                    phase: ObjectPhase::Before,
                }),
                index: 0,
            })
            .expect("typed resolution sacrifice reference")
        } else {
            reference_at(self.result)
        }
    }
}

fn reference_at(result: u64) -> Value {
    json!({"kind":"Query","args":{"objects":{"kind":"Transition",
        "args":{"result":result,"phase":"Before"}},"index":0}})
}

pub(super) fn marker(
    root: &mut Value,
    scope: &mut Scope,
    path: &[Step],
    name: &str,
) -> Result<bool, String> {
    if name == "sacrificed_damage_source" {
        let component = own_source_component(root)?;
        if !damage_source_position(root, path) {
            return Err("a sacrificed damage source needs its immediate damage operation".into());
        }
        let node = at_path(root, path).ok_or("sacrificed damage source path")?;
        if node["args"][0] != json!({"kind":"Source"}) {
            return Err("a sacrificed damage source must name this activation's holder".into());
        }
        *node = paid_source(component);
        return Ok(true);
    }
    if name == "sacrificed_object" {
        let reference = named(root, path, scope)?;
        *at_path(root, path).ok_or("sacrificed object path")? = reference;
        return Ok(true);
    }
    if name != "sacrificed_result" {
        return Ok(false);
    }
    let result = transitions::effect_result(root, path)?;
    let node = at_path(root, path).ok_or("sacrifice result marker path")?;
    let wrapped = &node["args"][0];
    let operation = selections::action(wrapped);
    if operation["kind"] != "Sacrifice" {
        return Err("sacrifice result needs a Sacrifice operation".into());
    }
    let args = &operation["args"];
    let single_count = ["Exactly", "UpTo", "AsManyAsCan"].iter().any(|tag| {
        let amount = &args["choose"][tag];
        amount["kind"] == "Constant" && amount["args"].as_u64().is_some_and(|n| n <= 1)
    });
    // An explicit object already supplies its own current/LKI reference. A
    // selection introduces a new information subject; it does not change a
    // later physical action's object, attachment link or delayed capture.
    scope.sacrificed_result = (!args["choose"].is_null()).then_some(Information {
        result,
        single: args["players"]["kind"] == "One" && single_count,
        object_mentions: scope.objects.len(),
        explicit_actor: None,
        resolution: None,
    });
    Ok(true)
}

pub(super) fn reference(
    name: &str,
    reference: Value,
    path: &[Step],
    root: &Value,
    scope: &mut Scope,
) -> Result<Value, String> {
    // A written damage source governs its own bare possessive. A noun phrase
    // such as "that creature's power" can still name the earlier sacrifice.
    if matches!(name, "self" | "named_source")
        && matches!(path.last(), Some(Step::Key(key)) if key == "source")
        && path.len() >= 2
    {
        let mut parent = root;
        for step in &path[..path.len() - 2] {
            parent = match step {
                Step::Key(key) => parent.get(key),
                Step::Index(index) => parent.get(*index),
            }
            .ok_or("sacrifice actor path")?;
        }
        if parent["kind"] == "DealDamage" {
            let result = transitions::effect_result(root, path)?;
            if let Some(info) = &mut scope.sacrificed_result {
                info.explicit_actor = Some((result, reference.clone()));
            }
        }
    }
    // In the paid-holder reading, the damage dealer also supplies its own
    // possessive property read. This leaves the ordinary Source reading intact.
    if reference == json!({"kind":"Source"})
        && matches!(
            name,
            "it" | "current_it" | "defined_it" | "named_object" | "current_named_object"
        )
        && let Some(source) = paid_damage_property_source(root, path)
    {
        return Ok(source);
    }
    let Some(info) = &scope.sacrificed_result else {
        return Ok(reference);
    };
    if let Some((result, _)) = &info.resolution
        && !super::resolution_receipts::paid_guard(root, path, result.0)
    {
        return Ok(reference);
    }
    if !matches!(
        name,
        "it" | "current_it"
            | "that_object"
            | "named_object"
            | "current_named_object"
            | "qualified_object"
            | "defined_it"
            | "defined_object"
            | "defined_named_object"
    ) || scope.objects.len() != info.object_mentions
        || reference["kind"] == "Candidate"
        || !(scope.preceding().as_ref() == Some(&reference)
            || scope.object().as_ref() == Some(&reference)
            || scope.pronoun.as_ref() == Some(&reference))
    {
        return Ok(reference);
    }
    let mut parent = root;
    for step in path {
        if parent["kind"] == "ObjectProperty" {
            if matches!(name, "it" | "current_it" | "defined_it")
                && let Some((result, actor)) = &info.explicit_actor
                && transitions::effect_result(root, path).ok() == Some(*result)
            {
                return Ok(actor.clone());
            }
            if !info.single {
                return Err("a plural sacrifice result does not establish one object".into());
            }
            return Ok(info.reference());
        }
        parent = match step {
            Step::Key(key) => parent.get(key),
            Step::Index(index) => parent.get(*index),
        }
        .ok_or("sacrifice reference path")?;
    }
    Ok(reference)
}

/// "The sacrificed <noun>" names the one object of the current resolution
/// sacrifice; a paid sacrifice exists only in its positive PaymentChosen branch.
fn named(root: &Value, path: &[Step], scope: &Scope) -> Result<Value, String> {
    let info = scope
        .sacrificed_result
        .as_ref()
        .ok_or("no resolution sacrifice to name")?;
    if let Some((result, _)) = &info.resolution
        && !super::resolution_receipts::paid_guard(root, path, result.0)
    {
        return Err("the sacrificed object is named outside its paid branch".into());
    }
    if !info.single {
        return Err("a plural sacrifice result does not establish one object".into());
    }
    Ok(info.reference())
}

/// A deferred where-capture may freeze only this proved immutable paid view.
/// Other object-property candidates retain their ordinary inline contract.
pub(super) fn snapshot_definition(
    root: &Value,
    path: &[Step],
    scope: &Scope,
    value: Value,
) -> Result<Value, String> {
    let info = scope
        .sacrificed_result
        .as_ref()
        .ok_or("no resolution sacrifice for property snapshot")?;
    let (result, _) = info
        .resolution
        .as_ref()
        .ok_or("property snapshot is not a resolution payment")?;
    if !info.single
        || !super::resolution_receipts::paid_guard(root, path, result.0)
        || value["kind"] != "ObjectProperty"
        || value["args"]["information"] != "CurrentOrLastKnown"
        || value["args"]["object"] != info.reference()
    {
        return Err("property snapshot needs the exact paid Inputs/Before receipt".into());
    }
    Ok(value)
}

/// A separate typed source-sacrifice identity, independent of the older generic
/// first-sacrifice index. Alternative/nested payments need their own contract.
fn own_source_component(root: &Value) -> Result<usize, String> {
    let ability = super::presentation::core(root);
    if ability["kind"] != "Activated" {
        return Err("a sacrificed holder source needs an activation".into());
    }
    let mut cost = ability["args"]["cost"].clone();
    super::filters::strip(&mut cost);
    let cost: Cost = serde_json::from_value(cost)
        .map_err(|why| format!("sacrificed holder activation cost: {why}"))?;
    let mut components = cost.parts.iter().enumerate().filter_map(|(index, part)| {
        matches!(
            part,
            CostPart::Sacrifice {
                objects: ObjectSet::One(ObjectRef::Source),
                choose: None,
            }
        )
        .then_some(index)
    });
    let component = components
        .next()
        .ok_or("this activation does not sacrifice its holder")?;
    if components.next().is_some() {
        return Err("multiple holder-sacrifice components need an explicit identity".into());
    }
    Ok(component)
}

fn paid_source(component: usize) -> Value {
    json!({"kind":"Query","args":{"objects":{"kind":"CostObjects","args":{
        "payment":"Activation","component":[component],"view":"Inputs","phase":"Before"
    }},"index":0}})
}

fn damage_source_position(root: &Value, path: &[Step]) -> bool {
    if !matches!(path.last(), Some(Step::Key(key)) if key == "source") || path.len() < 2 {
        return false;
    }
    let mut parent = root;
    for step in &path[..path.len() - 2] {
        let child = match step {
            Step::Key(key) => parent.get(key),
            Step::Index(index) => parent.get(*index),
        };
        let Some(child) = child else {
            return false;
        };
        parent = child;
    }
    parent["kind"] == "DealDamage"
}

fn paid_damage_property_source(root: &Value, path: &[Step]) -> Option<Value> {
    let mut parent = root;
    let mut dealer = None;
    let mut property = false;
    for step in path {
        if parent["kind"] == "DealDamage" {
            let source = &parent["args"]["source"];
            let paid = &source["args"]["objects"];
            if source["kind"] == "Query"
                && source["args"]["index"] == 0
                && paid["kind"] == "CostObjects"
                && paid["args"]["payment"] == "Activation"
                && paid["args"]["view"] == "Inputs"
                && paid["args"]["phase"] == "Before"
            {
                dealer = Some(source.clone());
            }
        }
        property |= parent["kind"] == "ObjectProperty";
        parent = match step {
            Step::Key(key) => parent.get(key),
            Step::Index(index) => parent.get(*index),
        }?;
    }
    property.then_some(dealer).flatten()
}

#[cfg(test)]
mod paid_holder_tests {
    use super::*;

    fn activation(parts: Value) -> Value {
        json!({"kind":"Activated","args":{"cost":{"parts":parts}}})
    }
    fn sacrifice(reference: &str) -> Value {
        json!({"Sacrifice":{"objects":{"kind":"One","args":{"kind":reference}},"choose":null}})
    }

    #[test]
    fn source_component_is_not_the_first_other_sacrifice() {
        let root = activation(json!([
            sacrifice("GrantingObject"),
            "TapSymbol",
            sacrifice("Source")
        ]));
        assert_eq!(own_source_component(&root).unwrap(), 2);
        assert!(own_source_component(&activation(json!([sacrifice("GrantingObject")]))).is_err());
        assert!(
            own_source_component(&activation(json!([
                sacrifice("Source"),
                sacrifice("Source")
            ])))
            .is_err()
        );
    }

    #[test]
    fn resolution_or_optional_nested_payment_does_not_supply_an_activation_source() {
        let mut root = activation(json!([sacrifice("Source")]));
        root["kind"] = json!("Spell");
        assert!(own_source_component(&root).is_err());
        let mut root = activation(json!([{"Mana":[{"Generic":1}]}]));
        root["args"]["body"] = json!({"instructions":[{"kind":"Effect","args":{
            "result":0,"operation":{"kind":"PayCost","args":{
                "player":{"kind":"AbilityController"},"optional":true,
                "cost":{"parts":[sacrifice("Source")]}
            }}
        }}]});
        assert!(own_source_component(&root).is_err());
    }

    #[test]
    fn annotated_counter_recipients_keep_the_separate_source_sacrifice() {
        let root = activation(json!([
            {"RemoveCounters":{"removals":[{"player":{"kind":"AbilityController"},
                "recipients":{"kind":"Objects","args":{"$sym":"distributive_pump_subject","args":[{
                    "kind":"Zone","args":{"scope":{"Shared":"Battlefield"},
                        "kind":"Permanent","filter":{"kind":"Any"}}}]}},
                "kind":{"Named":"OilCounter"},
                "amount":{"AmongThem":{"Exactly":{"kind":"Constant","args":10}}},
                "selection":null}]}},
            sacrifice("Source")
        ]));
        assert_eq!(own_source_component(&root).unwrap(), 1);
    }
}
