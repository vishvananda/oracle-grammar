//! A local selected sacrifice is an information subject, never general action focus.
use super::{Step, symbol_name};
use crate::model::ir::{
    CostObjectView, Count, EventZone, Expr, ObjectKind, ObjectPhase, ObjectRef, ObjectSet,
    Operation, PaymentRef, PlayerSet, ResultId, ZoneScope,
};
use serde_json::{Value, json};

pub(super) const ACTOR: &str = "selected_sacrifice_actor";
/// A required sacrifice written without "if you do", whose damage must read it.
const MEASURED: &str = "measured_sacrifice";
const MEASURED_WORK: usize = 1 << 20;
const MEASURED_DEPTH: usize = 256;

fn operation(item: &Value) -> Option<&Value> {
    (item["kind"] == "Effect").then_some(&item["args"]["operation"])
}
fn action(value: &Value) -> &Value {
    if symbol_name(value) == Some("sacrificed_result") {
        &value["args"][0]
    } else {
        value
    }
}
fn bounded(value: &Value, depth: usize, work: &mut usize) -> bool {
    if depth > 32 || *work == 0 {
        return false;
    }
    *work -= 1;
    match value {
        Value::Object(fields) => {
            fields.len() <= 64
                && fields
                    .iter()
                    .all(|(k, v)| k.len() <= 1024 && bounded(v, depth + 1, work))
        }
        Value::Array(items) => {
            items.len() <= 1024 && items.iter().all(|v| bounded(v, depth + 1, work))
        }
        Value::String(s) => s.len() <= 4096,
        _ => true,
    }
}
fn filter(item: &Value) -> Option<&Value> {
    let op = action(operation(item)?);
    let a = &op["args"];
    let one = ["Exactly", "UpTo", "AsManyAsCan"]
        .iter()
        .any(|tag| a["choose"][tag] == json!({"kind":"Constant","args":1}));
    (op["kind"] == "Sacrifice"
        && a["players"]["kind"] == "One"
        && one
        && a["objects"]["kind"] == "Zone"
        && a["objects"]["args"]["kind"] == "Permanent"
        && a["objects"]["args"]["scope"] == json!({"Shared":"Battlefield"}))
    .then_some(&a["objects"]["args"]["filter"])
}
fn implies(filter: &Value, noun: &Value) -> bool {
    if noun == &json!({"kind":"Any"}) {
        return true;
    }
    if noun["kind"] != "CardType" || !noun["args"].is_string() {
        return false;
    }
    if filter == noun {
        return true;
    }
    match filter["kind"].as_str() {
        Some("AllOf") => filter["args"]
            .as_array()
            .is_some_and(|parts| parts.iter().any(|part| implies(part, noun))),
        Some("AnyOf") => filter["args"]
            .as_array()
            .is_some_and(|parts| !parts.is_empty() && parts.iter().all(|part| implies(part, noun))),
        _ => false,
    }
}
/// None is an explicit/independent actor; false is an unproved implicit candidate.
fn lexical(value: &Value, filter: &Value) -> Option<(bool, u64)> {
    let mut node = value;
    for _ in 0..4 {
        if matches!(
            symbol_name(node),
            Some("move_after_actor" | "remember_operand")
        ) {
            let [inner] = node["args"].as_array()?.as_slice() else {
                return Some((false, 0));
            };
            node = inner;
        } else {
            break;
        }
    }
    let at = node["at"].as_u64().unwrap_or(0);
    match symbol_name(node)? {
        "member_count_it" => {
            let [inner] = node["args"].as_array()?.as_slice() else {
                return Some((false, at));
            };
            Some((
                symbol_name(inner) == Some("it")
                    && inner["args"].as_array().is_some_and(Vec::is_empty),
                at,
            ))
        }
        "qualified_object" => Some((
            node["args"]
                .as_array()
                .is_some_and(|a| a.len() == 2 && a[0] == "Permanent" && implies(filter, &a[1])),
            at,
        )),
        // Raw legacy markers lack the literal-It wrapper. Never guess their word
        // provenance or allow a fallback Source to survive beside a proved route.
        "it"
        | "current_it"
        | "named_object"
        | "current_named_object"
        | "that_object"
        | "defined_it"
        | "defined_object"
        | "defined_named_object"
        | "loop_card_it"
        | "literal_damage_itself"
        | "move_after_actor"
        | "remember_operand" => Some((false, at)),
        _ => None,
    }
}
fn prepare_list(items: &mut [Value]) -> Result<(), String> {
    if items.len() > 1024 {
        return Err("sacrifice actor instruction limit".into());
    }
    for index in 1..items.len() {
        let (before, after) = items.split_at_mut(index);
        let previous = &mut before[index - 1];
        let current = &mut after[0];
        let Some(selected) = filter(previous) else {
            continue;
        };
        let Some(op) = operation(current) else {
            continue;
        };
        if op["kind"] != "DealDamage" {
            continue;
        }
        if !bounded(selected, 0, &mut 4096) || !bounded(&op["args"]["source"], 0, &mut 4096) {
            return Err("sacrifice actor proof work/depth limit".into());
        }
        let Some((proved, at)) = lexical(&op["args"]["source"], selected) else {
            continue;
        };
        current["args"]["operation"]["args"]["source"] =
            json!({"$sym":ACTOR,"args":[proved],"at":at});
        if proved && symbol_name(&previous["args"]["operation"]) != Some("sacrificed_result") {
            let at = previous["args"]["result"]["at"].as_u64().unwrap_or(0);
            let operation = previous["args"]["operation"].take();
            previous["args"]["operation"] =
                json!({"$sym":"sacrificed_result","args":[operation],"at":at});
        }
    }
    Ok(())
}
/// Visit only own programs and explicitly scoped mode containers. No Optional,
/// branch body, holder, loop or generated callback is traversed.
fn prepare_owner(
    value: &mut Value,
    own_body: bool,
    depth: usize,
    work: &mut usize,
) -> Result<(), String> {
    if depth > 32 || *work == 0 {
        return Err("sacrifice actor scope work/depth limit".into());
    }
    *work -= 1;
    if symbol_name(value) == Some("scope") {
        let [inner] = value["args"]
            .as_array_mut()
            .ok_or("sacrifice actor scope args")?
            .as_mut_slice()
        else {
            return Err("sacrifice actor scope arity".into());
        };
        return prepare_owner(inner, own_body, depth + 1, work);
    }
    if value["kind"] == "CostBranch" {
        for branch in ["paid", "unpaid"] {
            prepare_owner(&mut value["args"][branch], false, depth + 1, work)?;
        }
        return Ok(());
    }
    let mode = value.get("weight").is_some() && value.get("announcement").is_some();
    if !mode
        && !matches!(
            value["kind"].as_str(),
            Some("Spell" | "Activated" | "Triggered")
        )
    {
        return Ok(());
    }
    let core = if mode { value } else { &mut value["args"] };
    if own_body
        && let Some(items) = core
            .pointer_mut("/body/instructions")
            .and_then(Value::as_array_mut)
    {
        prepare_list(items)?;
    }
    if let Some(modes) = core
        .pointer_mut("/announcement/modes/options")
        .and_then(Value::as_array_mut)
    {
        if modes.len() > 64 {
            return Err("sacrifice actor mode limit".into());
        }
        for mode in modes {
            prepare_owner(mode, true, depth + 1, work)?;
        }
    }
    Ok(())
}
pub(super) fn prepare(root: &mut Value) -> Result<(), String> {
    let mut work = MEASURED_WORK;
    measured(root, 0, &mut work)?;
    prepare_owner(root, true, 0, &mut 4096)
}
/// The one selected permanent a single-sacrifice payment pays with.
fn paid_filter(op: &Value) -> Option<&Value> {
    let args = &op["args"];
    let [part] = args["cost"]["parts"].as_array()?.as_slice() else {
        return None;
    };
    let objects = &part["Sacrifice"]["objects"];
    (op["kind"] == "PayCost"
        && part["Sacrifice"]["choose"] == json!({"Exactly":{"kind":"Constant","args":1}})
        && objects["kind"] == "Zone"
        && objects["args"]["kind"] == "Permanent"
        && objects["args"]["scope"] == json!({"Shared":"Battlefield"}))
    .then_some(&objects["args"]["filter"])
}
/// A possessive noun ("that creature's", "the sacrificed creature's") names the
/// sacrificed permanent beside a written actor; a bare "its" belongs to that actor.
fn names_object(value: &Value, depth: usize, work: &mut usize) -> bool {
    if depth > 32 || *work == 0 {
        return false;
    }
    *work -= 1;
    if matches!(
        symbol_name(value),
        Some(
            "qualified_object"
                | "named_object"
                | "current_named_object"
                | "that_object"
                | "defined_object"
                | "defined_named_object"
                | "sacrificed_object"
        )
    ) {
        return true;
    }
    match value {
        Value::Object(fields) => fields.values().any(|v| names_object(v, depth + 1, work)),
        Value::Array(items) => items.iter().any(|v| names_object(v, depth + 1, work)),
        _ => false,
    }
}
/// A one-object sacrifice payment's PaymentChosen damage names its input as an
/// implicit actor proved by its noun. "Sacrifice X. <damage>" without "if you do"
/// pays X only when that damage reads X: such an actor, or a possessive naming it.
fn measured_list(items: &mut [Value]) -> Result<(), String> {
    for index in 0..items.len() {
        let (head, tail) = items.split_at_mut(index + 1);
        let payment = &mut head[index];
        if payment["kind"] != "Effect" {
            continue;
        }
        let marked = symbol_name(&payment["args"]["operation"]) == Some(MEASURED);
        if marked {
            let inner = match payment["args"]["operation"]["args"].as_array_mut() {
                Some(args) if args.len() == 1 => args[0].take(),
                _ => return Err("a measured sacrifice wraps one payment".into()),
            };
            payment["args"]["operation"] = inner;
        }
        let Some(selected) = paid_filter(&payment["args"]["operation"]).cloned() else {
            if marked {
                return Err("a measured sacrifice pays with one selected permanent".into());
            }
            continue;
        };
        let damage = tail
            .first_mut()
            .filter(|next| {
                next["kind"] == "If" && next["args"]["condition"]["kind"] == "PaymentChosen"
            })
            .and_then(|next| next.pointer_mut("/args/then/instructions/0"))
            .filter(|effect| operation(effect).is_some_and(|op| op["kind"] == "DealDamage"));
        let Some(damage) = damage else {
            if marked {
                return Err("a measured sacrifice needs its paid damage".into());
            }
            continue;
        };
        let args = &mut damage["args"]["operation"]["args"];
        if !bounded(&selected, 0, &mut 4096)
            || !bounded(&args["source"], 0, &mut 4096)
            || !bounded(&args["amount"], 0, &mut 4096)
        {
            return Err("paid sacrifice actor proof work/depth limit".into());
        }
        match lexical(&args["source"], &selected) {
            Some((proved, at)) => args["source"] = json!({"$sym":ACTOR,"args":[proved],"at":at}),
            None if !marked || names_object(&args["amount"], 0, &mut 4096) => {}
            None => {
                return Err(
                    "a written damage actor's own property does not measure its sacrifice".into(),
                );
            }
        }
    }
    Ok(())
}
/// Holders and branches can write the sentence too, so every program is visited.
fn measured(value: &mut Value, depth: usize, work: &mut usize) -> Result<(), String> {
    if depth > MEASURED_DEPTH || *work == 0 {
        return Err("measured sacrifice scan work/depth limit".into());
    }
    *work -= 1;
    match value {
        Value::Object(fields) => {
            if let Some(items) = fields.get_mut("instructions").and_then(Value::as_array_mut) {
                measured_list(items)?;
            }
            for child in fields.values_mut() {
                measured(child, depth + 1, work)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                measured(child, depth + 1, work)?;
            }
        }
        _ => {}
    }
    Ok(())
}
/// The actor under a required payment's PaymentChosen branch is that payment's input.
fn paid_reference(root: &Value, path: &[Step]) -> Result<Option<Value>, String> {
    let [
        prefix @ ..,
        Step::Index(index),
        Step::Key(a),
        Step::Key(then),
        Step::Key(list),
        Step::Index(0),
        Step::Key(b),
        Step::Key(c),
        Step::Key(d),
        Step::Key(e),
    ] = path
    else {
        return Ok(None);
    };
    let keys = [a, then, list, b, c, d, e].map(String::as_str);
    if keys
        != [
            "args",
            "then",
            "instructions",
            "args",
            "operation",
            "args",
            "source",
        ]
        || *index == 0
    {
        return Ok(None);
    }
    let items = get(root, prefix)
        .and_then(Value::as_array)
        .ok_or("paid sacrifice actor program")?;
    let payment = &items[*index - 1];
    if operation(payment).and_then(paid_filter).is_none() {
        return Ok(None);
    }
    let result = payment["args"]["result"]
        .as_u64()
        .and_then(|id| u32::try_from(id).ok())
        .ok_or("paid sacrifice actor actual result address")?;
    let condition = &items[*index]["args"]["condition"];
    if condition["kind"] != "PaymentChosen"
        || condition["args"]
            .as_u64()
            .is_some_and(|id| id != u64::from(result))
    {
        return Err("a paid sacrifice actor needs its own PaymentChosen branch".into());
    }
    serde_json::to_value(ObjectRef::Query {
        objects: Box::new(ObjectSet::CostObjects {
            payment: PaymentRef::Resolution(ResultId(result)),
            component: vec![0],
            view: CostObjectView::Inputs,
            phase: ObjectPhase::Before,
        }),
        index: 0,
    })
    .map(Some)
    .map_err(|e| e.to_string())
}
fn get<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(k) => value.get(k)?,
            Step::Index(i) => value.get(*i)?,
        };
    }
    Some(value)
}
pub(super) fn reference(root: &Value, path: &[Step]) -> Result<Value, String> {
    let node = get(root, path).ok_or("sacrifice actor path")?;
    if node["args"] != json!([true]) {
        return Err(
            "selected sacrifice source noun/provenance is not positively established".into(),
        );
    }
    if let Some(paid) = paid_reference(root, path)? {
        return Ok(paid);
    }
    let [
        prefix @ ..,
        Step::Index(index),
        Step::Key(a),
        Step::Key(b),
        Step::Key(c),
        Step::Key(d),
    ] = path
    else {
        return Err("sacrifice actor instruction path".into());
    };
    if (prefix != super::path_of(&["args", "body", "instructions"]).as_slice()
        && prefix != super::path_of(&["body", "instructions"]).as_slice())
        || *index == 0
        || (a.as_str(), b.as_str(), c.as_str(), d.as_str())
            != ("args", "operation", "args", "source")
    {
        return Err("sacrifice actor needs own immediate damage source".into());
    }
    let items = get(root, prefix)
        .and_then(Value::as_array)
        .ok_or("sacrifice actor program")?;
    if operation(&items[*index]).is_none_or(|op| op["kind"] != "DealDamage") {
        return Err("sacrifice actor is not damage source".into());
    }
    let previous = &items[*index - 1];
    let op = action(operation(previous).ok_or("sacrifice actor producer")?);
    if !bounded(op, 0, &mut 4096) {
        return Err("sacrifice actor producer work/depth limit".into());
    }
    let typed: Operation =
        serde_json::from_value(op.clone()).map_err(|e| format!("sacrifice actor producer: {e}"))?;
    if !matches!(
        typed,
        Operation::Sacrifice {
            players: PlayerSet::One(_),
            objects: ObjectSet::Zone {
                scope: ZoneScope::Shared(EventZone::Battlefield),
                kind: ObjectKind::Permanent,
                ..
            },
            choose: Some(
                Count::Exactly(Expr::Constant(1))
                    | Count::UpTo(Expr::Constant(1))
                    | Count::AsManyAsCan(Expr::Constant(1))
            )
        }
    ) {
        return Err("sacrifice actor needs one resolving selection".into());
    }
    let result = previous["args"]["result"]
        .as_u64()
        .and_then(|id| u32::try_from(id).ok())
        .ok_or("sacrifice actor actual result address")?;
    serde_json::to_value(ObjectRef::Query {
        objects: Box::new(ObjectSet::Transition {
            result: ResultId(result),
            phase: ObjectPhase::Before,
        }),
        index: 0,
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
