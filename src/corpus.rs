//! Reading the card data the tools share: Oracle texts and the IR's faces.
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Oracle text by face name, for every face of every card.
pub fn face_texts(path: &str) -> Result<HashMap<String, String>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let records: Vec<Value> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let mut out = HashMap::new();
    for record in &records {
        for card in record["cards"].as_array().map_or(&[][..], Vec::as_slice) {
            if let Some(name) = card["name"].as_str() {
                let text = card["text"].as_str().unwrap_or_default();
                out.entry(name.to_string())
                    .or_insert_with(|| text.to_string());
            }
        }
    }
    Ok(out)
}

/// Every name a sealed card goes by, whole and face by face.
pub fn sealed_names(path: &str) -> Result<BTreeSet<String>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let record: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let mut names = BTreeSet::new();
    for card in record["cards"]
        .as_array()
        .ok_or("sealed list has no cards")?
    {
        if let Some(name) = card["name"].as_str() {
            names.insert(name.to_string());
            names.extend(name.split(" // ").map(str::to_string));
        }
    }
    Ok(names)
}

/// What decides the noun a card calls itself, the most telling first.
pub fn print_context(face: &Value) -> Vec<String> {
    object_print_context(&face["characteristics"])
}

/// The same noun preferences apply to a token's own characteristics.
pub(crate) fn object_print_context(characteristics: &Value) -> Vec<String> {
    let strings = |field: &str| -> Vec<String> {
        characteristics[field]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut context = strings("supertypes");
    context.retain(|name| name == "Legendary");
    context.extend(strings("subtypes"));
    let types = strings("card_types");
    for name in ["Creature", "Land", "Artifact", "Enchantment"] {
        if types.iter().any(|kind| kind == name) {
            context.push(name.to_string());
        }
    }
    context.extend(types);
    context
}

/// Every face in a unified IR file, in file order.
pub fn faces(path: &str) -> Result<impl Iterator<Item = Result<Value, String>>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    Ok(BufReader::new(file).lines().flat_map(|line| {
        let parsed = line
            .map_err(|e| e.to_string())
            .and_then(|l| serde_json::from_str::<Value>(&l).map_err(|e| e.to_string()));
        match parsed {
            Ok(record) => {
                let items = record["faces"].as_array().cloned().unwrap_or_default();
                items.into_iter().map(Ok).collect::<Vec<_>>()
            }
            Err(e) => vec![Err(e)],
        }
    }))
}

/// Map `work` over `items`, keeping order. Default to two workers on shared hosts;
/// `ORACLE_GRAMMAR_THREADS` explicitly overrides that budget. Grammar work recurses,
/// so each worker gets a deep stack.
pub fn parallel<T: Sync, R: Send>(
    items: &[T],
    work: impl Fn(&T) -> R + Sync,
) -> Result<Vec<R>, String> {
    const STACK: usize = 1 << 28;
    let requested =
        std::env::var("ORACLE_GRAMMAR_THREADS")
            .map(Some)
            .or_else(|error| match error {
                std::env::VarError::NotPresent => Ok(None),
                _ => Err("ORACLE_GRAMMAR_THREADS must be a positive integer".to_string()),
            })?;
    let available = std::thread::available_parallelism().map_or(1, usize::from);
    let threads = worker_count(requested.as_deref(), available, items.len())?;
    let next = AtomicUsize::new(0);
    let mut done: Vec<(usize, R)> = std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for _ in 0..threads {
            let (next, work) = (&next, &work);
            let worker = std::thread::Builder::new()
                .stack_size(STACK)
                .spawn_scoped(scope, move || {
                    let mut out = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            return out;
                        };
                        out.push((index, work(item)));
                    }
                })
                .map_err(|e| e.to_string())?;
            workers.push(worker);
        }
        let mut all = Vec::with_capacity(items.len());
        for worker in workers {
            all.extend(worker.join().map_err(|_| "worker panicked".to_string())?);
        }
        Ok::<_, String>(all)
    })?;
    done.sort_by_key(|(index, _)| *index);
    Ok(done.into_iter().map(|(_, result)| result).collect())
}

fn worker_count(requested: Option<&str>, available: usize, items: usize) -> Result<usize, String> {
    let budget = match requested {
        None => 2,
        Some(value) => value
            .parse::<std::num::NonZeroUsize>()
            .map(usize::from)
            .map_err(|_| "ORACLE_GRAMMAR_THREADS must be a positive integer")?,
    };
    Ok(budget.min(available.max(1)).min(items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_budget_respects_the_host_and_amount_of_work() {
        assert_eq!(worker_count(None, 88, 100).unwrap(), 2);
        assert_eq!(worker_count(Some("4"), 88, 100).unwrap(), 4);
        assert_eq!(worker_count(Some("40"), 3, 100).unwrap(), 3);
        assert_eq!(worker_count(Some("40"), 88, 1).unwrap(), 1);
        assert_eq!(worker_count(None, 88, 0).unwrap(), 0);
        for invalid in ["0", "-1", "", "two"] {
            assert!(worker_count(Some(invalid), 88, 100).is_err());
        }
    }

    #[test]
    fn parallel_work_keeps_input_order() {
        assert_eq!(parallel(&[3, 1, 2], |n| n * n).unwrap(), [9, 1, 4]);
        assert!(parallel::<u32, u32>(&[], |n| *n).unwrap().is_empty());
    }
}
