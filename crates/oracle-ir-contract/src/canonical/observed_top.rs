//! One immediate conditional move of the same explicitly revealed library top.
//! This is a program proof, never a global live-query/observation equality.
use super::{
    trace,
    walk::{Visit, Walker},
};
use crate::ir_schema::Ty;
use serde_json::{Value, json};

fn fields(value: &Value, names: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == names.len() && names.iter().all(|name| object.contains_key(*name))
    })
}

fn one_library_top(query: &Value) -> bool {
    let args = &query["args"];
    let objects = &args["objects"];
    let zone = &objects["args"];
    let scope = &zone["scope"];
    query["kind"] == "Take"
        && fields(query, &["kind", "args"])
        && fields(args, &["objects", "count", "from"])
        && args["count"] == json!({"kind":"Constant","args":1})
        && args["from"] == "Top"
        && objects["kind"] == "Zone"
        && fields(objects, &["kind", "args"])
        && fields(zone, &["scope", "kind", "filter"])
        && zone["kind"] == "Card"
        && zone["filter"] == json!({"kind":"Any"})
        && fields(scope, &["Players"])
        && fields(&scope["Players"], &["zone", "players"])
        && scope["Players"]["zone"] == "Library"
        && scope["Players"]["players"]["kind"] == "One"
}

fn reveal(instruction: &Value) -> Option<(&Value, u64)> {
    let effect = &instruction["args"];
    let operation = &effect["operation"];
    (instruction["kind"] == "Effect"
        && fields(instruction, &["kind", "args"])
        && fields(effect, &["result", "operation"])
        && operation["kind"] == "Reveal"
        && fields(operation, &["kind", "args"])
        && fields(&operation["args"], &["objects"])
        && one_library_top(&operation["args"]["objects"]))
    .then(|| {
        (
            &operation["args"]["objects"],
            trace::number(&effect["result"]),
        )
    })
    .and_then(|(query, id)| Some((query, id.filter(|id| u32::try_from(*id).is_ok())?)))
}

fn observation_reference(condition: &mut Value, result: u64) -> Result<Option<Value>, String> {
    struct Reader {
        result: u64,
        reference: Option<Value>,
        abilities: usize,
    }
    impl Visit for Reader {
        fn enter(&mut self, _: &Value, ty: &Ty) {
            if matches!(ty, Ty::Named(name) if name == "Ability") {
                self.abilities += 1;
            }
        }
        fn leave(
            &mut self,
            value: &mut Value,
            ty: &Ty,
            _: bool,
            _: &str,
            _: &mut Walker<'_>,
        ) -> Result<(), String> {
            if matches!(ty, Ty::Named(name) if name == "Ability") {
                self.abilities -= 1;
            } else if self.abilities == 0
                && matches!(ty, Ty::Named(name) if name == "ObjectSet")
                && value["kind"] == "Observed"
                && fields(value, &["kind", "args"])
                && trace::number(&value["args"]) == Some(self.result)
            {
                if let Some(reference) = &mut self.reference {
                    trace::merge(reference, &value["args"]);
                } else {
                    self.reference = Some(value["args"].clone());
                }
            }
            Ok(())
        }
    }
    // Canonical unused labels are not candidates for a new receipt use. The
    // condition must already consume this precise observation in this scope.
    if result >= u64::from(u32::MAX) - 4096 {
        return Ok(None);
    }
    let mut reader = Reader {
        result,
        reference: None,
        abilities: 0,
    };
    Walker::new(super::schema()?).walk(
        condition,
        &Ty::Named("Condition".into()),
        false,
        "",
        0,
        &mut reader,
    )?;
    Ok(reader.reference)
}

struct Proof {
    query: Value,
    result: u64,
    reference: Value,
    query_bytes: usize,
}

struct State {
    restore: bool,
    tracing: bool,
    remaining: usize,
    bytes: usize,
    changed: bool,
}
impl State {
    fn step(&mut self) -> Result<(), String> {
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or("observed-top proof work limit")?;
        Ok(())
    }

    fn first(
        &mut self,
        program: &mut Value,
        proof: &mut Proof,
        depth: usize,
    ) -> Result<bool, String> {
        self.step()?;
        if depth > 16 || !fields(program, &["instructions"]) {
            return Ok(false);
        }
        let Some(first) = program["instructions"]
            .as_array_mut()
            .and_then(|items| items.first_mut())
        else {
            return Ok(false);
        };
        if !fields(first, &["kind", "args"]) {
            return Ok(false);
        }
        match first["kind"].as_str() {
            Some("Optional") if fields(&first["args"], &["decider", "body"]) => {
                self.first(&mut first["args"]["body"], proof, depth + 1)
            }
            Some("If") if fields(&first["args"], &["condition", "then", "otherwise"]) => {
                let then = self.first(&mut first["args"]["then"], proof, depth + 1)?;
                let otherwise = self.first(&mut first["args"]["otherwise"], proof, depth + 1)?;
                Ok(then || otherwise)
            }
            Some("Effect")
                if fields(&first["args"], &["result", "operation"])
                    && first["args"]["operation"]["kind"] == "Move" =>
            {
                let objects = &mut first["args"]["operation"]["args"]["objects"];
                if self.restore {
                    if !fields(objects, &["kind", "args"])
                        || objects["kind"] != "Observed"
                        || trace::number(&objects["args"]) != Some(proof.result)
                    {
                        return Ok(false);
                    }
                    self.bytes = self
                        .bytes
                        .checked_sub(proof.query_bytes)
                        .ok_or("observed-top projection byte limit")?;
                    *objects = proof.query.clone();
                } else {
                    if !trace::same(objects, &proof.query, self.tracing) {
                        return Ok(false);
                    }
                    // Redundant query reads retain all their ID-site origins on
                    // the producer. Clone the condition's existing observation
                    // read, retaining its typed binding-site provenance too.
                    if self.tracing {
                        trace::merge(&mut proof.query, objects);
                    }
                    *objects = json!({"kind":"Observed","args":proof.reference});
                }
                Ok(true)
            }
            // Every other effect invalidates the proof. Never skip a draw,
            // payment, movement, producer, independent trigger or child ability.
            _ => Ok(false),
        }
    }

    fn program(&mut self, value: &mut Value) -> Result<(), String> {
        if !fields(value, &["instructions"]) {
            return Ok(());
        }
        let Some(items) = value["instructions"].as_array_mut() else {
            return Ok(());
        };
        for index in 1..items.len() {
            self.step()?;
            let (before, after) = items.split_at_mut(index);
            let (producer, branch) = (&mut before[index - 1], &mut after[0]);
            if branch["kind"] != "If"
                || !fields(branch, &["kind", "args"])
                || !fields(&branch["args"], &["condition", "then", "otherwise"])
            {
                continue;
            }
            let Some((query, result)) = reveal(producer) else {
                continue;
            };
            let Some(reference) = observation_reference(&mut branch["args"]["condition"], result)?
            else {
                continue;
            };
            let mut proof = Proof {
                query: query.clone(),
                result,
                reference,
                query_bytes: serde_json::to_vec(query).map_err(|e| e.to_string())?.len(),
            };
            let then = self.first(&mut branch["args"]["then"], &mut proof, 0)?;
            let otherwise = self.first(&mut branch["args"]["otherwise"], &mut proof, 0)?;
            if then || otherwise {
                if !self.restore && self.tracing {
                    producer["args"]["operation"]["args"]["objects"] = proof.query;
                }
                self.changed = true;
            }
        }
        Ok(())
    }
}

/// Canonicalize one Program during the existing bounded typed program walk.
pub(super) fn program(value: &mut Value, tracing: bool) -> Result<bool, String> {
    let mut state = State {
        restore: false,
        tracing,
        remaining: 131_072,
        bytes: 4 * 1024 * 1024,
        changed: false,
    };
    state.program(value)?;
    Ok(state.changed)
}

impl Visit for State {
    fn leave(
        &mut self,
        value: &mut Value,
        ty: &Ty,
        _: bool,
        _: &str,
        _: &mut Walker<'_>,
    ) -> Result<(), String> {
        if matches!(ty, Ty::Named(name) if name == "Program") {
            self.program(value)?;
        }
        Ok(())
    }
}

/// A bounded spelling view for older connected productions. It restores only
/// the current reveal's own query in the same immediate branch; complete print
/// verification must normalize its reparse to the original observation operands.
pub fn projection(ability: &Value) -> Result<Option<Value>, String> {
    let mut value = ability.clone();
    let mut state = State {
        restore: true,
        tracing: false,
        remaining: 131_072,
        bytes: 4 * 1024 * 1024,
        changed: false,
    };
    Walker::new(super::schema()?).walk(
        &mut value,
        &Ty::Named("Ability".into()),
        false,
        "",
        0,
        &mut state,
    )?;
    Ok(state.changed.then_some(value))
}

#[cfg(test)]
#[path = "observed_top_tests.rs"]
mod tests;
