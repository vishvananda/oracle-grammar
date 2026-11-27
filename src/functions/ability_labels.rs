//! Keep optional presentation at its written position in an ability.
use crate::model::ir_schema::{Schema, Ty};
use crate::pattern::{Bindings, Pattern, Syntax, UnifyCtx, elaborate};
use serde_json::{Value, json};

pub(crate) fn is_symbol(name: &str) -> bool {
    matches!(
        name,
        "optional_ability_label"
            | "optional_ability_reminder"
            | "optional_cost_reminder"
            | "unlabelled_instructions"
    )
}

pub(crate) fn checked_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Pattern, String> {
    let ability = Ty::Named("Ability".into());
    let arguments = match (name, args) {
        (
            "optional_ability_label" | "optional_ability_reminder" | "optional_cost_reminder",
            [inner, decoration],
        ) if *ty == ability => {
            vec![
                elaborate(inner, &ability, schema)?,
                elaborate(
                    decoration,
                    &Ty::Option(Box::new(Ty::Named(
                        match name {
                            "optional_ability_label" => "String",
                            "optional_cost_reminder" => "Cost",
                            _ => "bool",
                        }
                        .into(),
                    ))),
                    schema,
                )?,
            ]
        }
        ("unlabelled_instructions", [spell])
            if *ty == Ty::Vec(Box::new(Ty::Named("Instruction".into()))) =>
        {
            vec![elaborate(spell, &ability, schema)?]
        }
        _ => {
            return Err(format!(
                "@{name} needs an ability and the matching result type"
            ));
        }
    };
    Ok(Pattern::Symbol(name.into(), arguments))
}

pub(crate) fn fill(name: &str, args: &[Value]) -> Result<Value, String> {
    match (name, args) {
        (
            "optional_ability_label" | "optional_ability_reminder" | "optional_cost_reminder",
            [ability, decoration],
        ) if ability["kind"] != "WithPresentation" => {
            optional_presentation(name, ability, decoration)
        }
        ("unlabelled_instructions", [spell]) => ordinary(spell)
            .cloned()
            .ok_or_else(|| "a labelled paragraph needs the complete spell".into()),
        _ => Err("ability label needs one underlying ability".into()),
    }
}

fn optional_presentation(name: &str, ability: &Value, decoration: &Value) -> Result<Value, String> {
    if decoration.is_null() {
        return Ok(ability.clone());
    }
    let (label, reminder) = match name {
        "optional_ability_label" if decoration.is_string() => (decoration, false),
        "optional_ability_reminder" if decoration == true => (&Value::Null, true),
        "optional_cost_reminder"
            if ability["kind"] == "CostBranch" && ability["args"]["cost"] == *decoration =>
        {
            (&Value::Null, true)
        }
        _ => return Err("invalid optional ability presentation".into()),
    };
    Ok(json!({"kind":"WithPresentation","args":{
        "label":label, "reminder":reminder, "ability":ability
    }}))
}

fn ordinary(spell: &Value) -> Option<&Value> {
    let args = &spell["args"];
    let instructions = &args["body"]["instructions"];
    (spell["kind"] == "Spell" && args["announcement"]["modes"].is_null() && instructions.is_array())
        .then_some(instructions)
}

pub(crate) fn unify<'p, 'v>(
    name: &str,
    args: &'p [Pattern],
    value: &'v Value,
    bindings: Bindings<'p, 'v>,
    ctx: Option<UnifyCtx<'p>>,
) -> Vec<Bindings<'p, 'v>> {
    match (name, args) {
        (
            "optional_ability_label" | "optional_ability_reminder" | "optional_cost_reminder",
            [ability, decoration],
        ) => {
            let Some((inner, presented)) = underlying(name, value) else {
                return Vec::new();
            };
            decoration
                .unify_with(presented, bindings, ctx)
                .into_iter()
                .flat_map(|bound| ability.unify_with(inner, bound, ctx))
                .collect()
        }
        ("unlabelled_instructions", [spell]) if value.is_array() => {
            let unlabelled = json!({"kind":"Spell","args":{
                "announcement":{"$sym":"announcement","args":[]},
                "body":{"instructions":value}
            }});
            spell.unify_made(&unlabelled, &bindings)
        }
        _ => Vec::new(),
    }
}

fn underlying<'a>(name: &str, value: &'a Value) -> Option<(&'a Value, &'a Value)> {
    if value["kind"] != "WithPresentation" {
        return Some((value, &Value::Null));
    }
    let fields = &value["args"];
    let ability = &fields["ability"];
    if ability["kind"] == "WithPresentation" {
        return None;
    }
    let label = &fields["label"];
    let reminder = &fields["reminder"];
    match name {
        "optional_ability_label" if reminder == false && label.is_string() => {
            Some((ability, label))
        }
        "optional_ability_reminder" if reminder == true && label.is_null() => {
            Some((ability, reminder))
        }
        "optional_cost_reminder"
            if reminder == true && label.is_null() && ability["kind"] == "CostBranch" =>
        {
            Some((ability, &ability["args"]["cost"]))
        }
        _ => None,
    }
}
