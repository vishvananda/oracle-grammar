//! Explicit structural roots supplement legacy `Named::[items]` vector sugar.
use crate::model::ir_schema::{Schema, Ty};

pub(super) fn parse(text: &str) -> Result<Ty, String> {
    if text.len() > 4096 {
        return Err("semantic root type exceeds byte limit".into());
    }
    let mut reader = Reader {
        text: text.as_bytes(),
        at: 0,
    };
    let ty = reader.ty(0)?;
    reader.space();
    if reader.at != reader.text.len() {
        return Err("trailing semantic root type".into());
    }
    Ok(ty)
}

pub(crate) fn declared(text: &str, schema: &Schema) -> Result<(Ty, bool), String> {
    let ty = parse(text)?;
    let legacy = matches!(ty, Ty::Named(_));
    if !legacy {
        validate(&ty, schema)?;
    }
    Ok((ty, legacy))
}

fn validate(ty: &Ty, schema: &Schema) -> Result<(), String> {
    match ty {
        Ty::Named(name)
            if schema.types.contains_key(name)
                || matches!(
                    name.as_str(),
                    "i64" | "u32" | "u64" | "usize" | "bool" | "String"
                ) =>
        {
            Ok(())
        }
        Ty::Named(name) => Err(format!("unknown structural root type {name}")),
        Ty::Vec(inner) | Ty::Option(inner) => validate(inner, schema),
        Ty::Tuple(parts) => {
            for part in parts {
                validate(part, schema)?;
            }
            Ok(())
        }
        Ty::Opaque => Err("opaque structural root type".into()),
    }
}

struct Reader<'a> {
    text: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn space(&mut self) {
        while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }
    fn eat(&mut self, byte: u8) -> bool {
        self.space();
        if self.text.get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn ty(&mut self, depth: usize) -> Result<Ty, String> {
        if depth > 32 {
            return Err("semantic root type depth limit".into());
        }
        if self.eat(b'(') {
            let mut parts = vec![self.ty(depth + 1)?];
            if !self.eat(b',') {
                return Err("tuple root needs at least two types".into());
            }
            parts.push(self.ty(depth + 1)?);
            while self.eat(b',') {
                if self.eat(b')') {
                    return Ok(Ty::Tuple(parts));
                }
                parts.push(self.ty(depth + 1)?);
            }
            if !self.eat(b')') {
                return Err("unclosed tuple root type".into());
            }
            return Ok(Ty::Tuple(parts));
        }
        self.space();
        let start = self.at;
        while self
            .text
            .get(self.at)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            self.at += 1;
        }
        if start == self.at || !self.text[start].is_ascii_alphabetic() {
            return Err("expected semantic root type name".into());
        }
        let name = std::str::from_utf8(&self.text[start..self.at])
            .map_err(|_| "root type is not ASCII")?
            .to_owned();
        if !self.eat(b'<') {
            return Ok(Ty::Named(name));
        }
        let inner = self.ty(depth + 1)?;
        if !self.eat(b'>') {
            return Err("unclosed generic root type".into());
        }
        match name.as_str() {
            "Vec" => Ok(Ty::Vec(Box::new(inner))),
            "Option" => Ok(Ty::Option(Box::new(inner))),
            _ => Err(format!("unsupported root type constructor {name}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_roots_parse_nested_types_without_legacy_vector_sugar() {
        let schema = Schema::of_ir().unwrap();
        let pair = Ty::Tuple(vec![
            Ty::Named("i64".into()),
            Ty::Named("ClassLevel".into()),
        ]);
        assert_eq!(
            declared("(i64, ClassLevel)", &schema).unwrap(),
            (pair.clone(), false)
        );
        assert_eq!(
            declared("Vec < (i64, ClassLevel) >", &schema).unwrap(),
            (Ty::Vec(Box::new(pair)), false)
        );
        assert_eq!(
            declared("ClassLevel", &schema).unwrap(),
            (Ty::Named("ClassLevel".into()), true)
        );
        assert_eq!(
            parse("Option<Vec<(i64,String)>>").unwrap(),
            Ty::Option(Box::new(Ty::Vec(Box::new(Ty::Tuple(vec![
                Ty::Named("i64".into()),
                Ty::Named("String".into())
            ])))))
        );
    }

    #[test]
    fn malformed_unknown_and_unbounded_structural_roots_refuse() {
        let schema = Schema::of_ir().unwrap();
        for text in [
            "Vec<(i64,NoSuchType)>",
            "Vec<i64",
            "Vec<>",
            "(i64)",
            "()",
            "(i64,)",
            "(i64,ClassLevel",
            "Vec<i64,i64>",
            "Map<i64>",
            "Vec<i64>tail",
        ] {
            assert!(declared(text, &schema).is_err(), "{text}");
        }
        assert!(parse(&("Vec<".repeat(34) + "i64" + &">".repeat(34))).is_err());
        assert!(parse(&"a".repeat(4097)).is_err());
    }
}
