//! Compare saved meanings through the pinned native canonicalizer only.
use super::{canonical_abilities, check};
use serde_json::{Value, json};

pub fn readings(row: &Value, gold: &Value) -> Result<Vec<Value>, String> {
    let values = if row["status"] == "refused" {
        Vec::new()
    } else if let Some(all) = row["all"].as_array() {
        all.clone()
    } else if row["status"] == "equal" {
        vec![gold.clone()]
    } else {
        vec![row["ours"].clone()]
    };
    if row["status"] != "refused" && Some(values.len() as u64) != row["readings"].as_u64() {
        return Err("saved reading set is incomplete".into());
    }
    Ok(values)
}

pub struct Preservation {
    old: Vec<Value>,
    explicit: usize,
    singletons: usize,
    total: usize,
    distinct: usize,
    merged: usize,
    missing: Vec<Value>,
    additions: Vec<Value>,
}

impl Preservation {
    pub fn new(old: Vec<Value>, current: &[Value]) -> Result<Self, String> {
        if old.len() != current.len()
            || old.iter().zip(current).any(|(a, b)| a["name"] != b["name"])
        {
            return Err("old/current score order or populations differ".into());
        }
        Ok(Self {
            old,
            explicit: 0,
            singletons: 0,
            total: 0,
            distinct: 0,
            merged: 0,
            missing: Vec::new(),
            additions: Vec::new(),
        })
    }

    pub fn inspect(
        &mut self,
        index: usize,
        current: &[Value],
        gold: &Value,
        face: &Value,
    ) -> Result<(), String> {
        let old = &self.old[index];
        let values = readings(old, gold)?;
        let explicit = old["all"].is_array() || old.get("ours").is_some();
        let mut distinct = Vec::new();
        for (reading, value) in values.iter().enumerate() {
            let normalized = if explicit {
                // Check the old operands before reducing: a new canonicalizer
                // must not conceal an invalid old reference frame.
                check(face, value)?;
                self.explicit += 1;
                canonical_abilities(value)?
            } else {
                self.singletons += 1;
                // Equal singletons were implicit in the unchanged source gold.
                gold.clone()
            };
            if !distinct.contains(&normalized) {
                distinct.push(normalized.clone());
            }
            if !current.contains(&normalized) {
                self.missing
                    .push(json!({"name":old["name"],"reading":reading}));
            }
        }
        self.total += values.len();
        self.distinct += distinct.len();
        self.merged += usize::from(distinct.len() < values.len());
        let added: Vec<_> = current
            .iter()
            .enumerate()
            .filter_map(|(i, value)| (!distinct.contains(value)).then_some(i))
            .collect();
        if !added.is_empty() {
            let retained: Vec<_> = current
                .iter()
                .enumerate()
                .filter_map(|(i, value)| distinct.contains(value).then_some(i))
                .collect();
            self.additions
                .push(json!({"occurrence":index,"name":old["name"],
                "previous_status":old["status"],"added_current_indices":added,
                "retained_current_indices":retained}));
        }
        Ok(())
    }

    pub fn passed(&self) -> bool {
        self.missing.is_empty()
    }

    pub fn report(&self) -> Value {
        json!({"occurrences":self.old.len(),"old_readings":self.total,
            "explicit_readings":self.explicit,"implicit_equal_singletons":self.singletons,
            "native_normalized_distinct_readings":self.distinct,"merged_faces":self.merged,
            "missing":self.missing,"passed":self.passed(),
            "additions":self.additions,
            "scope":"Old explicit full-card operands validate before native canonicalization; every old meaning must remain. Equal singletons use unchanged gold. Added current indices require semantic review: preserving old readings does not prove new ones correct. No parser, text rewrite or local equality alias."})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_requires_identical_order_and_complete_reading_sets() {
        assert!(Preservation::new(vec![json!({"name":"A"})], &[json!({"name":"B"})]).is_err());
        assert!(
            readings(
                &json!({"status":"different","readings":2,"ours":[]}),
                &json!([])
            )
            .is_err()
        );
    }

    #[test]
    fn an_implicit_singleton_cannot_disappear_under_a_canonical_pin_update() {
        let rows = vec![json!({"name":"A","status":"equal","readings":1})];
        let mut comparison = Preservation::new(rows.clone(), &rows).unwrap();
        comparison.inspect(0, &[], &json!([]), &json!({})).unwrap();
        assert!(!comparison.passed());
        assert_eq!(comparison.report()["old_readings"], 1);
    }

    #[test]
    fn added_readings_are_visible_even_when_the_old_match_is_preserved() {
        let old = vec![json!({"name":"A","status":"equal","readings":1})];
        let current = vec![json!({"name":"A","status":"equal","readings":2})];
        let extra =
            canonical_abilities(&json!([{"kind":"Keyword","args":{"kind":"Flying"}}])).unwrap();
        let mut comparison = Preservation::new(old, &current).unwrap();
        comparison
            .inspect(0, &[extra, json!([])], &json!([]), &json!({}))
            .unwrap();
        assert!(comparison.passed());
        assert_eq!(
            comparison.report()["additions"],
            json!([{"occurrence":0,"name":"A","previous_status":"equal",
                "added_current_indices":[0],"retained_current_indices":[1]}])
        );
    }

    #[test]
    fn newly_recognized_duplicate_names_keep_their_occurrence_identity() {
        let old = vec![
            json!({"name":"A","status":"equal","readings":1}),
            json!({"name":"A","status":"refused","readings":0}),
        ];
        let mut comparison = Preservation::new(old.clone(), &old).unwrap();
        comparison
            .inspect(0, &[json!([])], &json!([]), &json!({}))
            .unwrap();
        comparison
            .inspect(1, &[json!([])], &json!([]), &json!({}))
            .unwrap();
        assert_eq!(
            comparison.report()["additions"].as_array().unwrap().len(),
            1
        );
        assert_eq!(comparison.report()["additions"][0]["occurrence"], 1);
        assert_eq!(
            comparison.report()["additions"][0]["previous_status"],
            "refused"
        );
    }

    #[test]
    fn native_entry_merges_are_allowed_but_different_controller_meanings_are_not() {
        let control = json!({"kind":"AllOf","args":[{"kind":"CardType","args":"Creature"},
            {"kind":"ControlledBy","args":{"kind":"One","args":{"kind":"AbilityController"}}}]});
        let ability = |event: Value| {
            json!([{"kind":"Triggered","args":{"event":event,
            "limits":{"per_turn":null,"total":null},"intervening_if":null,
            "announcement":{"targets":[],"modes":null},"body":{"instructions":[{
                "kind":"Effect","args":{"result":0,"operation":{"kind":"Draw","args":{
                    "players":{"kind":"One","args":{"kind":"AbilityController"}},
                    "amount":{"kind":"Constant","args":1}}}}}]}}}])
        };
        let a = ability(json!({"ZoneChange":{"objects":control,"from":null,"to":"Battlefield"}}));
        let b = ability(json!({"Player":{"event":"EntersUnderControl",
            "players":{"kind":"One","args":{"kind":"AbilityController"}},"objects":control}}));
        let face = json!({"name":"A","schema_version":167,"characteristics":{
            "supertypes":[],"card_types":["Creature"],"subtypes":[],"mana_cost":null,
            "color_indicator":[],"power_toughness":[1,1],"loyalty":null,"defense":null}});
        let old = vec![json!({"name":"A","status":"equal","readings":2,"all":[a,b]})];
        let current = vec![json!({"name":"A","status":"equal","readings":1})];
        let meaning = canonical_abilities(&a).unwrap();
        let mut comparison = Preservation::new(old.clone(), &current).unwrap();
        comparison
            .inspect(0, std::slice::from_ref(&meaning), &meaning, &face)
            .unwrap();
        assert!(comparison.passed());
        assert_eq!(comparison.report()["merged_faces"], 1);
        assert!(
            comparison.report()["additions"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let mut other = meaning.clone();
        other[0]["args"]["event"]["Player"]["objects"] =
            json!({"kind":"CardType","args":"Creature"});
        let mut comparison = Preservation::new(old, &current).unwrap();
        comparison.inspect(0, &[other], &meaning, &face).unwrap();
        assert!(!comparison.passed());
    }
}
