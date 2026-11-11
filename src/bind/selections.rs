//! A selector phrase supplies a typed preparation step at its operation site.
//! This binds choices, not instruction results; printing exposes only single-use
//! steps that can be expressed inline without duplicating a player's decision.
use crate::model::ir_schema::{Fields, Schema, Ty, TypeDef};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Bind,
    Extract,
    Surface,
}

struct Walker {
    schema: &'static Schema,
    work: usize,
}

fn schema() -> Result<&'static Schema, String> {
    static SCHEMA: OnceLock<Result<Schema, String>> = OnceLock::new();
    SCHEMA
        .get_or_init(Schema::of_ir)
        .as_ref()
        .map_err(Clone::clone)
}

pub(super) fn index(value: &Value) -> Result<Value, String> {
    let id = value["args"]
        .get(0)
        .and_then(Value::as_u64)
        .filter(|id| *id < 256)
        .ok_or("selection id needs an index below 256")?;
    Ok(json!(id))
}

pub(super) fn action(mut operation: &Value) -> &Value {
    while operation["kind"] == "WithSelections" {
        operation = &operation["args"]["action"];
    }
    operation
}

pub(super) fn singular(operation: &Value, objects: &Value) -> bool {
    let step = if objects["$sym"] == "selection" {
        objects["args"].get(0)
    } else if objects["kind"] == "Prepared" {
        let mut current = operation;
        let mut found = None;
        while current["kind"] == "WithSelections" {
            if let Some(steps) = current["args"]["steps"].as_array()
                && let Some(step) = steps.iter().find(|step| step["binding"] == objects["args"])
            {
                found = Some(step);
            }
            current = &current["args"]["action"];
        }
        found
    } else {
        None
    };
    step.is_some_and(|step| {
        let chooser = &step["chooser"];
        let count = &step["count"];
        (chooser == "Chance" || chooser["Players"]["kind"] == "One")
            && ["Exactly", "UpTo", "AsManyAsCan"].iter().any(|kind| {
                count[*kind]["kind"] == "Constant"
                    && count[*kind]["args"].as_u64().is_some_and(|n| n <= 1)
            })
    })
}

pub(super) fn bind(value: &mut Value) -> Result<(), String> {
    if !has_marker(value) {
        return Ok(());
    }
    Walker {
        schema: schema()?,
        work: 0,
    }
    .walk(
        value,
        &Ty::Named("Ability".into()),
        Mode::Bind,
        &mut Vec::new(),
        0,
        false,
    )?;
    if has_marker(value) {
        return Err("an object choice has no operation preparation scope".into());
    }
    Ok(())
}

pub(super) fn surface(value: &mut Value) -> Result<(), String> {
    if !has_preparation(value) {
        return Ok(());
    }
    Walker {
        schema: schema()?,
        work: 0,
    }
    .walk(
        value,
        &Ty::Named("Ability".into()),
        Mode::Surface,
        &mut Vec::new(),
        0,
        false,
    )
}

fn has_marker(value: &Value) -> bool {
    if matches!(value["$sym"].as_str(), Some("selection" | "selection_id")) {
        return true;
    }
    match value {
        Value::Object(fields) => fields.values().any(has_marker),
        Value::Array(items) => items.iter().any(has_marker),
        _ => false,
    }
}

fn has_preparation(value: &Value) -> bool {
    if value["kind"] == "WithSelections" {
        return true;
    }
    match value {
        Value::Object(fields) => fields.values().any(has_preparation),
        Value::Array(items) => items.iter().any(has_preparation),
        _ => false,
    }
}

impl Walker {
    fn walk(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        mode: Mode,
        steps: &mut Vec<Value>,
        depth: usize,
        root: bool,
    ) -> Result<(), String> {
        self.work += 1;
        if depth > 256 || self.work > 1_000_000 {
            return Err("selection binding work limit".into());
        }
        match ty {
            Ty::Opaque => {}
            Ty::Vec(inner) => {
                if let Some(items) = value.as_array_mut() {
                    for item in items {
                        self.walk(item, inner, mode, steps, depth + 1, false)?;
                    }
                }
            }
            Ty::Option(inner) => {
                if !value.is_null() {
                    self.walk(value, inner, mode, steps, depth + 1, false)?;
                }
            }
            Ty::Tuple(types) => {
                if let Some(items) = value.as_array_mut() {
                    for (item, ty) in items.iter_mut().zip(types) {
                        self.walk(item, ty, mode, steps, depth + 1, false)?;
                    }
                }
            }
            Ty::Named(name) => {
                if name == "SelectionId" && mode == Mode::Bind && value["$sym"] == "selection_id" {
                    let id = value["args"]
                        .get(0)
                        .and_then(Value::as_u64)
                        .filter(|n| *n < 256)
                        .ok_or("selection id needs an index below 256")?;
                    *value = json!(id);
                    return Ok(());
                }
                if mode == Mode::Extract
                    && !root
                    && matches!(
                        name.as_str(),
                        "Operation"
                            | "Instruction"
                            | "Ability"
                            | "Cost"
                            | "Announcement"
                            | "TargetSpec"
                            | "EventSpec"
                            | "ObjectFilter"
                    )
                {
                    return Ok(());
                }
                if name == "ObjectSet" && value["$sym"] == "selection" {
                    if mode == Mode::Bind {
                        let step = value["args"].get_mut(0).ok_or("selection needs one step")?;
                        self.walk(
                            step,
                            &Ty::Named("SelectionStep".into()),
                            mode,
                            steps,
                            depth + 1,
                            false,
                        )?;
                    }
                    if mode == Mode::Extract {
                        let mut step = value["args"]
                            .get(0)
                            .ok_or("selection needs one step")?
                            .clone();
                        self.walk(
                            &mut step,
                            &Ty::Named("SelectionStep".into()),
                            mode,
                            steps,
                            depth + 1,
                            false,
                        )?;
                        if steps.len() >= 256 {
                            return Err("operation selection bound".into());
                        }
                        let binding = steps.len();
                        step["binding"] = json!(binding);
                        steps.push(step);
                        *value = json!({"kind":"Prepared","args":binding});
                    }
                    return Ok(());
                }
                let definition = self.schema.types.get(name);
                match definition {
                    Some(TypeDef::Struct(fields)) => {
                        self.fields(value, fields, mode, steps, depth)?
                    }
                    Some(TypeDef::Enum { adjacent, .. }) => {
                        let variant = if *adjacent {
                            value["kind"].as_str().map(str::to_owned)
                        } else {
                            value.as_object().and_then(|o| {
                                (o.len() == 1).then(|| o.keys().next().cloned()).flatten()
                            })
                        };
                        if let Some(variant) = variant
                            && let Some((_, fields)) = self.schema.variant(name, &variant)
                        {
                            let key = if *adjacent { "args" } else { &variant };
                            if let Some(body) = value.get_mut(key) {
                                self.fields(body, fields, mode, steps, depth)?;
                            }
                        }
                    }
                    None => {}
                }
                if name == "Operation" {
                    match mode {
                        Mode::Bind => self.prepare(value, depth + 1)?,
                        Mode::Surface => inline(value, self.schema),
                        Mode::Extract => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn fields(
        &mut self,
        value: &mut Value,
        fields: &Fields,
        mode: Mode,
        steps: &mut Vec<Value>,
        depth: usize,
    ) -> Result<(), String> {
        match fields {
            Fields::Unit => {}
            Fields::Tuple(types) => {
                if let [ty] = types.as_slice() {
                    self.walk(value, ty, mode, steps, depth + 1, false)?;
                } else if let Some(items) = value.as_array_mut() {
                    for (item, ty) in items.iter_mut().zip(types) {
                        self.walk(item, ty, mode, steps, depth + 1, false)?;
                    }
                }
            }
            Fields::Named(types) => {
                for (name, ty) in types {
                    if let Some(item) = value.get_mut(name) {
                        self.walk(item, ty, mode, steps, depth + 1, false)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn prepare(&mut self, value: &mut Value, depth: usize) -> Result<(), String> {
        let mut steps = Vec::new();
        if value["kind"] == "WithSelections" {
            let explicit = value["args"]["steps"]
                .take()
                .as_array()
                .ok_or("selection step list")?
                .clone();
            let mut mapping = HashMap::new();
            for mut step in explicit {
                let old = step["binding"].as_u64().ok_or("selection binding")?;
                rewrite(
                    &mut step,
                    &Ty::Named("SelectionStep".into()),
                    self.schema,
                    &mut mapping,
                    &mut HashMap::new(),
                );
                self.walk(
                    &mut step,
                    &Ty::Named("SelectionStep".into()),
                    Mode::Extract,
                    &mut steps,
                    depth + 1,
                    false,
                )?;
                let binding = steps.len() as u64;
                step["binding"] = json!(binding);
                mapping.insert(old, json!({"kind":"Prepared","args":binding}));
                steps.push(step);
            }
            rewrite(
                &mut value["args"]["action"],
                &Ty::Named("Operation".into()),
                self.schema,
                &mut mapping,
                &mut HashMap::new(),
            );
            value["args"]["steps"] = json!(steps);
            return Ok(());
        }
        self.walk(
            value,
            &Ty::Named("Operation".into()),
            Mode::Extract,
            &mut steps,
            depth + 1,
            true,
        )?;
        if !steps.is_empty() {
            *value = json!({"kind":"WithSelections","args":{"steps":steps,"action":value.take()}});
        }
        Ok(())
    }
}

/// Inline only choices used exactly once, preserving their order/dependencies.
fn inline(value: &mut Value, schema: &Schema) {
    if value["kind"] != "WithSelections" {
        return;
    }
    let Some(steps) = value["args"]["steps"].as_array() else {
        return;
    };
    let mut replacements = HashMap::new();
    let mut uses = HashMap::new();
    for step in steps {
        let Some(binding) = step["binding"].as_u64() else {
            return;
        };
        let mut step = step.clone();
        rewrite(
            &mut step,
            &Ty::Named("SelectionStep".into()),
            schema,
            &mut replacements,
            &mut uses,
        );
        step["binding"] = json!(0);
        replacements.insert(binding, json!({"$sym":"selection","at":0,"args":[step]}));
        uses.insert(binding, 0_usize);
    }
    let mut action = value["args"]["action"].clone();
    rewrite(
        &mut action,
        &Ty::Named("Operation".into()),
        schema,
        &mut replacements,
        &mut uses,
    );
    if uses.values().all(|n| *n == 1) {
        *value = action;
    }
}

/// Rewrite local preparation reads through the native lexical boundaries.
fn rewrite(
    value: &mut Value,
    ty: &Ty,
    schema: &Schema,
    replacements: &mut HashMap<u64, Value>,
    uses: &mut HashMap<u64, usize>,
) {
    match ty {
        Ty::Opaque => {}
        Ty::Option(inner) => {
            if !value.is_null() {
                rewrite(value, inner, schema, replacements, uses);
            }
        }
        Ty::Vec(inner) => {
            if let Some(items) = value.as_array_mut() {
                for item in items {
                    rewrite(item, inner, schema, replacements, uses);
                }
            }
        }
        Ty::Tuple(types) => {
            if let Some(items) = value.as_array_mut() {
                for (item, ty) in items.iter_mut().zip(types) {
                    rewrite(item, ty, schema, replacements, uses);
                }
            }
        }
        Ty::Named(name) => {
            if matches!(
                name.as_str(),
                "Instruction"
                    | "Ability"
                    | "Cost"
                    | "Announcement"
                    | "TargetSpec"
                    | "EventSpec"
                    | "ObjectFilter"
            ) {
                return;
            }
            if name == "ObjectSet" {
                if value["kind"] == "Prepared" {
                    if let Some(id) = value["args"].as_u64()
                        && let Some(replacement) = replacements.get(&id)
                    {
                        *value = replacement.clone();
                        *uses.entry(id).or_default() += 1;
                    }
                    return;
                }
                if value["$sym"] == "selection" {
                    if let Some(step) = value["args"].get_mut(0) {
                        rewrite(
                            step,
                            &Ty::Named("SelectionStep".into()),
                            schema,
                            replacements,
                            uses,
                        );
                    }
                    return;
                }
            }
            if name == "Operation" && value["kind"] == "WithSelections" {
                let saved = replacements.clone();
                if let Some(steps) = value["args"]["steps"].as_array_mut() {
                    for step in steps {
                        rewrite(
                            step,
                            &Ty::Named("SelectionStep".into()),
                            schema,
                            replacements,
                            uses,
                        );
                        if let Some(id) = step["binding"].as_u64() {
                            replacements.remove(&id);
                        }
                    }
                }
                rewrite(&mut value["args"]["action"], ty, schema, replacements, uses);
                *replacements = saved;
                return;
            }
            match schema.types.get(name) {
                Some(TypeDef::Struct(fields)) => {
                    rewrite_fields(value, fields, schema, replacements, uses)
                }
                Some(TypeDef::Enum { adjacent, .. }) => {
                    let variant = if *adjacent {
                        value["kind"].as_str().map(str::to_owned)
                    } else {
                        value.as_object().and_then(|o| {
                            (o.len() == 1).then(|| o.keys().next().cloned()).flatten()
                        })
                    };
                    if let Some(variant) = variant
                        && let Some((_, fields)) = schema.variant(name, &variant)
                        && let Some(body) = value.get_mut(if *adjacent { "args" } else { &variant })
                    {
                        rewrite_fields(body, fields, schema, replacements, uses);
                    }
                }
                None => {}
            }
        }
    }
}

fn rewrite_fields(
    value: &mut Value,
    fields: &Fields,
    schema: &Schema,
    replacements: &mut HashMap<u64, Value>,
    uses: &mut HashMap<u64, usize>,
) {
    match fields {
        Fields::Unit => {}
        Fields::Tuple(types) => {
            if let [ty] = types.as_slice() {
                rewrite(value, ty, schema, replacements, uses);
            } else if let Some(items) = value.as_array_mut() {
                for (item, ty) in items.iter_mut().zip(types) {
                    rewrite(item, ty, schema, replacements, uses);
                }
            }
        }
        Fields::Named(types) => {
            for (name, ty) in types {
                if let Some(item) = value.get_mut(name) {
                    rewrite(item, ty, schema, replacements, uses);
                }
            }
        }
    }
}
