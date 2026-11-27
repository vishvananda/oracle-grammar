//! Keep a label at its written position without changing the underlying ability.
use crate::model::ir_schema::{Schema, Ty};
use crate::pattern::{Bindings, Pattern, Syntax, UnifyCtx, elaborate};
use serde_json::{Value, json};

pub(crate) fn is_symbol(name: &str) -> bool {
    matches!(name, "optional_ability_label" | "unlabelled_instructions")
}

pub(crate) fn checked_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Pattern, String> {
    let ability = Ty::Named("Ability".into());
    let arguments = match (name, args) {
        ("optional_ability_label", [inner, label]) if *ty == ability => vec![
            elaborate(inner, &ability, schema)?,
            elaborate(
                label,
                &Ty::Option(Box::new(Ty::Named("String".into()))),
                schema,
            )?,
        ],
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
        ("optional_ability_label", [ability, label]) if ability["kind"] != "WithPresentation" => {
            if label.is_null() {
                Ok(ability.clone())
            } else if label.is_string() {
                Ok(json!({"kind":"WithPresentation","args":{
                    "label":label, "reminder":false, "ability":ability
                }}))
            } else {
                Err("spell paragraph label must be a string".into())
            }
        }
        ("unlabelled_instructions", [spell]) => ordinary(spell)
            .cloned()
            .ok_or_else(|| "a labelled paragraph needs the complete spell".into()),
        _ => Err("ability label needs one underlying ability".into()),
    }
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
        ("optional_ability_label", [ability, label]) => {
            let (inner, caption) = if value["kind"] == "WithPresentation" {
                let fields = &value["args"];
                if fields["reminder"] != false || !fields["label"].is_string() {
                    return Vec::new();
                }
                (&fields["ability"], &fields["label"])
            } else {
                (value, &Value::Null)
            };
            if inner["kind"] == "WithPresentation" {
                return Vec::new();
            }
            label
                .unify_with(caption, bindings, ctx)
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
