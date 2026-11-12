//! "That creature is a Nightmare in addition to its other types" after a battlefield move
//! describes the object as it enters (CR 614.12), unlike a later "becomes" or "gains". The
//! characteristics join that move's arrival instead of following it as a continuous effect.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

const STATES: &str = "/args/destination/Battlefield/arrival/states";

fn states(operation: &Value) -> Option<&Vec<Value>> {
    (operation["kind"] == "Move")
        .then(|| operation.pointer(STATES)?.as_array())
        .flatten()
}

/// The move with `changes` as its last arrival state.
pub(crate) fn changes(operation: &Value, changes: &Value) -> Result<Value, String> {
    let Some(existing) = states(operation) else {
        return Err("entry characteristics need a move onto the battlefield".into());
    };
    if existing.iter().any(|state| state.get("Changes").is_some()) || !changes.is_array() {
        return Err("entry characteristics need one list of changes".into());
    }
    let mut output = operation.clone();
    if let Some(Value::Array(states)) = output.pointer_mut(STATES) {
        states.push(json!({"Changes": changes}));
    }
    Ok(output)
}

/// The printable move and characteristics of an arrival whose last state is its changes.
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [operation, written] = args else {
        return Vec::new();
    };
    let Some(Some(changes)) = states(value).map(|states| states.last()?.get("Changes")) else {
        return Vec::new();
    };
    let mut plain = value.clone();
    if let Some(Value::Array(states)) = plain.pointer_mut(STATES) {
        states.pop();
    }
    operation
        .unify_made(&plain, &bindings)
        .into_iter()
        .flat_map(|found| written.unify_made(changes, &found))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved(states: Value) -> Value {
        json!({"kind":"Move","args":{"objects":{"kind":"One","args":{"kind":"Source"}},
            "destination":{"Battlefield":{"controller":"Owner","arrival":{"controller":null,
            "tapped":false,"counters":[],"states":states}}},"qualifiers":[]}})
    }

    #[test]
    fn entry_characteristics_join_only_a_battlefield_arrival() {
        let nightmare = json!([{"kind":"CreatureTypes","args":{"mode":"Add",
            "types":{"Listed":["Nightmare"]}}}]);
        let entered = changes(&moved(json!([])), &nightmare).unwrap();
        assert_eq!(entered, moved(json!([{"Changes": nightmare.clone()}])));
        assert!(changes(&entered, &nightmare).is_err());
        let exiled = json!({"kind":"Move","args":{"destination":{"Exile":{}}}});
        assert!(changes(&exiled, &nightmare).is_err());
    }
}
