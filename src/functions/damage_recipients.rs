//! A coordinated list can supply the antecedent for "they control".
use crate::model::ir_schema::{Schema, Ty};
use crate::pattern::{Bindings, Pattern, Syntax, elaborate};
use serde_json::{Value, json};

pub(crate) fn checked_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    if args.len() != 2 || *ty != Ty::Named("Recipients".into()) {
        return Err("@recipient_pair combines two Recipients values".into());
    }
    Ok(Pattern::Symbol(
        "recipient_pair".into(),
        args.iter()
            .map(|part| elaborate(part, ty, schema))
            .collect::<Result<_, _>>()?,
    ))
}

pub(crate) fn pair(left: &Value, right: &Value) -> Value {
    let mut second = right.clone();
    if let Some(players) = controlling_opponents(left) {
        replace_controller(&mut second, is_unbound_controller, players);
    }
    json!({"kind":"Union","args":[left,second]})
}

fn controlling_opponents(recipient: &Value) -> Option<&Value> {
    let players = &recipient["args"];
    (recipient["kind"] == "Players" && players["kind"] == "OpponentsOf").then_some(players)
}

fn is_unbound_controller(players: &Value) -> bool {
    players["kind"] == "One" && players["args"]["$sym"] == "that_player"
}

fn replace_controller(right: &mut Value, matches: impl Fn(&Value) -> bool, with: &Value) -> bool {
    if right["kind"] != "Objects" {
        return false;
    }
    let objects = &mut right["args"];
    let zone = if objects["$sym"] == "distributive_pump_subject" {
        &mut objects["args"][0]
    } else {
        objects
    };
    if zone["kind"] != "Zone" {
        return false;
    }
    let Some(filter) = zone.pointer_mut("/args/filter") else {
        return false;
    };
    let update = |term: &mut Value| {
        if term["kind"] == "ControlledBy" && matches(&term["args"]) {
            term["args"] = with.clone();
            true
        } else {
            false
        }
    };
    if filter["kind"] != "AllOf" {
        return update(filter);
    }
    let Some(terms) = filter["args"]
        .as_array_mut()
        .filter(|terms| terms.len() <= 32)
    else {
        return false;
    };
    let mut changed = false;
    for term in terms {
        changed |= update(term);
    }
    changed
}

pub(crate) fn unify<'p, 'v>(
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
) -> Vec<Bindings<'p, 'v>> {
    let ([left, right], Some([first, second])) =
        (args, value["args"].as_array().map(Vec::as_slice))
    else {
        return Vec::new();
    };
    if value["kind"] != "Union" {
        return Vec::new();
    }
    let mut result = Vec::new();
    for bound in left.unify(first, bindings) {
        result.extend(right.unify(second, bound.clone()));
        if let Some(players) = controlling_opponents(first) {
            let mut pronoun = second.clone();
            let reference = json!({"kind":"One","args":{"$sym":"that_player","args":[]}});
            if replace_controller(&mut pronoun, |owner| owner == players, &reference) {
                result.extend(right.unify_made(&pronoun, &bound));
            }
        }
    }
    result
}
