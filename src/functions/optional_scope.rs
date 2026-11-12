//! Preserve one declared optional action across an explicit conjunction.
use crate::pattern::{Bindings, Pattern};
use serde_json::{Value, json};

fn controller() -> Value {
    json!({"kind":"One","args":{"kind":"AbilityController"}})
}

// The simple sequence guards inspect only fixed-depth direct operands. The existing semantic
// parser/native input bounds still own nested bytes/depth; these list operations
// add no recursive walk and use the existing semantic-node ceiling.
fn instructions(value: &Value) -> Result<&[Value], String> {
    let items = value.as_array().ok_or("expected instruction list")?;
    if items.len() > 16_384 {
        return Err("instruction-list semantic work bound".into());
    }
    Ok(items)
}

fn hand_discard(instruction: &Value) -> bool {
    if instruction["kind"] != "Effect" {
        return false;
    }
    let operation = &instruction["args"]["operation"];
    operation["kind"] == "Discard"
        && operation["args"]["players"] == controller()
        && operation["args"]["random"] == false
        && operation["args"]["amount"]
            == json!({"kind":"CountObjects","args":{"kind":"Zone","args":{
                "scope":{"Players":{"zone":"Hand","players":controller()}},
                "kind":"Card","filter":{"kind":"Any"}
            }}})
}

fn controller_draw(instruction: &Value) -> bool {
    instruction["kind"] == "Effect"
        && instruction["args"]["operation"]["kind"] == "Draw"
        && instruction["args"]["operation"]["args"]["players"] == controller()
}

fn compound(mut actions: &[Value]) -> bool {
    while let [only] = actions {
        if only["kind"] != "Group" {
            break;
        }
        let Some(inner) = only["args"]["instructions"].as_array() else {
            return false;
        };
        actions = inner;
    }
    matches!(actions, [discard, draw] if hand_discard(discard) && controller_draw(draw))
}

/// `You may exile it and collect evidence 4. If you do, ...`: the check reads the
/// whole option, so `and` cannot end the option before a payment it checks.
fn split_optional(head: &[Value], tail: &[Value]) -> bool {
    let [head] = head else { return false };
    let optional = head["kind"] == "Optional"
        && head["args"]["decider"] == json!({"kind":"AbilityController"});
    let discard = head["args"]["body"]["instructions"]
        .as_array()
        .is_some_and(|body| matches!(body.as_slice(), [discard] if hand_discard(discard)));
    optional
        && (discard && tail.first().is_some_and(controller_draw)
            || checked_payment(&tail[..tail.len().min(2)]))
}

/// `Then if it has no counters on it, sacrifice it and you gain 4 life`: a condition
/// governs to the end of its sentence, so `and` never ends it.
fn forbidden_join(head: &[Value], tail: &[Value]) -> bool {
    split_optional(head, tail) || head.last().is_some_and(|last| last["kind"] == "If")
}

/// Only this EBNF-declared `and` join applies this policy. Period/then joins do
/// not call it, and nested abilities/branches are never traversed for a match.
pub(crate) fn sequence(head: &Value, tail: &Value) -> Result<Value, String> {
    let head = instructions(head)?;
    let tail = instructions(tail)?;
    if head
        .len()
        .checked_add(tail.len())
        .is_none_or(|length| length > 16_384)
    {
        return Err("instruction-list semantic work bound".into());
    }
    if forbidden_join(head, tail) {
        return Err("and does not end an optional action or a condition early".into());
    }
    Ok(Value::Array(head.iter().chain(tail).cloned().collect()))
}

// These joins also participate in inverse partition search. Keep both
// directions within the same finite action and serialized-byte budget.
const THEN_ACTIONS: usize = 64;
const THEN_BYTES: usize = 256 * 1024;

fn then_parts(value: &Value) -> Result<&[Value], String> {
    let items = instructions(value)?;
    if items.is_empty()
        || items.len() > THEN_ACTIONS
        || serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > THEN_BYTES
    {
        return Err("then composition work bound".into());
    }
    Ok(items)
}

fn then_allowed(head: &[Value], tail: &[Value], inside: bool) -> bool {
    let new_choice = tail.first().is_some_and(|i| i["kind"] == "Optional");
    // `If you do, draw a card, then you may search`: the condition reaches the full stop.
    if head.last().is_some_and(|i| i["kind"] == "If") {
        return false;
    }
    if inside {
        !new_choice
    } else {
        head.last().is_none_or(|i| i["kind"] != "Optional") || new_choice
    }
}

pub(crate) fn then_sequence(head: &Value, tail: &Value, inside: bool) -> Result<Value, String> {
    let left = then_parts(head)?;
    let right = then_parts(tail)?;
    if left.len() + right.len() > THEN_ACTIONS || !then_allowed(left, right, inside) {
        return Err("then keeps one optional action but starts an explicit new choice".into());
    }
    let joined = Value::Array(left.iter().chain(right).cloned().collect());
    then_parts(&joined)?;
    Ok(joined)
}

fn unify_then<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
    inside: bool,
) -> Vec<Bindings<'p, 'v>> {
    let [head, tail] = args else {
        return Vec::new();
    };
    let Ok(items) = then_parts(value) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for split in 1..items.len() {
        if !then_allowed(&items[..split], &items[split..], inside) {
            continue;
        }
        let left = Value::Array(items[..split].to_vec());
        let right = Value::Array(items[split..].to_vec());
        for bound in head.unify_made(&left, &bindings) {
            result.extend(tail.unify_made(&right, &bound));
        }
    }
    result
}

/// A choice of one card from your hand put onto the battlefield.
fn hand_entry(items: &[Value]) -> bool {
    let [choose, put] = items else { return false };
    let chosen = &choose["args"]["operation"];
    let moved = &put["args"]["operation"];
    chosen["kind"] == "ChooseObjects"
        && chosen["args"]["objects"]["args"]["scope"]
            == json!({"Players":{"zone":"Hand","players":controller()}})
        && moved["kind"] == "Move"
        && moved["args"]["destination"].get("Battlefield").is_some()
}

/// `You may put a creature card from your hand onto the battlefield. If you do, ...`
/// pays with the card itself (`payment_put_hand_card`), so an offered action never
/// performs that put as a competing reading.
pub(crate) fn offered_action(actions: &Value) -> Result<Value, String> {
    if hand_entry(instructions(actions)?) {
        return Err("a put from the hand pays with the card itself".into());
    }
    Ok(actions.clone())
}

/// This compound producer has its own declared completion receipt. It cannot
/// use the generic single-action backwards reference as a competing reading.
pub(crate) fn single_base(actions: &Value) -> Result<Value, String> {
    if compound(instructions(actions)?) {
        return Err("compound discard-hand/draw uses its declared completion receipt".into());
    }
    Ok(actions.clone())
}

/// A second explicit choice is outside the preceding `you may have` offer.
/// Groups are transparent; choices inside conditions or future abilities keep
/// their own scope. This guard never relocates instructions or changes operands.
pub(crate) fn program(value: &Value) -> Result<Value, String> {
    checked_program(value, Offer::SingleChoice)
}

/// Generic `you may` recipes cannot turn an explicit sacrifice payment into
/// an optional program around a required payment. Its owning payment clause
/// already sets optionality and supplies the dependent success condition.
/// Conditions and future abilities retain their own independent scopes.
pub(crate) fn action_program(value: &Value) -> Result<Value, String> {
    checked_program(value, Offer::Action)
}

#[derive(Clone, Copy)]
enum Offer {
    SingleChoice,
    Action,
}

fn required_sacrifice(item: &Value) -> bool {
    let operation = &item["args"]["operation"];
    item["kind"] == "Effect"
        && operation["kind"] == "PayCost"
        && operation["args"]["optional"] == false
        && operation["args"]["cost"]["parts"]
            .as_array()
            .is_some_and(|parts| parts.iter().any(|part| part.get("Sacrifice").is_some()))
}

/// `Pay {1}. If you do, ...` checked by its own success: under a generic `you may`
/// it is the optional payment its `you may pay` clause already reads.
fn checked_payment(items: &[Value]) -> bool {
    items.windows(2).any(|pair| {
        let operation = &pair[0]["args"]["operation"];
        let condition = &pair[1]["args"]["condition"];
        pair[0]["kind"] == "Effect"
            && operation["kind"] == "PayCost"
            && operation["args"]["optional"] == false
            && pair[1]["kind"] == "If"
            && condition["kind"] == "PaymentChosen"
            && (condition["args"] == pair[0]["args"]["result"]
                // While parsing, the check still names the payment as the last result.
                || condition["args"]["$sym"] == "last_result")
    })
}

fn checked_program(value: &Value, offer: Offer) -> Result<Value, String> {
    crate::functions::branch_bounds(value)?;
    let items = value["instructions"]
        .as_array()
        .ok_or("optional program needs instructions")?;
    if matches!(offer, Offer::Action) && checked_payment(items) {
        return Err("an optional payment uses its declared payment clause".into());
    }
    let mut pending: Vec<_> = items.iter().collect();
    let mut work = 0usize;
    while let Some(item) = pending.pop() {
        work += 1;
        if work > 16_384 {
            return Err("optional program instruction limit".into());
        }
        match offer {
            Offer::SingleChoice if item["kind"] == "Optional" => {
                return Err(
                    "an explicit new choice is outside the preceding optional offer".into(),
                );
            }
            Offer::Action if required_sacrifice(item) => {
                return Err("an optional sacrifice uses its declared payment clause".into());
            }
            _ => {}
        }
        if item["kind"] == "Group" {
            let group = item["args"]["instructions"]
                .as_array()
                .ok_or("optional program group needs instructions")?;
            if matches!(offer, Offer::Action) && checked_payment(group) {
                return Err("an optional payment uses its declared payment clause".into());
            }
            pending.extend(group);
        }
    }
    Ok(value.clone())
}

pub(crate) fn unify<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    match name {
        "and_sequence" => unify_sequence(args, value, bindings),
        "then_sequence" => unify_then(args, value, bindings, false),
        "optional_then" => unify_then(args, value, bindings, true),
        "single_reflexive_base" => unify_base(args, value, bindings),
        "offered_action" => match args {
            [inner] if offered_action(value).is_ok() => inner.unify(value, bindings),
            _ => Vec::new(),
        },
        "optional_action_program" => match args {
            [inner] if action_program(value).is_ok() => inner.unify(value, bindings),
            _ => Vec::new(),
        },
        "optional_program" => match args {
            [inner] if program(value).is_ok() => inner.unify(value, bindings),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

pub(crate) fn unify_base<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [inner] = args else { return Vec::new() };
    if single_base(value).is_err() {
        return Vec::new();
    }
    inner.unify(value, bindings)
}

pub(crate) fn unify_sequence<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let [head, tail] = args else {
        return Vec::new();
    };
    let Ok(items) = instructions(value) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    // Same contiguous partitions as the former pair of instruction splices.
    for split in 0..=items.len() {
        if forbidden_join(&items[..split], &items[split..]) {
            continue;
        }
        let left = Value::Array(items[..split].to_vec());
        let right = Value::Array(items[split..].to_vec());
        for bound in head.unify_made(&left, &bindings) {
            result.extend(tail.unify_made(&right, &bound));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ir_schema::Ty;

    #[test]
    fn generic_optional_actions_do_not_absorb_required_sacrifice_payments() {
        let payment = json!({"kind":"Effect","args":{"operation":{
            "kind":"PayCost","args":{"optional":false,"cost":{"parts":[{"Sacrifice":{}}]}}
        }}});
        let direct = json!({"instructions":[payment.clone()]});
        assert!(action_program(&direct).is_err());
        assert!(program(&direct).is_ok()); // The earlier policy is unchanged.
        let grouped = json!({"instructions":[{"kind":"Group","args":direct}]});
        assert!(action_program(&grouped).is_err());
        let nested = json!({"instructions":[{"kind":"If","args":{
            "then":{"instructions":[payment]},"otherwise":{"instructions":[]}
        }}]});
        assert_eq!(action_program(&nested).unwrap(), nested);
        let independent = json!({"instructions":[{"kind":"Optional"}]});
        assert_eq!(action_program(&independent).unwrap(), independent);
        let args = [Pattern::Hole("body".into(), Ty::Named("Program".into()))];
        assert!(unify("optional_action_program", &args, &grouped, Bindings::new()).is_empty());
        assert_eq!(
            unify("optional_action_program", &args, &nested, Bindings::new()).len(),
            1
        );
    }

    #[test]
    fn an_offered_action_never_performs_a_put_from_the_hand() {
        let choose = |zone: &str| {
            json!({"kind":"Effect","args":{"operation":{"kind":"ChooseObjects","args":{
                "objects":{"kind":"Zone","args":{"scope":{"Players":{"zone":zone,"players":controller()}}}}
            }}}})
        };
        let put = json!({"kind":"Effect","args":{"operation":{"kind":"Move","args":{
            "destination":{"Battlefield":{}}
        }}}});
        let hand = json!([choose("Hand"), put.clone()]);
        assert!(offered_action(&hand).is_err());
        let args = [Pattern::Hole(
            "body".into(),
            Ty::Vec(Box::new(Ty::Named("Instruction".into()))),
        )];
        assert!(unify("offered_action", &args, &hand, Bindings::new()).is_empty());
        let graveyard = json!([choose("Graveyard"), put]);
        assert_eq!(offered_action(&graveyard).unwrap(), graveyard);
        assert_eq!(
            unify("offered_action", &args, &graveyard, Bindings::new()).len(),
            1
        );
    }

    #[test]
    fn generic_optional_actions_do_not_absorb_a_payment_checked_by_its_success() {
        let pay = |optional: bool| {
            json!({"kind":"Effect","args":{"result":0,"operation":{
                "kind":"PayCost","args":{"optional":optional,"cost":{"parts":[{"Mana":[]}]}}
            }}})
        };
        let check = json!({"kind":"If","args":{"condition":{"kind":"PaymentChosen","args":0}}});
        let reflexive =
            json!({"kind":"Effect","args":{"result":1,"operation":{"kind":"ReflexiveTrigger"}}});
        let checked = json!({"instructions":[pay(false), check.clone()]});
        assert!(action_program(&checked).is_err());
        assert!(program(&checked).is_ok());
        let grouped = json!({"instructions":[{"kind":"Group","args":checked}]});
        assert!(action_program(&grouped).is_err());
        // `You may pay {1}. When you do, ...` keeps its required payment.
        let when = json!({"instructions":[pay(false), reflexive]});
        assert!(action_program(&when).is_ok());
        let offered = json!({"instructions":[pay(true), check]});
        assert!(action_program(&offered).is_ok());
    }

    #[test]
    fn optional_offer_excludes_a_new_choice_but_keeps_conditional_choices() {
        let extra = json!({"kind":"Optional","args":{"body":{"instructions":[]}}});
        let bad = json!({"instructions":[{"kind":"Effect"},extra.clone()]});
        assert!(program(&bad).is_err());
        let grouped = json!({"instructions":[{"kind":"Group","args":bad}]});
        assert!(program(&grouped).is_err());
        let allowed = json!({"instructions":[{"kind":"If","args":{
            "then":{"instructions":[extra]},"otherwise":{"instructions":[]}
        }}]});
        assert_eq!(program(&allowed).unwrap(), allowed);
        let args = [Pattern::Hole("body".into(), Ty::Named("Program".into()))];
        assert!(unify("optional_program", &args, &grouped, Bindings::new()).is_empty());
        assert_eq!(
            unify("optional_program", &args, &allowed, Bindings::new()).len(),
            1
        );
    }

    #[test]
    fn phrase_gaps_compose_complete_native_cards_without_the_absorbed_choice() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../../tests/fixtures/shared-clauses-21.json"))
                .unwrap();
        let engine = crate::test_support::engine();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let attempt = engine.inspect_with_candidate_ir(name, case["text"].as_str().unwrap());
            assert!(attempt.result.is_ok(), "{name}: {:?}", attempt.evidence);
            let actual = attempt.result.unwrap();
            assert!(
                actual
                    .readings
                    .contains(&crate::engine::canonical_abilities(&case["expected"]).unwrap()),
                "{name}"
            );
            eprintln!("{name}: {} complete readings", actual.readings.len());
        }
    }

    #[test]
    fn then_continues_an_option_but_another_option_starts_a_new_choice() {
        let plain = json!([{"kind":"Effect"}]);
        let option = json!([{"kind":"Optional"}]);
        assert!(then_sequence(&option, &plain, false).is_err());
        assert!(then_sequence(&option, &option, false).is_ok());
        assert!(then_sequence(&plain, &option, true).is_err());
        assert!(then_sequence(&plain, &plain, true).is_ok());
        let too_many = Value::Array(vec![json!({"kind":"Effect"}); THEN_ACTIONS + 1]);
        assert!(then_sequence(&too_many, &plain, true).is_err());
    }

    #[test]
    fn then_forward_and_inverse_obey_the_same_partition_guards() {
        let args = [
            Pattern::Hole("head".into(), Ty::Opaque),
            Pattern::Hole("tail".into(), Ty::Opaque),
        ];
        let plain = json!([{"kind":"Effect"}]);
        let option = json!([{"kind":"Optional"}]);
        for (head, tail, inside, permitted) in [
            (&plain, &plain, false, true),
            (&plain, &plain, true, true),
            (&option, &plain, false, false),
            (&option, &option, false, true),
            (&plain, &option, true, false),
        ] {
            let value = json!([head[0].clone(), tail[0].clone()]);
            let inverse = unify_then(&args, &value, Bindings::new(), inside);
            assert_eq!(then_sequence(head, tail, inside).is_ok(), permitted);
            assert_eq!(!inverse.is_empty(), permitted);
        }
        assert!(then_sequence(&json!([]), &plain, false).is_err());
        assert!(unify_then(&args, &plain, Bindings::new(), false).is_empty());
    }
    fn discard() -> Value {
        json!({"kind":"Effect","args":{"result":{"$sym":"next","args":[],"at":0},"operation":{"kind":"Discard","args":{
            "players":controller(),"random":false,"amount":{"kind":"CountObjects","args":{"kind":"Zone","args":{
                "scope":{"Players":{"zone":"Hand","players":controller()}},"kind":"Card","filter":{"kind":"Any"}
            }}}
        }}}})
    }
    fn draw() -> Value {
        json!({"kind":"Effect","args":{"result":1,"operation":{"kind":"Draw","args":{"players":controller(),"amount":{"kind":"Constant","args":2}}}}})
    }
    fn option() -> Value {
        json!({"kind":"Optional","args":{"decider":{"kind":"AbilityController"},"body":{"instructions":[discard()]}}})
    }
    #[test]
    fn only_declared_same_player_hand_draw_split_is_rejected() {
        assert!(sequence(&json!([option()]), &json!([draw()])).is_err());
        assert!(sequence(&json!([discard()]), &json!([draw()])).is_ok());
        let mut other = draw();
        other["args"]["operation"]["args"]["players"] =
            json!({"kind":"OpponentsOf","args":{"kind":"AbilityController"}});
        assert!(sequence(&json!([option()]), &json!([other])).is_ok());
        let mut one = option();
        one["args"]["body"]["instructions"][0]["args"]["operation"]["args"]["amount"] =
            json!({"kind":"Constant","args":1});
        assert!(sequence(&json!([one]), &json!([draw()])).is_ok());
        let scope = json!({"$sym":"scope","args":[option()]});
        assert!(sequence(&json!([scope]), &json!([draw()])).is_ok());
    }
    #[test]
    fn compound_base_guard_has_the_same_forward_and_inverse_contract() {
        let actions = json!([discard(), draw()]);
        let hole = [Pattern::Hole("body".into(), Ty::Opaque)];
        assert!(single_base(&actions).is_err());
        assert!(unify_base(&hole, &actions, Bindings::new()).is_empty());
        for allowed in [
            json!([discard()]),
            json!([draw()]),
            json!([draw(), discard()]),
            json!([{"$sym":"holder","args":[actions.clone()]}]),
        ] {
            assert_eq!(single_base(&allowed).unwrap(), allowed);
            assert_eq!(unify_base(&hole, &allowed, Bindings::new()).len(), 1);
        }
    }
    #[test]
    fn inverse_partitions_never_reintroduce_the_forbidden_join() {
        let value = json!([option(), draw()]);
        let args = [
            Pattern::Hole("head".into(), Ty::Opaque),
            Pattern::Hole("tail".into(), Ty::Opaque),
        ];
        let bindings = unify_sequence(&args, &value, Bindings::new());
        assert_eq!(bindings.len(), 2);
        for binding in bindings {
            assert_eq!(
                sequence(binding.get("head").unwrap(), binding.get("tail").unwrap()).unwrap(),
                value
            );
        }
    }
}
