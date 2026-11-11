use super::{
    Context, META,
    operands::{actions, group, scalar},
};
use crate::model::ir::Instruction;
use serde_json::{Value, json};
fn lower_marked(
    instruction: &Value,
    context: &Context,
    allocate: &mut impl FnMut() -> Result<u32, String>,
) -> Result<Option<Value>, String> {
    crate::functions::branch_bounds(instruction)?;
    let _: Instruction = serde_json::from_value(instruction.clone()).map_err(|e| e.to_string())?;
    let Some((result, original)) = context.current_return() else {
        return Ok(None);
    };
    if instruction["kind"] != "If" {
        return Ok(None);
    }
    let args = &instruction["args"];
    let c = &args["condition"];
    if c["kind"] != "ObjectMatches"
        || (c["args"]["object"] != scalar(result)
            && (!matches!(
                original["kind"].as_str(),
                Some("Target" | "Source" | "GrantingObject")
            ) || &c["args"]["object"] != original.as_ref()))
    {
        return Ok(None);
    }
    let one = json!({"kind":"One","args":scalar(result)});
    // Prove all operands before allocating definitions or changing anything.
    if actions(&args["then"], &one, &one).is_none()
        || actions(&args["otherwise"], &one, &one).is_none()
    {
        return Ok(None);
    }
    let yes_id = allocate()?;
    let yes = json!({"kind":"Selected","args":yes_id});
    let no_id = (!args["otherwise"]["instructions"]
        .as_array()
        .is_some_and(Vec::is_empty))
    .then(allocate)
    .transpose()?;
    let mut items = vec![
        json!({"kind":"Effect","args":{"result":yes_id,"operation":{"kind":"NoteObjects","args":{"objects":{"kind":"Matching","args":{"objects":group(result),"filter":c["args"]["filter"]}}}}}}),
    ];
    if let Some(no_id) = no_id {
        items.push(json!({"kind":"Effect","args":{"result":no_id,"operation":{"kind":"NoteObjects","args":{"objects":{"kind":"Difference","args":{"objects":group(result),"excluding":yes}}}}}}));
    }
    items.extend(actions(&args["then"], &one, &yes).ok_or("returned yes actions changed")?);
    if let Some(no_id) = no_id {
        items.extend(
            actions(
                &args["otherwise"],
                &one,
                &json!({"kind":"Selected","args":no_id}),
            )
            .ok_or("returned no actions changed")?,
        );
    }
    let output = json!({"kind":"Group","args":{"instructions":items}});
    crate::functions::branch_bounds(&output)?;
    Ok(Some(output))
}
pub(super) fn visit(
    value: &mut Value,
    context: &Context,
    depth: usize,
    work: &mut usize,
    allocate: &mut impl FnMut() -> Result<u32, String>,
) -> Result<(), String> {
    if depth > 128 || *work == 0 {
        return Err("returned conditional work/depth".into());
    }
    *work -= 1;
    if value.get(META).is_some() {
        if value[META] != true {
            return Err("returned conditional annotation".into());
        }
        value
            .as_object_mut()
            .ok_or("returned conditional object")?
            .remove(META);
        if let Some(replacement) = lower_marked(value, context, allocate)? {
            *value = replacement;
            return Ok(());
        }
    }
    let fresh;
    let context = if matches!(
        crate::traverse::boundary(value),
        crate::traverse::Boundary::Ability
            | crate::traverse::Boundary::Mode
            | crate::traverse::Boundary::Holder
            | crate::traverse::Boundary::Lexical
            | crate::traverse::Boundary::Reflexive
    ) || matches!(
        value["kind"].as_str(),
        Some("CreateTrigger" | "CreateReplacement" | "ReflexiveTrigger")
    ) {
        fresh = Context::default();
        &fresh
    } else {
        context
    };
    if matches!(
        value["kind"].as_str(),
        Some(
            "If" | "Optional"
                | "ChooseOne"
                | "ForEachObject"
                | "ForEachPlayer"
                | "RepeatableWithLimit"
                | "CreateDelayedTrigger"
        )
    ) {
        let child = context.scalar_branch();
        if let Value::Object(fields) = value {
            for v in fields.values_mut() {
                visit(v, &child, depth + 1, work, allocate)?;
            }
        }
        return Ok(());
    }
    if let Some(items) = value.get_mut("instructions").and_then(Value::as_array_mut) {
        let mut local = context.clone();
        for i in items {
            let consumed = i.get(META).is_some();
            visit(i, &local, depth + 1, work, allocate)?;
            local.record(i)?;
            if consumed {
                local = local.scalar_branch();
            }
        }
        return Ok(());
    }
    match value {
        Value::Object(o) => {
            for v in o.values_mut() {
                visit(v, context, depth + 1, work, allocate)?;
            }
        }
        Value::Array(a) => {
            for v in a {
                visit(v, context, depth + 1, work, allocate)?;
            }
        }
        _ => {}
    }
    Ok(())
}
