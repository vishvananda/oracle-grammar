//! Result addresses follow the same reading order as their operand references.
use super::{Scope, Step};
use serde_json::{Value, json};

/// An earlier lexical instruction supersedes an event count. Generated value
/// definitions consume result addresses without becoming such antecedents.
pub(super) fn quantity(
    root: &Value,
    scope: &Scope,
    many: bool,
    path: &[Step],
) -> Result<Value, String> {
    if !many && unresolved_mana_quantity(root, path) {
        return Err(
            "that much after a mana-spent test needs its explicit numeric antecedent".into(),
        );
    }
    if let Some(quantity) = scope
        .event_quantity
        .filter(|_| !many || scope.results.lexical_issued() < 2)
    {
        return Ok(quantity.expression());
    }
    let result = scope.results.back(2)?;
    if many
        && draw_result(
            root,
            &serde_json::to_value(result).map_err(|e| e.to_string())?,
            true,
        )
    {
        return Err("that many after drawing requires a captured requested amount".into());
    }
    Ok(if many {
        json!({"kind":"CountObjects","args":{
            "kind":"Transition","args":{"result":result,"phase":"Before"}
        }})
    } else {
        json!({"kind":"Performed","args":result})
    })
}

// A one-object sacrifice supplies no lexical amount for `that much`. In this
// immediate branch the written numeric antecedent is the mana-spent predicate.
// Decline the unsupported implicit recipe instead of guessing Performed(1).
// Explicit ManaSpent expressions and every unrelated quantity keep their route.
fn unresolved_mana_quantity(root: &Value, path: &[Step]) -> bool {
    let Some(at) = path.len().checked_sub(8) else {
        return false;
    };
    let suffix = &path[at..];
    let expected = [
        "args",
        "then",
        "instructions",
        "",
        "args",
        "operation",
        "args",
        "amount",
    ];
    if !suffix
        .iter()
        .zip(expected)
        .enumerate()
        .all(|(i, (step, key))| {
            if i == 3 {
                matches!(step, Step::Index(1))
            } else {
                matches!(step, Step::Key(found) if found == key)
            }
        })
    {
        return false;
    }
    let Some(branch) = path[..at].iter().try_fold(root, |node, step| match step {
        Step::Key(key) => node.get(key),
        Step::Index(index) => node.get(*index),
    }) else {
        return false;
    };
    let args = &branch["args"];
    let condition = &args["condition"];
    let comparison = &condition["args"]["filter"]["args"];
    let mana = &comparison["args"]["left"];
    let Some([sacrifice, damage]) = args["then"]["instructions"].as_array().map(Vec::as_slice)
    else {
        return false;
    };
    let mut sacrifice = &sacrifice["args"]["operation"];
    if super::symbol_name(sacrifice) == Some("sacrificed_result") {
        let Some([inner]) = sacrifice["args"].as_array().map(Vec::as_slice) else {
            return false;
        };
        sacrifice = inner;
    }
    let damage = &damage["args"]["operation"];
    branch["kind"] == "If"
        && args["otherwise"]["instructions"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && condition["kind"] == "ObjectMatches"
        && condition["args"]["filter"]["kind"] == "Condition"
        && comparison["kind"] == "Compare"
        && comparison["args"]["test"]["args"]["comparison"] == "GreaterOrEqual"
        && comparison["args"]["test"]["args"]["value"]["kind"] == "Constant"
        && comparison["args"]["test"]["args"]["value"]["args"]
            .as_i64()
            .is_some()
        && mana["kind"] == "ManaSpent"
        && mana["args"]["object"]["kind"] == "Candidate"
        && mana["args"]["aggregation"] == "Count"
        && sacrifice["kind"] == "Sacrifice"
        && sacrifice["args"]["players"] == json!({"kind":"One","args":{"kind":"AbilityController"}})
        && sacrifice["args"]["objects"] == json!({"kind":"One","args":{"kind":"Source"}})
        && sacrifice["args"]["choose"].is_null()
        && damage["kind"] == "DealDamage"
}

pub(super) fn result(name: &str, node: &Value, scope: &mut Scope) -> Result<Value, String> {
    Ok(match name {
        "next" => {
            json!(scope.results.allocate()?)
        }
        // A result later instructions keep naming, however many follow it.
        "mark" | "choice_mark" => {
            if name == "choice_mark"
                && (scope.defined_x || scope.possible_announced_x || scope.snapshot_payment)
            {
                return Err("X is already defined before this resolving choice".into());
            }
            let result = scope.results.allocate()?;
            scope.marked = Some(result);
            json!(result)
        }
        "marked" => json!(scope.marked.ok_or("nothing was chosen earlier")?),
        // A result counted back from the one being written.
        "back" => {
            let back = node["args"][0].as_u64().ok_or("how far back")?;
            json!(scope.results.back(back)?)
        }
        // The instruction before the one this mention sits in.
        "prior" => json!(scope.results.back(2)?),
        // The instruction just finished, from a sibling that makes no result.
        "last_result" => json!(scope.results.back(1)?),
        _ => return Err(format!("unknown result symbol {name}")),
    })
}

// Distinguish a requested draw quantity from actual moved-card receipts.
// A nested independent ability cannot supply this scope's antecedent.
pub(super) fn draw_result(value: &Value, result: &Value, root: bool) -> bool {
    use crate::traverse::{Boundary, boundary};
    if !root
        && matches!(
            boundary(value),
            Boundary::Ability | Boundary::Holder | Boundary::Lexical | Boundary::Reflexive
        )
    {
        return false;
    }
    if value["kind"] == "Effect"
        && value["args"]["result"] == *result
        && value["args"]["operation"]["kind"] == "Draw"
    {
        return true;
    }
    match value {
        Value::Object(fields) => fields.values().any(|v| draw_result(v, result, false)),
        Value::Array(items) => items.iter().any(|v| draw_result(v, result, false)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch() -> Value {
        json!({"kind":"If","args":{"condition":{"kind":"ObjectMatches","args":{"object":{"kind":"EventSubject"},"filter":{"kind":"Condition","args":{"kind":"Compare","args":{"left":{"kind":"ManaSpent","args":{"object":{"kind":"Candidate"},"filter":{"colors":null,"snow":false},"aggregation":"Count"}},"test":{"kind":"Compare","args":{"comparison":"GreaterOrEqual","value":{"kind":"Constant","args":8}}}}}}}},"then":{"instructions":[
                {"kind":"Effect","args":{"result":0,"operation":{"$sym":"sacrificed_result","args":[{"kind":"Sacrifice","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"objects":{"kind":"One","args":{"kind":"Source"}},"choose":null}}],"at":20}}},
                {"kind":"Effect","args":{"result":1,"operation":{"kind":"DealDamage","args":{"source":{"$sym":"member_count_it","args":[{"kind":"Source"}],"at":24},"recipients":{"kind":"Players","args":{"kind":"OpponentsOf","args":{"kind":"AbilityController"}}},"amount":{"$sym":"that_much","args":[],"at":27},"preventable":true}}}}
            ]},"otherwise":{"instructions":[]}}})
    }

    fn amount_path() -> Vec<Step> {
        [
            "args",
            "then",
            "instructions",
            "",
            "args",
            "operation",
            "args",
            "amount",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, key)| {
            if i == 3 {
                Step::Index(1)
            } else {
                Step::Key(key.into())
            }
        })
        .collect()
    }

    #[test]
    fn mana_threshold_cannot_make_self_sacrifice_the_implicit_damage_amount() {
        let value = branch();
        assert!(unresolved_mana_quantity(&value, &amount_path()));
        assert!(quantity(&value, &Scope::default(), false, &amount_path()).is_err());
        let mut nested = json!({"kind":"If","args":{"then":{"instructions":[value]}}});
        let mut path = vec![
            Step::Key("args".into()),
            Step::Key("then".into()),
            Step::Key("instructions".into()),
            Step::Index(0),
        ];
        path.extend(amount_path());
        assert!(unresolved_mana_quantity(&nested, &path));
        nested["args"]["then"]["instructions"][0]["args"]["then"]["instructions"][0]["args"]["operation"] =
            json!({"kind":"Draw"});
        assert!(!unresolved_mana_quantity(&nested, &path));
    }

    #[test]
    fn unrelated_conditions_plural_counts_and_nested_callbacks_keep_old_routes() {
        let mut value = branch();
        value["args"]["condition"]["args"]["filter"]["args"]["args"]["left"]["kind"] =
            json!("Constant");
        assert!(!unresolved_mana_quantity(&value, &amount_path()));
        let mut value = branch();
        value["args"]["then"]["instructions"][0]["args"]["operation"]["args"][0]["args"]["choose"] =
            json!({"AnyNumber":null});
        assert!(!unresolved_mana_quantity(&value, &amount_path()));
        let value = branch();
        let mut scope = Scope::default();
        scope.results.allocate().unwrap();
        scope.results.allocate().unwrap();
        assert!(quantity(&value, &scope, true, &amount_path()).is_ok());
        let mut path = amount_path();
        path.extend([Step::Key("args".into()), Step::Index(0)]);
        assert!(!unresolved_mana_quantity(&value, &path));
    }
}
