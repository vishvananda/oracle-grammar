//! Native type-table proof: other integer namespaces cannot impersonate results.
use crate::model::ir::Ability;
use crate::model::ir_schema::{Fields, Schema, Ty, TypeDef};
use serde_json::Value;
use std::sync::OnceLock;

fn schema() -> Result<&'static Schema, String> {
    static SCHEMA: OnceLock<Result<Schema, String>> = OnceLock::new();
    SCHEMA
        .get_or_init(Schema::of_ir)
        .as_ref()
        .map_err(Clone::clone)
}

/// The clone already has the exact proposed replacements, so the removed
/// definition and replaced implicit recipient are absent. Uniqueness is proved
/// separately: any remaining occurrence of this ResultId would be a read.
pub(super) fn absent(value: Value, removed: u64) -> Result<bool, String> {
    super::bounded(&value)?;
    let Ok(ability) = serde_json::from_value::<Ability>(value) else {
        return Ok(false);
    };
    let plain = serde_json::to_value(ability).map_err(|e| e.to_string())?;
    super::bounded(&plain)?;
    let mut scan = Scan {
        schema: schema()?,
        removed,
        work: 16_384,
    };
    scan.walk(&plain, &Ty::Named("Ability".into()), 0)
}

struct Scan {
    schema: &'static Schema,
    removed: u64,
    work: usize,
}
impl Scan {
    fn walk(&mut self, value: &Value, ty: &Ty, depth: usize) -> Result<bool, String> {
        if depth > 128 || self.work == 0 {
            return Err("paid self-exile result-use work/depth bound".into());
        }
        self.work -= 1;
        match ty {
            Ty::Named(name) if name == "ResultId" => {
                Ok(value.as_u64().is_some_and(|id| id != self.removed))
            }
            Ty::Named(name) => {
                let schema = self.schema;
                match schema.types.get(name) {
                    Some(TypeDef::Struct(fields)) => self.fields(value, fields, depth + 1),
                    Some(TypeDef::Enum { adjacent, .. }) => {
                        let (variant, contents) = if *adjacent {
                            (value["kind"].as_str(), &value["args"])
                        } else if let Some(unit) = value.as_str() {
                            (Some(unit), &Value::Null)
                        } else if let Some(items) = value.as_object().filter(|v| v.len() == 1) {
                            let (key, item) = items.iter().next().ok_or("empty native variant")?;
                            (Some(key.as_str()), item)
                        } else {
                            return Ok(false);
                        };
                        let Some((_, fields)) = variant.and_then(|v| schema.variant(name, v))
                        else {
                            return Ok(false);
                        };
                        self.fields(contents, fields, depth + 1)
                    }
                    // Imported inert vocabulary is atomic. Do not guess about
                    // unknown structured types that might contain an ID.
                    None => Ok(!value.is_object() && !value.is_array()),
                }
            }
            Ty::Option(_) if value.is_null() => Ok(true),
            Ty::Option(inner) => self.walk(value, inner, depth + 1),
            Ty::Vec(inner) => {
                let Some(items) = value.as_array() else {
                    return Ok(false);
                };
                for item in items {
                    if !self.walk(item, inner, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Ty::Tuple(types) => self.tuple(value, types, depth + 1),
            Ty::Opaque => Ok(!value.is_object() && !value.is_array()),
        }
    }
    fn fields(&mut self, value: &Value, fields: &Fields, depth: usize) -> Result<bool, String> {
        match fields {
            Fields::Unit => Ok(true),
            Fields::Tuple(types) => match types.as_slice() {
                [one] => self.walk(value, one, depth + 1),
                _ => self.tuple(value, types, depth + 1),
            },
            Fields::Named(types) => {
                for (name, ty) in types {
                    if !self.walk(&value[name], ty, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
        }
    }
    fn tuple(&mut self, value: &Value, types: &[Ty], depth: usize) -> Result<bool, String> {
        let Some(items) = value.as_array().filter(|v| v.len() == types.len()) else {
            return Ok(false);
        };
        for (item, ty) in items.iter().zip(types) {
            if !self.walk(item, ty, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
