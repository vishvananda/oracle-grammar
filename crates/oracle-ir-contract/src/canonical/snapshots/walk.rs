//! A bounded schema walk for capture-use proofs and origin-preserving edits.
use crate::ir_schema::{Fields, Schema, Ty, TypeDef};
use serde_json::Value;

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
            return Err("snapshot normalization work/depth limit".into());
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
                            super::super::defines(owner, field),
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
