//! A Saga's explanatory sacrifice clause repeats the final chapter number.
use crate::pattern::{Bindings, Pattern};
use serde_json::Value;

fn last_chapter(chapters: &Value) -> Option<u64> {
    let chapters = chapters.as_array()?;
    if chapters.is_empty() || chapters.len() > 256 {
        return None;
    }
    let mut last = 0;
    for chapter in chapters {
        let chapter = if chapter["$sym"] == "scope" {
            &chapter["args"][0]
        } else {
            chapter
        };
        if chapter["kind"] != "Triggered" {
            return None;
        }
        let number = chapter["args"]["event"]["SagaChapter"]
            .as_u64()
            .filter(|n| *n != 0 && u32::try_from(*n).is_ok())?;
        last = last.max(number);
    }
    Some(last)
}

pub(crate) fn agree(chapters: &Value, printed: &Value) -> Result<Value, String> {
    if last_chapter(chapters).is_some_and(|last| printed.as_u64() == Some(last)) {
        Ok(chapters.clone())
    } else {
        Err("Saga reminder disagrees with the final chapter".into())
    }
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let ([chapters, printed], Some(last)) = (args, last_chapter(value)) else {
        return Vec::new();
    };
    chapters
        .unify(value, bindings)
        .into_iter()
        .flat_map(|binding| printed.unify_made(&Value::from(last), &binding))
        .collect()
}
