//! Type/arity contracts for semantic operations with specialized arguments.
use crate::model::ir_schema::{Schema, Ty};
use crate::pattern::{Pattern, Syntax, elaborate};

pub(crate) fn checked_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Option<Pattern>, String> {
    if let Some(pattern) = coin_call(name, args, ty, schema)? {
        return Ok(Some(pattern));
    }
    if name == "unordered" {
        return unordered_call(args, ty, schema);
    }
    if name == "damage_relative_object" {
        return damage_relative_call(args, ty).map(Some);
    }
    if let Some(pattern) = specialized_call(name, args, ty, schema)? {
        return Ok(Some(pattern));
    }
    if matches!(name, "reflexive_performed" | "choice_mark") {
        if !args.is_empty() || *ty != Ty::Named("ResultId".into()) {
            return Err(format!("@{name} takes no arguments and produces ResultId"));
        }
        return Ok(Some(Pattern::Symbol(name.into(), Vec::new())));
    }
    if name == "back" && !matches!(args, [Syntax::Int(_)]) {
        return Err("@back needs how far back".into());
    }
    let arity = match name {
        "holder" | "in_filter" | "continuous_parts" | "remember_object" | "zone_members"
        | "remember_operand" | "enchanted_operand" | "resolution" | "leading_if" => Some(1),
        "keyword_abilities" | "keyword_changes" | "announced_x" | "entering_controller" => Some(1),
        "named_source" | "named_object" | "current_it" | "current_named_object" | "next_slot" => {
            Some(0)
        }
        "token_abilities" | "qualified_object" | "group_reference" => Some(2),
        "qualified_members" | "last_exiled" | "last_exiled_creature" => Some(1),
        "either_event" | "agree" | "payment_cost" | "announced_cost" => Some(2),
        "kicker_branch" | "conditional_ability" | "arrival_changes" => Some(2),
        "fixed_mana"
        | "variable_mana"
        | "singular_count"
        | "plural_count"
        | "plural_amount"
        | "inline_counter_price"
        | "captured_counter_price" => Some(1),
        "counter_choice"
        | "owner_return"
        | "sacrificed_result"
        | "measured_sacrifice"
        | "manifest_result"
        | "created_result"
        | "selected_result"
        | "drafted_result"
        | "exiled_card_result"
        | "search_move_result"
        | "seek_move_result"
        | "mana_options"
        | "retain_animation_types"
        | "retain_animation" => Some(1),
        "counter_reference"
        | "counter_reference_single"
        | "created_reference"
        | "created_reference_single"
        | "created_batch_reference"
        | "created_named_reference"
        | "drafted_reference"
        | "drafted_reference_explicit" => Some(0),
        _ => None,
    };
    if arity.is_some_and(|arity| args.len() != arity) {
        return Err(format!("@{name} has the wrong number of arguments"));
    }
    if let Some(pattern) = selection_call(name, args, ty, schema)? {
        return Ok(Some(pattern));
    }
    if name == "drafted_result" {
        return draft_result_call(args, ty, schema).map(Some);
    }
    if matches!(name, "retain_animation_types" | "retain_animation") {
        let expected = if name == "retain_animation_types" {
            Ty::Vec(Box::new(Ty::Named("Change".into())))
        } else {
            Ty::Named("Operation".into())
        };
        if *ty != expected {
            return Err(format!("@{name} produces {expected:?}"));
        }
        return Ok(Some(Pattern::Symbol(
            name.into(),
            vec![elaborate(&args[0], &expected, schema)?],
        )));
    }
    if matches!(
        name,
        "counter_choice"
            | "counter_reference"
            | "counter_reference_single"
            | "created_reference"
            | "created_reference_single"
            | "created_batch_reference"
            | "created_named_reference"
            | "drafted_reference"
            | "drafted_reference_explicit"
    ) {
        let expected = Ty::Named(
            if name == "counter_choice" {
                "Operation"
            } else {
                "ObjectSet"
            }
            .into(),
        );
        if *ty != expected {
            return Err(format!("@{name} produces {expected:?}, not {ty:?}"));
        }
        return Ok(Some(Pattern::Symbol(
            name.into(),
            args.iter()
                .map(|a| elaborate(a, &expected, schema))
                .collect::<Result<_, _>>()?,
        )));
    }
    if reference_marker(name) {
        return reference_call(name, args, ty, schema).map(Some);
    }
    if name == "qualified_members" {
        return qualified_members_call(args, ty, schema).map(Some);
    }
    if matches!(name, "qualified_object" | "group_reference") {
        let output = if name == "qualified_object" {
            "ObjectRef"
        } else {
            "ObjectSet"
        };
        if *ty != Ty::Named(output.into()) {
            return Err(format!("@{name} produces {output}"));
        }
        return Ok(Some(Pattern::Symbol(
            name.into(),
            vec![
                elaborate(&args[0], &Ty::Named("ObjectKind".into()), schema)?,
                elaborate(&args[1], &Ty::Named("ObjectFilter".into()), schema)?,
            ],
        )));
    }
    if name == "arrival_changes" {
        return arrival_call(args, ty, schema).map(Some);
    }
    if name == "mana_options" {
        let expected = Ty::Vec(Box::new(Ty::Named("ManaColor".into())));
        if *ty != expected {
            return Err("@mana_options produces a list of ManaColor".into());
        }
        return Ok(Some(Pattern::Symbol(
            name.into(),
            vec![elaborate(&args[0], &expected, schema)?],
        )));
    }
    if let Some(pattern) = cost_marker(name, args, ty, schema)? {
        return Ok(Some(pattern));
    }
    if matches!(name, "conditional_ability" | "kicker_branch") {
        if *ty != Ty::Named("Ability".into()) {
            return Err(format!("@{name} produces an Ability"));
        }
        let first = if name == "kicker_branch" {
            "Cost"
        } else {
            "Condition"
        };
        return Ok(Some(Pattern::Symbol(
            name.into(),
            vec![
                elaborate(&args[0], &Ty::Named(first.into()), schema)?,
                elaborate(&args[1], ty, schema)?,
            ],
        )));
    }
    if let Some(expected) = crate::functions::keyword_list_type(name) {
        if *ty != expected {
            return Err(format!("@{name} produces {expected:?}, not {ty:?}"));
        }
        let keywords = Ty::Vec(Box::new(Ty::Named("Keyword".into())));
        return Ok(Some(Pattern::Symbol(
            name.to_owned(),
            vec![elaborate(&args[0], &keywords, schema)?],
        )));
    }
    if matches!(name, "resolution" | "leading_if") && *ty != Ty::Named("Instruction".into()) {
        return Err(format!("@{name} marks an Instruction, not {ty:?}"));
    }
    if operation_marker(name) && *ty != Ty::Named("Operation".into()) {
        return Err(format!("@{name} marks an Operation"));
    }
    if name == "remember_operand" && *ty != Ty::Named("ObjectRef".into()) {
        return Err(format!("@{name} marks an ObjectRef, not {ty:?}"));
    }
    if name == "either_event" && *ty != Ty::Named("EventSpec".into()) {
        return Err(format!("@{name} combines EventSpec values, not {ty:?}"));
    }
    if name == "agree" {
        return agreement_call(args, ty, schema).map(Some);
    }
    Ok(None)
}

fn counter_payment_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    let [kind] = args else {
        return Err("@activation_removed_counters needs one CounterKind".into());
    };
    if *ty != Ty::Named("Expr".into()) {
        return Err("@activation_removed_counters produces Expr".into());
    }
    Ok(Pattern::Symbol(
        "activation_removed_counters".into(),
        vec![elaborate(kind, &Ty::Named("CounterKind".into()), schema)?],
    ))
}

fn arrival_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    let operation = Ty::Named("Operation".into());
    if *ty != operation {
        return Err("@arrival_changes produces an Operation".into());
    }
    let changes = Ty::Vec(Box::new(Ty::Named("Change".into())));
    Ok(Pattern::Symbol(
        "arrival_changes".into(),
        vec![
            elaborate(&args[0], &operation, schema)?,
            elaborate(&args[1], &changes, schema)?,
        ],
    ))
}

fn operation_marker(name: &str) -> bool {
    matches!(
        name,
        "manifest_result"
            | "owner_return"
            | "created_result"
            | "selected_result"
            | "sacrificed_result"
            | "measured_sacrifice"
            | "exiled_card_result"
            | "search_move_result"
            | "seek_move_result"
    )
}

fn damage_relative_call(args: &[Syntax], ty: &Ty) -> Result<Pattern, String> {
    if !args.is_empty() || *ty != Ty::Named("ObjectRef".into()) {
        return Err("@damage_relative_object takes no arguments and produces ObjectRef".into());
    }
    Ok(Pattern::Symbol("damage_relative_object".into(), Vec::new()))
}

fn agreement_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    if !matches!(ty, Ty::Named(kind) if matches!(kind.as_str(), "Keyword" | "Cost" | "ObjectFilter" | "Ability" | "ManaSymbol" | "Expr"))
    {
        return Err(format!(
            "@agree repeats a Keyword, Cost, ObjectFilter, Ability, ManaSymbol or Expr, not {ty:?}"
        ));
    }
    Ok(Pattern::Symbol(
        "agree".into(),
        args.iter()
            .map(|arg| {
                let mut inner = arg;
                while let Syntax::Typed(kind, body) = inner {
                    if *ty != Ty::Named(kind.clone()) {
                        return Err("@agree operands must have the result type".into());
                    }
                    inner = body;
                }
                elaborate(inner, ty, schema)
            })
            .collect::<Result<_, _>>()?,
    ))
}

fn qualified_members_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    if *ty != Ty::Named("ObjectSet".into()) {
        return Err("@qualified_members produces ObjectSet".into());
    }
    Ok(Pattern::Symbol(
        "qualified_members".into(),
        vec![elaborate(&args[0], &Ty::Named("ObjectRef".into()), schema)?],
    ))
}

fn draft_result_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    let expected = Ty::Named("Operation".into());
    if *ty != expected {
        return Err("@drafted_result produces Operation".into());
    }
    let mut operation = &args[0];
    while let Syntax::Typed(kind, inner) = operation {
        if kind != "Operation" {
            return Err("@drafted_result needs an Operation".into());
        }
        operation = inner;
    }
    Ok(Pattern::Symbol(
        "drafted_result".into(),
        vec![elaborate(operation, &expected, schema)?],
    ))
}

fn snapshot_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    let [body, amount] = args else {
        return Err("@where_snapshot needs a program and an expression".into());
    };
    let program = Ty::Named("Program".into());
    if *ty != program {
        return Err("@where_snapshot produces Program".into());
    }
    Ok(Pattern::Symbol(
        "where_snapshot".into(),
        vec![
            elaborate(body, &program, schema)?,
            elaborate(amount, &Ty::Named("Expr".into()), schema)?,
        ],
    ))
}

fn reference_marker(name: &str) -> bool {
    matches!(
        name,
        "member_count_it"
            | "last_exiled"
            | "last_exiled_creature"
            | "return_ref"
            | "sacrificed_damage_source"
            | "activation_creature_property"
            | "other_reference"
            | "another_target_reference"
            | "exiled_card_reference"
    )
}

fn reference_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Pattern, String> {
    if *ty != Ty::Named("ObjectRef".into()) {
        return Err(format!("@{name} produces ObjectRef"));
    }
    if name == "exiled_card_reference" {
        return if args.is_empty() {
            Ok(Pattern::Symbol(name.into(), vec![]))
        } else {
            Err("@exiled_card_reference takes no arguments".into())
        };
    }
    let argument = if matches!(name, "other_reference" | "another_target_reference") {
        "ObjectFilter"
    } else {
        "ObjectRef"
    };
    if args.len() != 1 {
        return Err(format!("@{name} needs one {argument}"));
    }
    if let Syntax::Typed(given, _) = &args[0]
        && given != argument
    {
        return Err(format!("@{name} needs a {argument}, not {given}"));
    }
    Ok(Pattern::Symbol(
        name.into(),
        vec![elaborate(&args[0], &Ty::Named(argument.into()), schema)?],
    ))
}

fn cost_marker(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Option<Pattern>, String> {
    let kind = match name {
        "singular_count" | "plural_count" => "Count",
        "plural_amount" => "Expr",
        "announced_x" => "CostPart",
        "entering_controller" => "Ability",
        "payment_cost" | "announced_cost" => "Cost",
        _ => return Ok(None),
    };
    if *ty != Ty::Named(kind.into()) {
        return Err(format!("@{name} marks a {kind}"));
    }
    let mut operands = vec![elaborate(&args[0], ty, schema)?];
    if kind == "Cost" {
        operands.push(elaborate(&args[1], &Ty::Named("Expr".into()), schema)?);
    }
    Ok(Some(Pattern::Symbol(name.into(), operands)))
}

fn selection_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Option<Pattern>, String> {
    if name == "selection_id" {
        let [Syntax::Int(index)] = args else {
            return Err("@selection_id needs a literal index".into());
        };
        if *ty != Ty::Named("SelectionId".into()) || !(0..256).contains(index) {
            return Err("@selection_id produces SelectionId with an index below 256".into());
        }
        return Ok(Some(Pattern::Symbol(
            name.into(),
            vec![Pattern::Value((*index).into())],
        )));
    }
    if name == "selection" {
        if args.len() != 1 || *ty != Ty::Named("ObjectSet".into()) {
            return Err("@selection needs one SelectionStep and produces ObjectSet".into());
        }
        return Ok(Some(Pattern::Symbol(
            name.into(),
            vec![elaborate(
                &args[0],
                &Ty::Named("SelectionStep".into()),
                schema,
            )?],
        )));
    }
    Ok(None)
}

fn class_levels_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    let expected = Ty::Vec(Box::new(Ty::Named("ClassLevel".into())));
    if *ty != expected || args.len() != 1 {
        return Err(
            "@class_levels needs one labelled tuple list and produces Vec<ClassLevel>".into(),
        );
    }
    let labelled = Ty::Vec(Box::new(Ty::Tuple(vec![
        Ty::Named("i64".into()),
        Ty::Named("ClassLevel".into()),
    ])));
    Ok(Pattern::Symbol(
        "class_levels".into(),
        vec![elaborate(&args[0], &labelled, schema)?],
    ))
}

fn optional_scope_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Pattern, String> {
    let expected = if matches!(name, "optional_program" | "optional_action_program") {
        Ty::Named("Program".into())
    } else {
        Ty::Vec(Box::new(Ty::Named("Instruction".into())))
    };
    let arity = if matches!(
        name,
        "single_reflexive_base"
            | "offered_action"
            | "captured_targets"
            | "announced_targets"
            | "optional_program"
            | "optional_action_program"
    ) {
        1
    } else {
        2
    };
    if *ty != expected || args.len() != arity {
        return Err(format!(
            "@{name} needs {arity} instruction lists and produces instructions"
        ));
    }
    Ok(Pattern::Symbol(
        name.into(),
        args.iter()
            .map(|arg| elaborate(arg, &expected, schema))
            .collect::<Result<_, _>>()?,
    ))
}

fn guarded_value_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Pattern, String> {
    let [value] = args else {
        return Err(format!("@{name} needs one argument"));
    };
    let expected = if matches!(name, "inline_counter_price" | "captured_counter_price") {
        "Expr"
    } else {
        "Cost"
    };
    if *ty != Ty::Named(expected.into()) {
        return Err(format!("@{name} guards a {expected}, not {ty:?}"));
    }
    Ok(Pattern::Symbol(
        name.into(),
        vec![elaborate(value, ty, schema)?],
    ))
}

fn specialized_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Option<Pattern>, String> {
    if matches!(name, "defining_static" | "defining_changes") {
        let expected = if name == "defining_static" {
            Ty::Named("Ability".into())
        } else {
            Ty::Vec(Box::new(Ty::Named("Change".into())))
        };
        if args.len() != 1 || *ty != expected {
            return Err(format!("@{name} takes one {expected:?} argument"));
        }
        return Ok(Some(Pattern::Symbol(
            name.into(),
            vec![elaborate(&args[0], ty, schema)?],
        )));
    }

    match name {
        "activation_removed_counters" => counter_payment_call(args, ty, schema).map(Some),
        "enchanted_creature_operand" => {
            let [reference] = args else {
                return Err("@enchanted_creature_operand needs one ObjectRef".into());
            };
            if *ty != Ty::Named("ObjectRef".into()) {
                return Err("@enchanted_creature_operand produces ObjectRef".into());
            }
            Ok(Some(Pattern::Symbol(
                name.into(),
                vec![elaborate(reference, ty, schema)?],
            )))
        }
        "departure_controller" => {
            let [event, filter] = args else {
                return Err("@departure_controller needs EventSpec and ObjectFilter".into());
            };
            if *ty != Ty::Named("EventSpec".into()) {
                return Err("@departure_controller produces EventSpec".into());
            }
            Ok(Some(Pattern::Symbol(
                name.into(),
                vec![
                    elaborate(event, ty, schema)?,
                    elaborate(filter, &Ty::Named("ObjectFilter".into()), schema)?,
                ],
            )))
        }
        "repeated_damage_actor" => {
            let [instruction] = args else {
                return Err("@repeated_damage_actor needs one Instruction".into());
            };
            if *ty != Ty::Named("Instruction".into()) {
                return Err("@repeated_damage_actor produces Instruction".into());
            }
            Ok(Some(Pattern::Symbol(
                name.into(),
                vec![elaborate(instruction, ty, schema)?],
            )))
        }
        "returned_conditional" | "returned_conditional_literal" => {
            let [instructions] = args else {
                return Err(format!("@{name} needs instructions"));
            };
            let expected = Ty::Vec(Box::new(Ty::Named("Instruction".into())));
            if *ty != expected {
                return Err(format!("@{name} produces instructions"));
            }
            Ok(Some(Pattern::Symbol(
                name.into(),
                vec![elaborate(instructions, ty, schema)?],
            )))
        }
        "implicit_return_grant" | "return_grant" => {
            let [operation] = args else {
                return Err(format!("@{name} needs one Operation"));
            };
            if *ty != Ty::Named("Operation".into()) {
                return Err(format!("@{name} produces Operation"));
            }
            Ok(Some(Pattern::Symbol(
                name.into(),
                vec![elaborate(operation, ty, schema)?],
            )))
        }
        "that_opponent" | "that_player_literal" => {
            opponent_reference_call(name, args, ty).map(Some)
        }
        "target_contrast" => {
            let [fallback, domain] = args else {
                return Err(
                    "@target_contrast needs ObjectRef fallback and ObjectFilter domain".into(),
                );
            };
            if *ty != Ty::Named("ObjectRef".into()) {
                return Err("@target_contrast produces ObjectRef".into());
            }
            Ok(Some(Pattern::Symbol(
                name.into(),
                vec![
                    elaborate(fallback, ty, schema)?,
                    elaborate(domain, &Ty::Named("ObjectFilter".into()), schema)?,
                ],
            )))
        }
        "indefinite_opponent_controller" => controller_annotation_call(args, ty, schema).map(Some),
        "event_free_once" => event_free_once_call(args, ty, schema).map(Some),
        "recipient_value" => recipient_value_call(args, ty, schema).map(Some),
        "distributive_pump_subject" | "pump_other_ref" | "pump_name_ref" => {
            pump_annotation_call(name, args, ty, schema).map(Some)
        }
        "damage_actor_value" | "its_stat" => {
            scalar_annotation_call(name, args, ty, schema).map(Some)
        }
        "literal_damage_itself"
        | "fight_prior_counterpart"
        | "reflexive_moved_fight_object"
        | "move_after_current_type"
        | "move_after_actor"
        | "move_after_property"
        | "move_after_other" => object_annotation_call(name, args, ty, schema).map(Some),
        "fixed_mana" | "variable_mana" | "inline_counter_price" | "captured_counter_price" => {
            guarded_value_call(name, args, ty, schema).map(Some)
        }
        "linked_return_literal"
        | "literal_damage_itself_objects"
        | "implicit_card_set"
        | "exile_followup" => {
            let [value] = args else {
                return Err(format!("@{name} needs one ObjectSet"));
            };
            if *ty != Ty::Named("ObjectSet".into()) {
                return Err(format!("@{name} produces ObjectSet"));
            }
            Ok(Some(Pattern::Symbol(
                name.into(),
                vec![elaborate(value, ty, schema)?],
            )))
        }
        "class_levels" => class_levels_call(args, ty, schema).map(Some),
        "and_sequence"
        | "then_sequence"
        | "optional_then"
        | "single_reflexive_base"
        | "offered_action"
        | "captured_targets"
        | "announced_targets"
        | "optional_program"
        | "optional_action_program" => optional_scope_call(name, args, ty, schema).map(Some),
        "computed_condition" => {
            let [condition] = args else {
                return Err("@computed_condition needs one Condition".into());
            };
            if *ty != Ty::Named("Condition".into()) {
                return Err("@computed_condition produces Condition".into());
            }
            Ok(Some(Pattern::Symbol(
                name.into(),
                vec![elaborate(condition, ty, schema)?],
            )))
        }
        "look_inline" => {
            let [operation] = args else {
                return Err("@look_inline needs one Operation".into());
            };
            if *ty != Ty::Named("Operation".into()) {
                return Err("@look_inline produces Operation".into());
            }
            Ok(Some(Pattern::Symbol(
                name.into(),
                vec![elaborate(operation, ty, schema)?],
            )))
        }
        "where_snapshot" => snapshot_call(args, ty, schema).map(Some),
        "capture_amount" => capture_amount_call(args, ty, schema).map(Some),
        _ => Ok(None),
    }
}

fn event_free_once_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    let [ability] = args else {
        return Err("@event_free_once needs one ability".into());
    };
    if *ty != Ty::Named("Ability".into()) {
        return Err("@event_free_once produces Ability".into());
    }
    Ok(Pattern::Symbol(
        "event_free_once".into(),
        vec![elaborate(ability, ty, schema)?],
    ))
}

fn opponent_reference_call(name: &str, args: &[Syntax], ty: &Ty) -> Result<Pattern, String> {
    if !args.is_empty() || *ty != Ty::Named("PlayerRef".into()) {
        return Err(format!("@{name} takes no arguments and produces PlayerRef"));
    }
    Ok(Pattern::Symbol(name.into(), Vec::new()))
}

fn controller_annotation_call(
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Pattern, String> {
    let [filter] = args else {
        return Err("@indefinite_opponent_controller needs one ObjectFilter".into());
    };
    if *ty != Ty::Named("ObjectFilter".into()) {
        return Err("@indefinite_opponent_controller produces ObjectFilter".into());
    }
    Ok(Pattern::Symbol(
        "indefinite_opponent_controller".into(),
        vec![elaborate(filter, ty, schema)?],
    ))
}

fn capture_amount_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    let [body, amount] = args else {
        return Err("@capture_amount needs instructions and an expression".into());
    };
    let instructions = Ty::Vec(Box::new(Ty::Named("Instruction".into())));
    if *ty != instructions {
        return Err("@capture_amount produces instructions".into());
    }
    Ok(Pattern::Symbol(
        "capture_amount".into(),
        vec![
            elaborate(body, &instructions, schema)?,
            elaborate(amount, &Ty::Named("Expr".into()), schema)?,
        ],
    ))
}

fn recipient_value_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Pattern, String> {
    let [recipients, value] = args else {
        return Err("@recipient_value needs recipients and a scalar expression".into());
    };
    if *ty != Ty::Named("Expr".into()) {
        return Err("@recipient_value produces Expr".into());
    }
    Ok(Pattern::Symbol(
        "recipient_value".into(),
        vec![
            elaborate(recipients, &Ty::Named("Recipients".into()), schema)?,
            elaborate(value, ty, schema)?,
        ],
    ))
}

fn scalar_annotation_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Pattern, String> {
    let [value] = args else {
        return Err(format!("@{name} needs one scalar expression"));
    };
    if *ty != Ty::Named("Expr".into()) {
        return Err(format!("@{name} produces Expr"));
    }
    Ok(Pattern::Symbol(
        name.into(),
        vec![elaborate(value, ty, schema)?],
    ))
}

fn object_annotation_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Pattern, String> {
    let [value] = args else {
        return Err(format!("@{name} needs one object reference"));
    };
    if *ty != Ty::Named("ObjectRef".into()) {
        return Err(format!("@{name} produces ObjectRef"));
    }
    Ok(Pattern::Symbol(
        name.into(),
        vec![elaborate(value, ty, schema)?],
    ))
}

fn pump_annotation_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Pattern, String> {
    let [value] = args else {
        return Err(format!("@{name} needs one typed operand"));
    };
    let expected = if name == "distributive_pump_subject" {
        "ObjectSet"
    } else {
        "ObjectRef"
    };
    if *ty != Ty::Named(expected.into()) {
        return Err(format!("@{name} produces {expected}"));
    }
    Ok(Pattern::Symbol(
        name.into(),
        vec![elaborate(value, ty, schema)?],
    ))
}

fn unordered_call(args: &[Syntax], ty: &Ty, schema: &Schema) -> Result<Option<Pattern>, String> {
    let [Syntax::List(items)] = args else {
        return Err("@unordered requires one list argument".into());
    };
    if !matches!(ty, Ty::Vec(_))
        || !(2..=4).contains(&items.len())
        || items
            .iter()
            .filter(|item| matches!(item, Syntax::Splice(_)))
            .count()
            > 1
    {
        return Err(
            "@unordered requires a Vec list of two to four elements and at most one splice".into(),
        );
    }
    Ok(Some(Pattern::Symbol(
        "unordered".into(),
        vec![elaborate(&args[0], ty, schema)?],
    )))
}

fn coin_call(
    name: &str,
    args: &[Syntax],
    ty: &Ty,
    schema: &Schema,
) -> Result<Option<Pattern>, String> {
    if name == "single_player" {
        if args.len() != 1 || *ty != Ty::Named("PlayerRef".into()) {
            return Err("@single_player takes one PlayerSet and produces PlayerRef".into());
        }
        return Ok(Some(Pattern::Symbol(
            name.into(),
            vec![elaborate(&args[0], &Ty::Named("PlayerSet".into()), schema)?],
        )));
    }
    if matches!(name, "coin_protocol" | "coin_receipt") {
        let expected = if name == "coin_protocol" {
            "CoinProtocol"
        } else {
            "ResultId"
        };
        if !args.is_empty() || *ty != Ty::Named(expected.into()) {
            return Err(format!(
                "@{name} takes no arguments and produces {expected}"
            ));
        }
        return Ok(Some(Pattern::Symbol(name.into(), Vec::new())));
    }
    Ok(None)
}
