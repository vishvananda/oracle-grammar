//! A bounded schema walk for capture-use proofs and origin-preserving edits.
use crate::ir_schema::{Fields, Schema, Ty, TypeDef};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::HashMap;

pub(super) trait Visit {
    fn enter(&mut self, _: &Value, _: &Ty) {}
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        defining: bool,
        path: &str,
        walker: &mut Walker<'_>,
    ) -> Result<(), String>;
}

pub(super) struct Walker<'a> {
    pub schema: &'a Schema,
    remaining: usize,
}
impl<'a> Walker<'a> {
    pub fn new(schema: &'a Schema) -> Self {
        Self {
            schema,
            remaining: 1_000_000,
        }
    }
    pub fn walk(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        defining: bool,
        path: &str,
        depth: usize,
        visitor: &mut dyn Visit,
    ) -> Result<(), String> {
        if depth > 256 || self.remaining == 0 {
            return Err("canonical program normalization work/depth limit".into());
        }
        self.remaining -= 1;
        visitor.enter(value, ty);
        match ty {
            Ty::Opaque => {}
            Ty::Option(inner) => {
                if !value.is_null() {
                    self.walk(value, inner, defining, path, depth + 1, visitor)?;
                }
            }
            Ty::Vec(inner) => {
                if let Some(items) = value.as_array_mut() {
                    for (i, item) in items.iter_mut().enumerate() {
                        self.walk(
                            item,
                            inner,
                            false,
                            &format!("{path}/{i}"),
                            depth + 1,
                            visitor,
                        )?;
                    }
                }
            }
            Ty::Tuple(parts) => {
                if let Some(items) = value.as_array_mut() {
                    for (i, (item, part)) in items.iter_mut().zip(parts).enumerate() {
                        self.walk(
                            item,
                            part,
                            false,
                            &format!("{path}/{i}"),
                            depth + 1,
                            visitor,
                        )?;
                    }
                }
            }
            Ty::Named(name) => {
                let schema = self.schema;
                match schema.types.get(name) {
                    Some(TypeDef::Struct(fields)) => {
                        self.fields(value, fields, name, path, depth + 1, visitor)?;
                    }
                    Some(TypeDef::Enum { adjacent, .. }) => {
                        let variant = if *adjacent {
                            value["kind"].as_str()
                        } else {
                            value
                                .as_str()
                                .or_else(|| value.as_object()?.keys().next().map(String::as_str))
                        }
                        .map(str::to_owned);
                        if let Some(variant) = variant
                            && let Some((_, fields)) = schema.variant(name, &variant)
                        {
                            let key = if *adjacent { "args" } else { &variant };
                            if let Some(content) = value.get_mut(key) {
                                self.fields(
                                    content,
                                    fields,
                                    &format!("{name}::{variant}"),
                                    &crate::binding_map::pointer(path, key),
                                    depth + 1,
                                    visitor,
                                )?;
                            }
                        }
                    }
                    None => {}
                }
            }
        }
        visitor.leave(value, ty, defining, path, self)
    }

    fn fields(
        &mut self,
        value: &mut Value,
        fields: &Fields,
        owner: &str,
        path: &str,
        depth: usize,
        visitor: &mut dyn Visit,
    ) -> Result<(), String> {
        match fields {
            Fields::Unit => {}
            Fields::Named(fields) => {
                for (field, ty) in fields {
                    if let Some(item) = value.get_mut(field) {
                        self.walk(
                            item,
                            ty,
                            super::defines(owner, field),
                            &crate::binding_map::pointer(path, field),
                            depth,
                            visitor,
                        )?;
                    }
                }
            }
            Fields::Tuple(types) => {
                if let [ty] = types.as_slice() {
                    self.walk(value, ty, false, path, depth, visitor)?;
                } else if let Some(items) = value.as_array_mut() {
                    for (i, (item, ty)) in items.iter_mut().zip(types).enumerate() {
                        self.walk(item, ty, false, &format!("{path}/{i}"), depth, visitor)?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Uses {
    pub total: HashMap<u64, usize>,
    pub definitions: HashMap<u64, usize>,
    pub programs: Vec<String>,
    pub abilities: usize,
    pub bindings: Vec<(String, u64)>,
}
impl Visit for Uses {
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        defining: bool,
        path: &str,
        _: &mut Walker<'_>,
    ) -> Result<(), String> {
        if matches!(ty, Ty::Named(name) if name == "Ability") {
            self.abilities += 1;
        }
        if defining
            && matches!(ty, Ty::Named(name) if name == "LoopBinding")
            && let Some(id) = super::trace::number(value)
        {
            self.bindings.push((path.into(), id));
        }
        if matches!(ty, Ty::Named(name) if name == "ResultId")
            && let Some(id) = super::trace::number(value)
        {
            *self.total.entry(id).or_default() += 1;
            if defining {
                *self.definitions.entry(id).or_default() += 1;
            }
        }
        if matches!(ty, Ty::Vec(inner) if matches!(inner.as_ref(), Ty::Named(name) if name == "Instruction"))
        {
            // Postorder paths keep inner arrays stable until their parent is edited.
            self.programs.push(path.into());
        }
        Ok(())
    }
}

/// Refuse to drop undeclared fields or trust forged origin metadata. Bound the
/// raw candidate as well as its schema walk: unknown fields can contain data.
pub(super) fn lossless<T: DeserializeOwned + Serialize>(value: &Value, tracing: bool) -> Option<T> {
    let mut remaining = 32_768;
    let mut bytes: usize = 1024 * 1024;
    let mut pending = vec![(value, 0)];
    while let Some((value, depth)) = pending.pop() {
        if remaining == 0 || depth > 128 {
            return None;
        }
        remaining -= 1;
        // A private traced ID is still one numeric leaf in the public value.
        // Origin bookkeeping must not change which semantic candidates fit.
        if tracing && value.get("$site_origins").is_some() && super::trace::number(value).is_some()
        {
            continue;
        }
        let size = match value {
            Value::Object(fields) => {
                if fields.len().checked_add(pending.len())? > remaining {
                    return None;
                }
                pending.extend(fields.values().map(|v| (v, depth + 1)));
                fields
                    .keys()
                    .try_fold(0usize, |sum, key| sum.checked_add(key.len()))?
            }
            Value::Array(items) => {
                if items.len().checked_add(pending.len())? > remaining {
                    return None;
                }
                pending.extend(items.iter().map(|v| (v, depth + 1)));
                0
            }
            Value::String(text) => text.len(),
            _ => 0,
        };
        bytes = bytes.checked_sub(size)?;
    }
    let plain = if tracing {
        super::trace::plain(value)
    } else {
        value.clone()
    };
    let typed: T = serde_json::from_value(plain.clone()).ok()?;
    (serde_json::to_value(&typed).ok()? == plain).then_some(typed)
}
