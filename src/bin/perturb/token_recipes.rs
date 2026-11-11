//! Intrinsic token descriptions have words of their own. Rules-defined tokens
//! keep a fixed recipe, but their printed number is still a mutation site.
use super::{COLORS, SUBTYPES, next_of};
use serde_json::{Value, json};

pub(super) fn rules_defined(value: &Value) -> bool {
    value["words"] == "Rules" && value.get("recipe").is_some()
}

pub(super) fn sites(value: &Value, out: &mut Vec<(String, Value)>) {
    if value["words"] != "CreatingEffect" || !value["recipe"]["Defined"].is_object() {
        return;
    }
    for family in ["color", "selector", "type"] {
        if changed(value, family).is_some() {
            out.push((family.into(), value.clone()));
        }
    }
}

pub(super) fn changed(value: &Value, family: &str) -> Option<Value> {
    let mut copy = value.clone();
    let definition = copy.pointer_mut("/recipe/Defined")?;
    match family {
        "color" => {
            let chosen = definition["chosen_color"] == true;
            let colors = definition["colors"].as_array_mut()?;
            if colors.is_empty() && !chosen {
                colors.push(json!("White"));
            } else if !swap_list(colors, &COLORS) {
                return None;
            }
        }
        "selector" => {
            if !swap_list(definition["subtypes"].as_array_mut()?, &SUBTYPES) {
                return None;
            }
        }
        "type" => {
            let types = definition["card_types"].as_array_mut()?;
            // Keep the token a creature when its printed description has stats.
            // Add/change the artifact/enchantment adjective instead.
            if let Some(at) = types.iter().position(|kind| kind == "Artifact") {
                if types.iter().any(|kind| kind == "Enchantment") {
                    types.remove(at);
                } else {
                    types[at] = json!("Enchantment");
                }
            } else if let Some(at) = types.iter().position(|kind| kind == "Enchantment") {
                types[at] = json!("Artifact");
            } else if types.iter().any(|kind| kind == "Creature") {
                types.insert(0, json!("Artifact"));
            } else {
                return None;
            }
        }
        _ => return None,
    }
    Some(copy)
}

fn swap_list(items: &mut [Value], choices: &[&str]) -> bool {
    for at in 0..items.len() {
        let Some(next) = items[at].as_str().and_then(|old| next_of(choices, old)) else {
            continue;
        };
        if !items.iter().any(|item| item == next) {
            items[at] = json!(next);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{changed as change_site, renumbered, replaced, swaps};

    fn token(words: &str) -> Value {
        json!({"amount":{"kind":"Constant","args":1},"words":words,
            "recipe":{"Defined":{"colors":["Green"],"card_types":["Creature"],
                "subtypes":["Elf"],"power_toughness":[{"kind":"Constant","args":2},{"kind":"Constant","args":3}],
                "abilities":[{"kind":"Spell","args":{"body":{"instructions":[{"kind":"Effect","args":{"operation":{"kind":"Draw","args":{"amount":{"kind":"Constant","args":4}}}}}]}}}]}}})
    }

    #[test]
    fn rules_defined_recipes_are_fixed_but_the_printed_count_is_not() {
        let original = token("Rules");
        let mut sites = Vec::new();
        swaps(&original, &mut sites);
        assert_eq!(
            sites,
            vec![("number".into(), json!({"kind":"Constant","args":1}))]
        );
        let mut edited = original.clone();
        assert!(!renumbered(
            &mut edited,
            &json!({"kind":"Constant","args":4}),
            &json!({"kind":"Constant","args":5})
        ));
        assert_eq!(edited, original);
        assert!(renumbered(
            &mut edited,
            &sites[0].1,
            &json!({"kind":"Constant","args":2})
        ));
        assert_eq!(edited["recipe"], original["recipe"]);
        assert_eq!(
            replaced(&original, &json!("Green"), &json!("Blue")),
            original
        );
    }

    #[test]
    fn creating_effect_recipes_expose_written_colors_types_stats_and_abilities() {
        let original = token("CreatingEffect");
        let mut sites = Vec::new();
        swaps(&original, &mut sites);
        for family in ["color", "type", "selector"] {
            let (_, site) = sites.iter().find(|(name, _)| name == family).unwrap();
            let new = change_site(site, family).unwrap();
            assert_ne!(new, original);
            assert_eq!(new["amount"], original["amount"]);
            assert_eq!(
                new["recipe"]["Defined"]["abilities"],
                original["recipe"]["Defined"]["abilities"]
            );
            assert!(
                new["recipe"]["Defined"]["card_types"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("Creature"))
            );
        }
        for count in [2, 3, 4] {
            assert!(
                sites
                    .iter()
                    .any(|(family, value)| family == "number" && value["args"] == count)
            );
        }
    }
}
