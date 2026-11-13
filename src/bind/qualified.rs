//! Noun-qualified references retain introduced objects across token creation.
use super::{Scope, json};
use crate::model::ir::ObjectKind;
use crate::model::mtg_types::{CardType, SubType};
use serde_json::Value;
use std::sync::Arc;

mod groups;
pub(super) use groups::reference as group_reference;

#[derive(Clone)]
pub(super) struct Referent {
    pub(super) other_target: Option<Arc<super::other::Target>>,
    reference: Value,
    kinds: Vec<ObjectKind>,
    types: Vec<CardType>,
    subtypes: Vec<SubType>,
    single: bool,
}

fn labels(value: &Value, types: &mut Vec<CardType>, subtypes: &mut Vec<SubType>) {
    match value["kind"].as_str() {
        Some("CardType") => {
            if let Ok(kind) = serde_json::from_value(value["args"].clone()) {
                types.push(kind);
            }
        }
        Some("Subtype") => {
            if let Ok(kind) = serde_json::from_value(value["args"].clone()) {
                subtypes.push(kind);
            }
        }
        Some("AllOf") => {
            if let Some(items) = value["args"].as_array() {
                for child in items {
                    labels(child, types, subtypes);
                }
            }
        }
        Some("AnyOf") => {
            let Some((first, rest)) = value["args"]
                .as_array()
                .and_then(|items| items.split_first())
            else {
                return;
            };
            let (mut common_types, mut common_subtypes) = (Vec::new(), Vec::new());
            labels(first, &mut common_types, &mut common_subtypes);
            for child in rest {
                let (mut types, mut subtypes) = (Vec::new(), Vec::new());
                labels(child, &mut types, &mut subtypes);
                common_types.retain(|kind| contains(&types, kind));
                common_subtypes.retain(|kind| contains(&subtypes, kind));
            }
            types.extend(common_types);
            subtypes.extend(common_subtypes);
        }
        _ => {}
    }
}

fn described(reference: Value, kinds: Vec<ObjectKind>, filter: &Value, single: bool) -> Referent {
    let mut entry = Referent {
        other_target: None,
        reference,
        kinds,
        types: Vec::new(),
        subtypes: Vec::new(),
        single,
    };
    labels(filter, &mut entry.types, &mut entry.subtypes);
    entry
}

pub(super) fn selector(event: &Value) -> Option<&Value> {
    if let Some(inner) = super::events::inner(event) {
        return selector(inner);
    }
    super::events::object_event(event)
        .or_else(|| event.pointer("/Player/objects"))
        .or_else(|| event.pointer("/Attack/attackers"))
}

pub(super) fn event(reference: Value, filter: &Value, moved: bool, spell: bool) -> Referent {
    let mut kinds = vec![if spell {
        ObjectKind::Spell
    } else {
        ObjectKind::Permanent
    }];
    if moved {
        kinds.push(ObjectKind::Card);
    }
    described(reference, kinds, filter, true)
}

fn push(scope: &mut Scope, entry: Referent) -> Result<(), String> {
    if scope.references.len() >= 4096 {
        return Err("qualified reference context limit".into());
    }
    scope.references.push(entry);
    Ok(())
}

pub(super) fn target(scope: &mut Scope, reference: Value, spec: &Value) -> Result<(), String> {
    let Some(domain) = spec["kind"].get("Object") else {
        return Ok(());
    };
    let kind: ObjectKind =
        serde_json::from_value(domain["kind"].clone()).map_err(|e| e.to_string())?;
    let single = ["Exactly", "UpTo", "AsManyAsCan"].iter().any(|tag| {
        let amount = &spec["count"][tag];
        amount["kind"] == "Constant" && amount["args"].as_u64().is_some_and(|n| n <= 1)
    });
    let mut entry = described(reference.clone(), vec![kind], &spec["filter"], single);
    entry.other_target = super::other::target(&reference, spec).map(Arc::new);
    push(scope, entry)
}

pub(super) fn created(
    scope: &mut Scope,
    operation: &Value,
    result: u64,
    single: bool,
) -> Result<(), String> {
    let mut entry = described(
        json!({"kind":"Result","args":{"result":result,"phase":"After","index":0}}),
        vec![ObjectKind::Permanent],
        &Value::Null,
        single,
    );
    if let Some(definition) = operation["args"]["tokens"][0]["recipe"].get("Defined") {
        entry.types =
            serde_json::from_value(definition["card_types"].clone()).map_err(|e| e.to_string())?;
        entry.subtypes =
            serde_json::from_value(definition["subtypes"].clone()).map_err(|e| e.to_string())?;
    } else if let Some(object) = operation["args"]["tokens"][0]["recipe"].pointer("/Copy/object")
        && let Some(source) = scope
            .references
            .iter()
            .rev()
            .find(|entry| entry.reference == *object)
    {
        entry.types = source.types.clone();
        entry.subtypes = source.subtypes.clone();
    }
    push(scope, entry)
}

fn contains<T: serde::Serialize>(items: &[T], item: &T) -> bool {
    let Ok(wanted) = serde_json::to_value(item) else {
        return false;
    };
    items
        .iter()
        .any(|candidate| serde_json::to_value(candidate).is_ok_and(|value| value == wanted))
}

fn matches(entry: &Referent, kind: &ObjectKind, filter: &Value) -> bool {
    if !entry.single || !entry.kinds.contains(kind) {
        return false;
    }
    match filter["kind"].as_str() {
        Some("Any") => true,
        Some("CardType") => serde_json::from_value::<CardType>(filter["args"].clone())
            .is_ok_and(|kind| contains(&entry.types, &kind)),
        Some("Subtype") => serde_json::from_value::<SubType>(filter["args"].clone())
            .is_ok_and(|kind| contains(&entry.subtypes, &kind)),
        _ => false,
    }
}

pub(super) fn known_scalar(
    scope: &Scope,
    reference: &Value,
    kind: &ObjectKind,
    filter: &Value,
) -> bool {
    scope
        .references
        .iter()
        .any(|entry| entry.reference == *reference && matches(entry, kind, filter))
}

pub(super) fn resolve(node: &Value, scope: &Scope, announced: bool) -> Result<Value, String> {
    let args = node["args"]
        .as_array()
        .ok_or("qualified reference operands")?;
    let kind: ObjectKind = serde_json::from_value(args[0].clone()).map_err(|e| e.to_string())?;
    // Preserve a positively described lexical focus. Otherwise an explicit
    // noun may recover one earlier compatible introduction across an unrelated
    // action; numeric proximity alone never breaks a genuine ambiguity.
    if scope.created_result.is_none() && args[1]["kind"] != "Subtype" {
        return preceding_typed(scope, &kind, &args[1], announced);
    }
    scope
        .references
        .iter()
        .rev()
        .find(|entry| {
            (!announced || entry.reference["kind"] != "Result") && matches(entry, &kind, &args[1])
        })
        .or_else(|| {
            scope.references.iter().rev().find(|entry| {
                entry.single
                    && (!announced || entry.reference["kind"] != "Result")
                    && entry.kinds.contains(&kind)
                    && entry.types.is_empty()
                    && entry.subtypes.is_empty()
            })
        })
        .map(|entry| entry.reference.clone())
        .ok_or("no introduced object matches the reference noun".into())
}

fn preceding_typed(
    scope: &Scope,
    kind: &ObjectKind,
    filter: &Value,
    announced: bool,
) -> Result<Value, String> {
    let eligible = |entry: &&Referent| {
        (!announced || entry.reference["kind"] != "Result") && matches(entry, kind, filter)
    };
    let legacy = scope.preceding();
    if filter["kind"] == "Any" && kind != &ObjectKind::Spell
        || scope
            .references
            .iter()
            .filter(eligible)
            .any(|entry| Some(&entry.reference) == legacy.as_ref())
    {
        return legacy.ok_or("no preceding object".into());
    }
    let mut unique: Option<&Value> = None;
    for entry in scope.references.iter().filter(eligible) {
        if unique.is_some_and(|reference| *reference != entry.reference) {
            return Err("qualified noun has multiple compatible antecedents".into());
        }
        unique = Some(&entry.reference);
    }
    unique
        .cloned()
        .or_else(|| (kind != &ObjectKind::Spell).then_some(legacy).flatten())
        .ok_or("no preceding object".into())
}

pub(super) fn card(scope: &Scope) -> Result<Value, String> {
    if let Some(reference) = super::exiled_card::card(scope) {
        return Ok(reference);
    }
    if let Some(reference) = super::drafted::card(scope) {
        return reference;
    }
    if scope.created_result.is_some() {
        if let Some(entry) = scope
            .references
            .iter()
            .rev()
            .find(|entry| entry.single && entry.kinds.contains(&ObjectKind::Card))
        {
            return Ok(entry.reference.clone());
        }
        return Err("token creation supplies no card referent".into());
    }
    if scope.objectless_player_event
        && scope.objects.len() == 1
        && scope.objects[0] == json!({"kind":"Source"})
        && scope.others.is_empty()
    {
        return Err("an objectless player event supplies no card antecedent".into());
    }
    scope
        .preceding()
        .ok_or("no named object for this reference".into())
}

pub(super) fn members(node: &Value, scope: &Scope) -> Result<Value, String> {
    let reference = node["args"]
        .get(0)
        .ok_or("qualified members need a reference")?;
    if let Some((result, single)) = scope.created_result
        && *reference == json!({"kind":"Result","args":{"result":result,"phase":"After","index":0}})
    {
        if !single {
            return Err("a qualified singular noun cannot select a token batch member".into());
        }
        return Ok(json!({"kind":"Created","args":result}));
    }
    Ok(json!({"kind":"One","args":reference}))
}

/// Supply one bounded extra spelling form from current operands. The ordinary
/// form remains available for literal templates; reparsing still proves meaning.
pub(super) fn spelling_forms(forms: Vec<Value>, specs: &[Value]) -> Vec<Value> {
    let mut descriptions = Vec::new();
    for spec in specs {
        if let Some(domain) = spec["kind"].get("Object")
            && let Ok(kind) = serde_json::from_value::<ObjectKind>(domain["kind"].clone())
        {
            descriptions.push(described(
                json!({"kind":"Target","args":{
                    "slot":spec["slot"],"index":0
                }}),
                vec![kind],
                &spec["filter"],
                true,
            ));
        }
    }
    let mut out = forms.clone();
    for form in forms {
        if out.len() >= 64 {
            break;
        }
        let mut copy = form.clone();
        let event = &form["args"]["event"];
        let mut local = descriptions.clone();
        if let Some(filter) = selector(event) {
            let moved =
                event.get("ZoneChange").is_some() || event.get("ZoneChangeFromElsewhere").is_some();
            local.push(self::event(
                json!({"kind":"EventSubject"}),
                filter,
                moved,
                false,
            ));
            if let Ok(current) = super::events::Frame::of_ability(&form)
                .reference(json!({"kind":"EventSubject"}), true)
                && current != json!({"kind":"EventSubject"})
            {
                local.push(described(current, vec![ObjectKind::Card], filter, true));
            }
        }
        let mut seen = false;
        if let Some(body) = copy.get_mut("args").and_then(|args| args.get_mut("body")) {
            qualify_print(body, &local, &mut seen);
        }
        if copy != form {
            out.push(copy);
        }
    }
    out
}

fn qualify_print(value: &mut Value, descriptions: &[Referent], seen: &mut bool) {
    if super::opens_scope(value) {
        return;
    }
    // A predicate can repeat its event's introduced noun before any token
    // creation. Qualify just that object operand; embedded controller/owner
    // references keep their independent possessive wording in the same form.
    if value["kind"] == "ObjectMatches"
        && let Some(object) = value.pointer_mut("/args/object")
        && let Some(entry) = descriptions.iter().find(|entry| entry.reference == *object)
        && let Some(spelling) = spelling(entry)
    {
        *object = spelling;
    }
    // A target introduction already contains its symbolic slot marker, so
    // only plain later uses match these descriptions. Reparsing proves which
    // noun is valid after intervening targets/actions.
    if (*seen || value["kind"] == "Target")
        && let Some(entry) = descriptions.iter().find(|entry| entry.reference == *value)
        && let Some(spelling) = spelling(entry)
    {
        *value = spelling;
        return;
    }
    match value {
        Value::Object(fields) => {
            let create = fields
                .get("kind")
                .is_some_and(|kind| kind == "CreateTokens");
            for child in fields.values_mut() {
                qualify_print(child, descriptions, seen);
            }
            *seen |= create;
        }
        Value::Array(items) => {
            for child in items {
                qualify_print(child, descriptions, seen);
            }
        }
        _ => {}
    }
}

fn spelling(entry: &Referent) -> Option<Value> {
    if !entry.single {
        return None;
    }
    if entry.kinds.first() == Some(&ObjectKind::Card) {
        return Some(json!({"$sym":"current_named_object","args":[]}));
    }
    let filter = if let Some(subtype) = entry.subtypes.first() {
        json!({"kind":"Subtype","args":subtype})
    } else {
        let kind = entry.types.first()?;
        json!({"kind":"CardType","args":kind})
    };
    Some(json!({"$sym":"qualified_object","args":[entry.kinds[0],filter]}))
}

/// A drafted card's type is unknown until the draft resolves. Do not infer a
/// creature/permanent from the spellbook or the creating ability's holder.
pub(super) fn drafted(scope: &mut Scope, result: u64, single: bool) -> Result<(), String> {
    push(
        scope,
        described(
            json!({"kind":"Result","args":{"result":result,"phase":"After","index":0}}),
            vec![ObjectKind::Card],
            &Value::Null,
            single,
        ),
    )
}

#[cfg(test)]
#[path = "qualified_tests.rs"]
mod tests;
