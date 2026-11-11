//! Class labels are positional IR data, checked before discarding their spelling.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

// A resource limit, not a two-/three-level semantic restriction. Both directions
// bound their own linear allocation even for caller-supplied synthesized IR.
const MAX_LEVELS: usize = 4096;

fn number(index: usize) -> Option<i64> {
    i64::try_from(index).ok()?.checked_add(2)
}

pub(crate) fn lower(value: &Value) -> Result<Value, String> {
    let rows = value.as_array().ok_or("Class labels need a tuple list")?;
    if rows.is_empty() || rows.len() > MAX_LEVELS {
        return Err("Class level list is empty or exceeds its work limit".into());
    }
    let mut levels = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let pair = row.as_array().ok_or("Class label row needs a pair")?;
        let [label, level] = pair.as_slice() else {
            return Err("Class label row needs exactly two operands".into());
        };
        let expected = number(index).ok_or("Class label index overflow")?;
        if label.as_i64() != Some(expected) {
            return Err("Class labels must be consecutive from level 2".into());
        }
        levels.push(level.clone());
    }
    Ok(Value::Array(levels))
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [rows] = args else { return Vec::new() };
    let Some(levels) = value.as_array() else {
        return Vec::new();
    };
    if levels.is_empty() || levels.len() > MAX_LEVELS {
        return Vec::new();
    }
    let Some(labelled) = levels
        .iter()
        .enumerate()
        .map(|(index, level)| number(index).map(|n| json!([n, level])))
        .collect::<Option<Vec<_>>>()
    else {
        return Vec::new();
    };
    rows.unify_made(&Value::Array(labelled), &bindings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ir_schema::Ty;
    use crate::pattern::Pattern;

    #[test]
    fn tuple_labels_lower_and_inverse_uses_current_vector() {
        let level = json!({"cost":{"parts":[]},"abilities":[]});
        for count in [1, 2, 3, 17] {
            let levels = Value::Array(vec![level.clone(); count]);
            let rows = Value::Array((0..count).map(|i| json!([i + 2, level])).collect());
            assert_eq!(lower(&rows).unwrap(), levels);
            let args = [Pattern::Hole("rows".into(), Ty::Opaque)];
            let found = unify(&args, &levels, Bindings::new());
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].get("rows"), Some(&rows));
        }
    }

    #[test]
    fn invalid_empty_skipped_repeated_or_reordered_labels_refuse() {
        for labels in [
            vec![],
            vec![0],
            vec![1],
            vec![3],
            vec![2, 4],
            vec![2, 2],
            vec![3, 2],
        ] {
            let rows = Value::Array(labels.into_iter().map(|n| json!([n, {}])).collect());
            assert!(lower(&rows).is_err(), "{rows}");
        }
        for malformed in [json!([[2]]), json!([[2, {}, {}]]), json!([["2", {}]])] {
            assert!(lower(&malformed).is_err());
        }
        assert!(lower(&Value::Array(vec![json!([2, {}]); MAX_LEVELS + 1])).is_err());
        assert_eq!(number(usize::MAX), None);
    }
}
