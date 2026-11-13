//! An explicit where-clause snapshots its value immediately before its first use.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

#[cfg(test)]
mod shared_tests;

/// Two eager X amounts share the definition named by their clause, including
/// after an unrelated instruction or with unrelated instructions between them.
/// Re-evaluating the expression after the first action can change its meaning.
pub(crate) fn shared(body: &Value) -> bool {
    let Some(instructions) = body["instructions"].as_array() else {
        return false;
    };
    instructions
        .iter()
        .filter(|instruction| {
            instruction["kind"] == "Effect"
                && matches!(
                    instruction["args"]["operation"]["kind"].as_str(),
                    Some("DealDamage" | "GainLife" | "LoseLife" | "Draw" | "Mill")
                )
                && super::is_x(&instruction["args"]["operation"]["args"]["amount"])
        })
        .count()
        == 2
        && x_count(body) == 2
}

/// Pumps retain their existing scalar/aggregate definition contract.
fn aggregate_definition(amount: &Value) -> bool {
    match amount["kind"].as_str() {
        Some(
            "Constant" | "PlayerNumber" | "CountObjects" | "Aggregate" | "CountDistinct"
            | "ManaSpent" | "CountManaSymbols",
        ) => true,
        Some("Add" | "Multiply") => amount["args"]
            .as_array()
            .is_some_and(|parts| !parts.is_empty() && parts.iter().all(aggregate_definition)),
        _ => false,
    }
}

/// Eager amounts can also share party/mutation counts and a proven Saga's mana
/// value. Other properties still require the independent sacrifice/LKI proof.
pub(crate) fn shared_definition(amount: &Value) -> bool {
    match amount["kind"].as_str() {
        Some("PartySize" | "TimesMutated" | "CountCounters") => true,
        Some("ObjectProperty") => {
            let args = &amount["args"];
            let object = &args["object"];
            args["property"] == "ManaValue"
                && args["information"] == "CurrentOrLastKnown"
                && (object["kind"] == "EventSubject"
                    || object["$sym"] == "qualified_object"
                        && object["args"] == json!(["Permanent", {"kind":"Subtype","args":"Saga"}]))
        }
        Some("Add" | "Multiply") => amount["args"]
            .as_array()
            .is_some_and(|parts| !parts.is_empty() && parts.iter().all(shared_definition)),
        _ => aggregate_definition(amount),
    }
}

/// Use the production's source position only as a private binder key. It is not
/// a native result address; the binder allocates that address in program order.
pub(crate) fn snapshot(body: &Value, amount: &Value, at: usize) -> Result<Value, String> {
    if !eligible(body) {
        return Err("snapshot requires supported immediate X operands".into());
    }
    let paid_property = shared(body)
        && !shared_definition(amount)
        && paid_property_candidate(amount)
        && body["instructions"]
            .as_array()
            .is_some_and(|items| items.len() == 2);
    if !definition_eligible(body, amount) {
        return Err("shared snapshot requires a supported definition".into());
    }
    let first_use = body["instructions"]
        .as_array()
        .and_then(|items| items.iter().position(|item| x_count(item) > 0))
        .ok_or("where snapshot has no X use")?;
    let reference = json!({"$sym":"snapshot_ref", "args":[at], "at":at});
    let mut program = super::substituted(body.clone(), &reference);
    if &program == body {
        return Err("where snapshot has no X use".into());
    }
    let instructions = program["instructions"]
        .as_array_mut()
        .ok_or("where snapshot requires a program")?;
    let x = json!({"kind":"AnnouncedX"});
    let marked = super::substituted(json!({"value":x}), amount)["value"].clone();
    let marked = if paid_property {
        json!({"$sym":"paid_sacrifice_definition", "args":[marked], "at":at})
    } else {
        marked
    };
    instructions.insert(
        first_use,
        json!({"kind":"Effect","args":{
            "result":{"$sym":"snapshot_next","args":[at],"at":at},
            "operation":{"kind":"BindValue","args":{"value":marked}}
        }}),
    );
    Ok(program)
}

/// Reconstruct the clause from its current native definition and uses. Never
/// inline unrelated computed expressions or remove another instruction's result.
pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    unify_inner(args, value, bindings, false)
}

pub(crate) fn unify_shared<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    unify_inner(args, value, bindings, true)
}

fn unify_inner<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
    shared_only: bool,
) -> Vec<Bindings<'p, 'v>> {
    let [body, amount] = args else {
        return Vec::new();
    };
    let Some(instructions) = value["instructions"].as_array() else {
        return Vec::new();
    };
    instructions
        .iter()
        .enumerate()
        .filter(|(_, instruction)| {
            instruction["kind"] == "Effect"
                && instruction["args"]["operation"]["kind"] == "BindValue"
        })
        .flat_map(|(index, _)| unify_at(body, amount, value, &bindings, shared_only, index))
        .collect()
}

fn unify_at<'p, 'v>(
    body: &'p Pattern,
    amount: &'p Pattern,
    value: &'v Value,
    bindings: &Bindings<'p, 'v>,
    shared_only: bool,
    index: usize,
) -> Vec<Bindings<'p, 'v>> {
    let instruction = &value["instructions"][index];
    let Some(id) = instruction["args"]["result"].as_u64() else {
        return Vec::new();
    };
    let definition = &instruction["args"]["operation"]["args"]["value"];
    let mut abstracted = value.clone();
    abstracted["instructions"]
        .as_array_mut()
        .unwrap()
        .remove(index);
    let reference = json!({"kind":"BoundValue","args":id});
    let before = abstracted.clone();
    crate::traverse::replace(&mut abstracted, &reference, &json!({"kind":"AnnouncedX"}));
    if before == abstracted
        || abstracted["instructions"]
            .as_array()
            .and_then(|items| items.iter().position(|item| x_count(item) > 0))
            != Some(index)
        || !eligible(&abstracted)
        || shared_only && !shared(&abstracted)
        || !definition_eligible(&abstracted, definition)
    {
        return Vec::new();
    }
    amount
        .unify_made(definition, bindings)
        .into_iter()
        .flat_map(|b| body.unify_made(&abstracted, &b))
        .collect()
}

/// Immediate operands and one end-of-turn snapshot Pump evaluate before their
/// instruction changes game state. Other lifetime/capture shapes stay excluded.
fn eligible(body: &Value) -> bool {
    if shared(body) || paired_pumps(body) {
        return true;
    }
    let Some(instructions) = body["instructions"].as_array() else {
        return false;
    };
    let [instruction] = instructions.as_slice() else {
        return false;
    };
    if instruction["kind"] != "Effect" {
        return false;
    }
    let operation = &instruction["args"]["operation"];
    if initial_token(body, operation) {
        return true;
    }
    if operation["kind"] == "CreateContinuousEffect" {
        return pump_uses(operation).is_some_and(|uses| x_count(body) == uses);
    }
    let operand = match operation["kind"].as_str() {
        Some("DealDamage" | "GainLife" | "LoseLife" | "Draw" | "Mill") => {
            &operation["args"]["amount"]
        }
        Some("Scry")
            if operation["args"]["players"]
                == json!({"kind":"One","args":{"kind":"AbilityController"}}) =>
        {
            &operation["args"]["amount"]
        }
        Some("PutCounters") => {
            let Some(placements) = operation["args"]["placements"].as_array() else {
                return false;
            };
            let [placement] = placements.as_slice() else {
                return false;
            };
            if !placement["selection"].is_null() {
                return false;
            }
            &placement["amount"]
        }
        _ => return false,
    };
    super::is_x(operand) && x_count(body) == 1
}

/// This is only a candidate: the binder must prove its resolved property
/// belongs to the exact positively chosen resolution sacrifice's Before view.
fn paid_property_candidate(value: &Value) -> bool {
    if value["kind"] != "ObjectProperty" || value["args"]["information"] != "CurrentOrLastKnown" {
        return false;
    }
    let object = &value["args"]["object"];
    matches!(
        object["$sym"].as_str(),
        Some(
            "activation_creature_property"
                | "named_object"
                | "defined_named_object"
                | "that_object"
                | "defined_object"
                | "qualified_object"
        )
    ) || object["kind"] == "Query"
        && object["args"]["objects"]["kind"] == "CostObjects"
        && object["args"]["objects"]["args"]["view"] == "Inputs"
        && object["args"]["objects"]["args"]["phase"] == "Before"
        && object["args"]["objects"]["args"]["payment"]
            .get("Resolution")
            .is_some()
}

/// CreatingEffect initial copiable values are sampled before the entire batch
/// (IR CreateTokens contract). This is distinct from live token ability/CDA X.
fn initial_token(body: &Value, operation: &Value) -> bool {
    // The token verb records its produced batch through this exact transparent
    // binder annotation. It is not an independent holder/scope or operation.
    let operation = if operation["$sym"] == "created_result" {
        let Some([operation]) = operation["args"].as_array().map(Vec::as_slice) else {
            return false;
        };
        operation
    } else {
        operation
    };
    if operation["kind"] != "CreateTokens" || operation["args"]["players"]["kind"] != "One" {
        return false;
    }
    let Some([token]) = operation["args"]["tokens"].as_array().map(Vec::as_slice) else {
        return false;
    };
    if token["words"] != "CreatingEffect" || super::constant(&token["amount"]) != Some(1) {
        return false;
    }
    let definition = &token["recipe"]["Defined"];
    // Keep independent definitions entirely outside this first batch contract,
    // including mana-X declarations which are not Expr::AnnouncedX nodes.
    if !definition["abilities"]
        .as_array()
        .is_some_and(Vec::is_empty)
        || !definition["abilities_of"].is_null()
    {
        return false;
    }
    let Some([power, toughness]) = definition["power_toughness"].as_array().map(Vec::as_slice)
    else {
        return false;
    };
    super::is_x(power) && super::is_x(toughness) && x_count(body) == 2
}

/// Snapshot evaluates both changes before applying the continuous effect. A
/// leading BindValue has that same instant and retains its number for the effect
/// lifetime; it never crosses another instruction, creates a holder, or goes live.
fn pump_uses(operation: &Value) -> Option<usize> {
    if operation["kind"] != "CreateContinuousEffect" {
        return None;
    }
    let args = &operation["args"];
    if args["evaluation"] != "Snapshot" || args["duration"] != "EndOfTurn" {
        return None;
    }
    let changes = args["changes"].as_array()?;
    let [change] = changes.as_slice() else {
        return None;
    };
    if change["kind"] != "Pump" {
        return None;
    }
    let power = &change["args"]["power"];
    let toughness = &change["args"]["toughness"];
    let supported = |operand: &Value| {
        super::is_x(operand)
            || operand["kind"] == "Constant" && operand["args"].as_i64().is_some()
            || operand["kind"] == "Multiply"
                && operand["args"].as_array().is_some_and(|parts| {
                    matches!(parts.as_slice(), [negative, x]
                    if super::constant(negative) == Some(-1) && super::is_x(x))
                })
    };
    let uses = x_count(power) + x_count(toughness);
    (supported(power) && supported(toughness) && uses > 0 && x_count(operation) == uses)
        .then_some(uses)
}

/// A separate opt-in retains the ordinary inline @where reading. Each pure
/// end-of-turn Snapshot pump contains one signed X use and no hidden X slot.
fn paired_pumps(body: &Value) -> bool {
    body["instructions"].as_array().is_some_and(|items| {
        items.len() == 2
            && items.iter().all(|item| {
                item["kind"] == "Effect" && pump_uses(&item["args"]["operation"]) == Some(1)
            })
            && x_count(body) == 2
    })
}

/// Exactly the same definition gate serves forward capture and inverse view.
/// Paired pumps do not borrow the sacrifice/property exception of eager amounts.
fn definition_eligible(body: &Value, amount: &Value) -> bool {
    if paired_pumps(body) {
        return aggregate_definition(amount);
    }
    !shared(body)
        || shared_definition(amount)
        || paid_property_candidate(amount)
            && body["instructions"]
                .as_array()
                .is_some_and(|items| items.len() == 2)
}

fn x_count(value: &Value) -> usize {
    if super::is_x(value) {
        return 1;
    }
    match value {
        Value::Object(fields) => fields.values().map(x_count).sum(),
        Value::Array(items) => items.iter().map(x_count).sum(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(amount: Value) -> Value {
        json!({"instructions":[{"kind":"Effect","args":{
            "result":{"$sym":"next","args":[],"at":10},
            "operation":{"kind":"Draw","args":{
                "players":{"kind":"One","args":{"kind":"AbilityController"}},
                "amount":amount
            }}
        }}]})
    }

    #[test]
    fn definition_uses_private_key_instead_of_native_token_address() {
        let body = draw(json!({"kind":"AnnouncedX"}));
        let amount = json!({"kind":"PlayerNumber","args":{
            "player":{"kind":"AbilityController"},"property":"Life"
        }});
        let program = snapshot(&body, &amount, 800).unwrap();
        assert_eq!(
            program["instructions"][0]["args"]["result"],
            json!({"$sym":"snapshot_next","args":[800],"at":800})
        );
        assert_eq!(
            program["instructions"][0]["args"]["operation"]["args"]["value"],
            amount
        );
        assert_eq!(
            program["instructions"][1]["args"]["operation"]["args"]["amount"],
            json!({"$sym":"snapshot_ref","args":[800],"at":800})
        );
    }

    #[test]
    fn counter_choices_remain_outside_snapshot_contract() {
        let body = json!({"instructions":[{"kind":"Effect","args":{
            "result":{"$sym":"next","args":[],"at":0},
            "operation":{"kind":"PutCounters","args":{"placements":[{
                "amount":{"kind":"AnnouncedX"},"selection":{"player":{"kind":"AbilityController"}},
                "recipients":{"kind":"Objects","args":{"kind":"One","args":{"kind":"Source"}}},
                "kind":{"PowerToughness":[1,1]}
            }]}}
        }}]});
        assert!(snapshot(&body, &json!({"kind":"Constant","args":2}), 0).is_err());
    }

    fn pump() -> Value {
        json!({"instructions":[{"kind":"Effect","args":{
            "result":{"$sym":"next","args":[],"at":0},
            "operation":{"kind":"CreateContinuousEffect","args":{
                "source":{"kind":"Source"},"affected":{"Objects":{"kind":"One","args":{"kind":"Source"}}},
                "changes":[{"kind":"Pump","args":{"power":{"kind":"AnnouncedX"},"toughness":{"kind":"AnnouncedX"}}}],
                "duration":"EndOfTurn","evaluation":"Snapshot"
            }}
        }}]})
    }

    #[test]
    fn repeated_snapshot_pump_captures_one_number_for_both_operands() {
        let value = snapshot(&pump(), &json!({"kind":"Constant","args":3}), 0).unwrap();
        let operands = &value["instructions"][1]["args"]["operation"]["args"]["changes"][0]["args"];
        assert_eq!(operands["power"], operands["toughness"]);
        assert_eq!(operands["power"]["$sym"], "snapshot_ref");
        assert_eq!(value["instructions"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn live_durable_granted_and_multi_instruction_pumps_do_not_opt_in() {
        let amount = json!({"kind":"Constant","args":3});
        for (field, replacement) in [
            ("evaluation", json!("Live")),
            ("evaluation", json!("LiveObjects")),
            ("duration", json!("Indefinite")),
            ("source", json!({"kind":"AnnouncedX"})),
        ] {
            let mut body = pump();
            body["instructions"][0]["args"]["operation"]["args"][field] = replacement;
            assert!(snapshot(&body, &amount, 0).is_err());
        }
        let mut body = pump();
        body["instructions"][0]["args"]["operation"]["args"]["changes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"kind":"GrantKeyword","args":{"kind":"Haste"}}));
        assert!(snapshot(&body, &amount, 0).is_err());
        let mut body = pump();
        body["instructions"]
            .as_array_mut()
            .unwrap()
            .push(draw(json!({"kind":"AnnouncedX"}))["instructions"][0].clone());
        assert!(snapshot(&body, &amount, 0).is_err());
    }

    #[test]
    fn paired_eager_uses_capture_once_after_an_unrelated_prefix() {
        let x = json!({"kind":"AnnouncedX"});
        let amount = json!({"kind":"Constant","args":4});
        let mut repeated = draw(x.clone());
        repeated["instructions"]
            .as_array_mut()
            .unwrap()
            .push(draw(x.clone())["instructions"][0].clone());
        let captured = snapshot(&repeated, &amount, 0).unwrap();
        assert_eq!(captured["instructions"].as_array().unwrap().len(), 3);
        assert_eq!(
            captured["instructions"][1]["args"]["operation"]["args"]["amount"],
            captured["instructions"][2]["args"]["operation"]["args"]["amount"]
        );
        repeated["instructions"]
            .as_array_mut()
            .unwrap()
            .insert(0, draw(amount.clone())["instructions"][0].clone());
        assert!(shared(&repeated));
        let captured = snapshot(&repeated, &amount, 0).unwrap();
        assert_eq!(captured["instructions"][0], repeated["instructions"][0]);
        assert_eq!(
            captured["instructions"][1]["args"]["operation"]["kind"],
            "BindValue"
        );
    }

    #[test]
    fn local_payments_and_grants_remain_outside_snapshot_contract() {
        let x = json!({"kind":"AnnouncedX"});
        let amount = json!({"kind":"Constant","args":4});
        let mut payment = draw(x.clone());
        payment["instructions"][0]["args"]["operation"]["kind"] = json!("PayCost");
        assert!(snapshot(&payment, &amount, 0).is_err());
        let mut grant = draw(x);
        grant["instructions"][0]["args"]["operation"]["kind"] = json!("GrantAbility");
        assert!(snapshot(&grant, &amount, 0).is_err());
        assert!(snapshot(&draw(amount.clone()), &amount, 0).is_err());
    }

    #[test]
    fn shared_capture_excludes_incarnation_sensitive_definitions_and_future_uses() {
        let x = json!({"kind":"AnnouncedX"});
        let mut pair = draw(x.clone());
        pair["instructions"]
            .as_array_mut()
            .unwrap()
            .push(draw(x.clone())["instructions"][0].clone());
        let definition = json!({"kind":"ObjectProperty","args":{"object":{"kind":"Source"},"property":"Power","information":"CurrentOrLastKnown"}});
        assert!(!shared_definition(&definition));
        assert!(snapshot(&pair, &definition, 0).is_err());
        let future = json!({"kind":"Effect","args":{"operation":{"kind":"CreateDelayedTrigger","args":{"body":draw(x)}}}});
        pair["instructions"].as_array_mut().unwrap().push(future);
        assert!(!shared(&pair));
        assert!(snapshot(&pair, &json!({"kind":"Constant","args":3}), 0).is_err());
    }

    #[test]
    fn non_x_tail_retains_order_and_pair_recipients_are_not_snapshotted() {
        let x = json!({"kind":"AnnouncedX"});
        let mut pair = draw(x.clone());
        pair["instructions"]
            .as_array_mut()
            .unwrap()
            .push(draw(x)["instructions"][0].clone());
        let tail = draw(json!({"kind":"Constant","args":1}))["instructions"][0].clone();
        pair["instructions"]
            .as_array_mut()
            .unwrap()
            .push(tail.clone());
        let captured = snapshot(&pair, &json!({"kind":"Constant","args":3}), 5).unwrap();
        assert_eq!(captured["instructions"][3], tail);
        for index in [1, 2] {
            assert_eq!(
                captured["instructions"][index]["args"]["operation"]["args"]["players"],
                pair["instructions"][index - 1]["args"]["operation"]["args"]["players"]
            );
        }
    }
    fn initial_token_body() -> Value {
        json!({"instructions":[{"kind":"Effect","args":{"result":0,"operation":{"kind":"CreateTokens","args":{"players":{"kind":"One","args":{"kind":"AbilityController"}},"tokens":[{"amount":{"kind":"Constant","args":1},"words":"CreatingEffect","recipe":{"Defined":{"power_toughness":[{"kind":"AnnouncedX"},{"kind":"AnnouncedX"}],"abilities":[]}}}],"entry":{"tapped":false,"counters":[],"states":[]}}}}}]})
    }

    #[test]
    fn declared_token_result_wrapper_and_typed_receipt_noun_are_candidates() {
        let mut body = initial_token_body();
        let operation = body["instructions"][0]["args"]["operation"].take();
        body["instructions"][0]["args"]["operation"] =
            json!({"$sym":"created_result","args":[operation.clone()],"at":0});
        assert!(eligible(&body));
        for wrapper in ["scope", "holder", "reflexive"] {
            body["instructions"][0]["args"]["operation"] =
                json!({"$sym":wrapper,"args":[operation.clone()],"at":0});
            assert!(!eligible(&body));
        }
        let definition = json!({"kind":"ObjectProperty","args":{
            "object":{"$sym":"qualified_object","args":["Permanent",{"kind":"CardType","args":"Creature"}],"at":30},
            "property":"Power","information":"CurrentOrLastKnown"
        }});
        assert!(paid_property_candidate(&definition));
    }

    #[test]
    fn initial_token_inverse_enumerates_both_typed_slots_once() {
        let expression = json!({"kind":"CountObjects","args":{"kind":"Selected","args":7}});
        let recipe = json!({"Defined":{"power_toughness":[expression.clone(),expression.clone()]}});
        let mut found = Vec::new();
        super::super::computed_numbers(&recipe, &mut found);
        assert_eq!(found, vec![expression]);
        let mut found = Vec::new();
        super::super::computed_numbers(
            &json!({"power_toughness":[{"kind":"CountObjects","args":{"kind":"Selected","args":7}}]}),
            &mut found,
        );
        assert!(found.is_empty());
    }

    #[test]
    fn token_batch_capture_retains_the_equivalent_inline_route() {
        let body = initial_token_body();
        let value = json!({"kind":"CountObjects","args":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},"kind":"Permanent","filter":{"kind":"CardType","args":"Creature"}}}});
        assert!(!shared(&body)); // Ordinary @where still substitutes inline.
        let inline = crate::functions::substituted(body.clone(), &value);
        assert_eq!(inline["instructions"].as_array().unwrap().len(), 1);
        let captured = snapshot(&body, &value, 10).unwrap();
        assert_eq!(captured["instructions"].as_array().unwrap().len(), 2);
        assert_eq!(
            captured["instructions"][0]["args"]["operation"]["args"]["value"],
            value
        );
    }

    #[test]
    fn token_rules_amounts_and_independent_ability_x_do_not_opt_in() {
        for field in ["rules", "amount", "partial", "ability", "players"] {
            let mut body = initial_token_body();
            let args = &mut body["instructions"][0]["args"]["operation"]["args"];
            match field {
                "rules" => args["tokens"][0]["words"] = json!("Rules"),
                "amount" => args["tokens"][0]["amount"]["args"] = json!(2),
                "partial" => {
                    args["tokens"][0]["recipe"]["Defined"]["power_toughness"][0] =
                        json!({"kind":"Constant","args":1})
                }
                "ability" => {
                    args["tokens"][0]["recipe"]["Defined"]["abilities"] =
                        json!([{"kind":"AnnouncedX"}])
                }
                _ => args["players"] = json!({"kind":"All"}),
            }
            assert!(
                snapshot(&body, &json!({"kind":"Constant","args":3}), 10).is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn paid_property_candidate_is_deferred_and_not_general_shared_definition() {
        let mut body = draw(json!({"kind":"AnnouncedX"}));
        let mut mill = body["instructions"][0].clone();
        mill["args"]["operation"]["kind"] = json!("Mill");
        body["instructions"].as_array_mut().unwrap().push(mill);
        let definition = json!({"kind":"ObjectProperty","args":{"object":{"$sym":"named_object","args":[],"at":30},"property":"Power","information":"CurrentOrLastKnown"}});
        assert!(!shared_definition(&definition));
        let captured = snapshot(&body, &definition, 10).unwrap();
        assert_eq!(
            captured["instructions"][0]["args"]["operation"]["args"]["value"]["$sym"],
            "paid_sacrifice_definition"
        );
        let explicit_source = json!({"kind":"ObjectProperty","args":{"object":{"kind":"Source"},"property":"Power","information":"CurrentOrLastKnown"}});
        assert!(snapshot(&body, &explicit_source, 10).is_err());
    }

    fn paired_pump_body() -> Value {
        let mut first = pump()["instructions"][0].clone();
        first["args"]["operation"]["args"]["changes"][0]["args"]["toughness"] =
            json!({"kind":"Constant","args":0});
        let mut second = first.clone();
        second["args"]["result"] = json!(1);
        second["args"]["operation"]["args"]["changes"][0]["args"] = json!({"power":{"kind":"Constant","args":0},"toughness":{"kind":"Multiply","args":[{"kind":"Constant","args":-1},{"kind":"AnnouncedX"}]}});
        json!({"instructions":[first,second]})
    }
    fn snapshot_inverse(body: &Value, definition: &Value) -> usize {
        let mut captured =
            super::super::substituted(body.clone(), &json!({"kind":"BoundValue","args":7}));
        captured["instructions"].as_array_mut().unwrap().insert(0, json!({"kind":"Effect","args":{"result":7,"operation":{"kind":"BindValue","args":{"value":definition}}}}));
        let holes = [
            Pattern::Hole("program".into(), crate::model::ir_schema::Ty::Opaque),
            Pattern::Hole("value".into(), crate::model::ir_schema::Ty::Opaque),
        ];
        unify(&holes, &captured, Bindings::new()).len()
    }
    #[test]
    fn where_snapshots103_scry_is_one_immediate_use_without_source_holder_alias() {
        let mut body = draw(json!({"kind":"AnnouncedX"}));
        body["instructions"][0]["args"]["operation"]["kind"] = json!("Scry");
        for kind in ["Source", "GrantingObject"] {
            let amount = json!({"kind":"ObjectProperty","args":{"object":{"kind":kind},"property":"Toughness","information":"CurrentOrLastKnown"}});
            let captured = snapshot(&body, &amount, 4).unwrap();
            assert_eq!(
                captured["instructions"][0]["args"]["operation"]["args"]["value"],
                amount
            );
            assert_eq!(snapshot_inverse(&body, &amount), 1);
        }
        let amount = json!({"kind":"Constant","args":3});
        for players in [
            json!({"kind":"All"}),
            json!({"kind":"One","args":{"kind":"Bound","args":1}}),
        ] {
            let mut foreign = body.clone();
            foreign["instructions"][0]["args"]["operation"]["args"]["players"] = players;
            assert!(snapshot(&foreign, &amount, 4).is_err());
            assert_eq!(snapshot_inverse(&foreign, &amount), 0);
        }

        for wrapper in [
            "Optional",
            "If",
            "ForEachPlayer",
            "GrantAbility",
            "CreateDelayedTrigger",
        ] {
            let nested = json!({"instructions":[{"kind":wrapper,"args":{"body":body}}]});
            assert!(snapshot(&nested, &amount, 4).is_err());
            assert_eq!(snapshot_inverse(&nested, &amount), 0);
        }
        body["instructions"]
            .as_array_mut()
            .unwrap()
            .insert(0, draw(amount.clone())["instructions"][0].clone());
        assert!(snapshot(&body, &amount, 4).is_err());
        assert_eq!(snapshot_inverse(&body, &amount), 0);
    }
    #[test]
    fn where_snapshots103_paired_pumps_add_capture_without_eager_inline_retirement() {
        let body = paired_pump_body();
        let definition = json!({"kind":"CountObjects","args":{"kind":"Selected","args":8}});
        assert!(!shared(&body));
        assert!(paired_pumps(&body));
        let inline = super::super::substituted(body.clone(), &definition);
        assert_eq!(inline["instructions"].as_array().unwrap().len(), 2);
        let captured = snapshot(&body, &definition, 5).unwrap();
        assert_eq!(captured["instructions"].as_array().unwrap().len(), 3);
        assert_eq!(snapshot_inverse(&body, &definition), 1);
        for kind in ["Source", "GrantingObject"] {
            let property = json!({"kind":"ObjectProperty","args":{"object":{"kind":kind},"property":"Power","information":"CurrentOrLastKnown"}});
            assert!(snapshot(&body, &property, 5).is_err());
            assert_eq!(snapshot_inverse(&body, &property), 0);
        }
    }
    #[test]
    fn where_snapshots103_pair_shapes_and_definitions_have_identical_inverse_exclusions() {
        let definition = json!({"kind":"Constant","args":3});
        for change in 0..11 {
            let mut body = paired_pump_body();
            let op = &mut body["instructions"][1]["args"]["operation"];
            match change {
                0 => op["args"]["duration"] = json!("Indefinite"),
                1 => op["args"]["evaluation"] = json!("LiveObjects"),
                2 => op["args"]["changes"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"kind":"GrantKeyword","args":{"kind":"Haste"}})),
                3 => op["args"]["source"] = json!({"kind":"AnnouncedX"}),
                4 => op["args"]["changes"][0]["args"]["power"] = json!({"kind":"AnnouncedX"}),
                5 => {
                    op["args"]["changes"][0]["args"]["toughness"] = json!({"kind":"Multiply","args":[{"kind":"Constant","args":-2},{"kind":"AnnouncedX"}]})
                }
                8 => op["kind"] = json!("Draw"),
                9 | 10 => {
                    let mut spec =
                        json!({"count":{"Exactly":{"kind":"Constant","args":1}},"distribute":null});
                    if change == 9 {
                        spec["count"]["Exactly"] = json!({"kind":"AnnouncedX"});
                    } else {
                        spec["distribute"] = json!({"kind":"AnnouncedX"});
                    }
                    op["args"]["affected"] = json!({"Objects":{"kind":"One","args":{"kind":"Target","args":{"slot":{"$sym":"target","args":[spec],"at":3},"index":0}}}});
                }

                6 => body["instructions"]
                    .as_array_mut()
                    .unwrap()
                    .push(draw(definition.clone())["instructions"][0].clone()),
                _ => body["instructions"]
                    .as_array_mut()
                    .unwrap()
                    .insert(0, draw(definition.clone())["instructions"][0].clone()),
            }
            assert!(snapshot(&body, &definition, 2).is_err(), "case {change}");
            assert_eq!(snapshot_inverse(&body, &definition), 0, "case {change}");
        }
    }
}
