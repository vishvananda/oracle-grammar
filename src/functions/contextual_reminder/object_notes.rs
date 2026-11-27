use super::evidence::{effects, in_resolution, step_operation, target_spec, unscoped};
use serde_json::Value;

pub(super) fn mentions(ability: &Value, mut visit: impl FnMut(&'static str) -> bool) -> bool {
    let args = &ability["args"];
    if ability["kind"] == "Triggered"
        && args["event"]["Player"]["event"] == "LosesLife"
        && is_controller(&args["event"]["Player"]["players"])
        && visit("YourDamageCausesLifeLoss")
    {
        return true;
    }
    if ability["kind"] == "Spell"
        && let Some(epic) = epic_announcement(args)
        && visit(epic)
    {
        return true;
    }
    if ability["kind"] == "Static"
        && args["affected"]["Objects"].is_object()
        && (makes_creature(&args["changes"], false) && visit("SummoningSickness")
            || args["affected"]["Objects"]["kind"] == "Zone"
                && grants_keyword(&args["changes"], "Wither")
                && visit("WitherGroup")
            || own_graveyard_cards(&args["affected"]["Objects"])
                && grants_mechanic(&args["changes"], "JumpStart")
                && visit("JumpStartFromYourGraveyard")
            || grants_mechanic(&args["changes"], "ReadAhead")
                && args["affected"]["Objects"]["kind"] == "Zone"
                && args["affected"]["Objects"]["args"]["scope"]["Shared"] == "Battlefield"
                && filter_proves(
                    &args["affected"]["Objects"]["args"]["filter"],
                    &|term| term["kind"] == "Subtype" && term["args"] == "Saga",
                    0,
                    &mut 256,
                )
                && visit("SagaReadAhead")
            || grants_sliver_regeneration(&args["changes"]) && visit("SliverRegeneration")
            || grants_source_prowess(args) && visit("SourceProwess")
            || grants_keyword(&args["changes"], "Flanking")
                && filter_proves(
                    &args["affected"]["Objects"]["args"]["filter"],
                    &|term| term["kind"] == "HasKeyword" && term["args"]["kind"] == "Flanking",
                    0,
                    &mut 256,
                )
                && visit("RepeatedFlanking")
            || makes_artifact_creature(&args["changes"]) && visit("CreatureEquipment")
            || replacement_notes(&args["changes"], &mut visit)
            || lifelink_notes(args, args, &mut visit))
    {
        return true;
    }
    if ability["kind"] == "Conditional"
        && args["otherwise"].as_array().is_some_and(Vec::is_empty)
        && let Some([inner]) = args["then"].as_array().map(Vec::as_slice)
        && inner["kind"] == "Static"
        && (lifelink_notes(&inner["args"], &inner["args"], &mut visit)
            || grants_source_prowess(&inner["args"]) && visit("SourceProwess"))
    {
        return true;
    }
    if let Some([step]) = args["body"]["instructions"].as_array().map(Vec::as_slice)
        && let Some(op) = step_operation(step)
        && op["kind"] == "Move"
        && exiles_one_card_face_down(&op["args"])
        && visit("FaceDownHiddenCard")
    {
        return true;
    }
    in_resolution(
        &args["body"],
        args,
        0,
        &mut 512,
        &mut |step, frame, previous| {
            if previous.is_some_and(|previous| seek_arrival(step, previous))
                && visit("SeekWholeLibrary")
            {
                return true;
            }
            let Some(op) = step_operation(step) else {
                return false;
            };
            match op["kind"].as_str() {
                Some("Attach") => {
                    attaches_equipment(&op["args"]["attachments"], &frame["announcement"])
                        && visit("EquipmentKeepsController")
                }
                Some("Keyword") => {
                    phases_out_for_controller(&op["args"], frame) && visit("PhaseOutUntilYourTurn")
                }
                Some("KeywordEach") => {
                    controlled_group_phases_out(&op["args"], frame)
                        && visit("ControlledGroupPhasesOut")
                        || target_player_group_phases_out(&op["args"], frame, previous)
                            && visit("TargetPlayerGroupPhasesOut")
                }
                Some("CreateDelayedTrigger") => {
                    delayed_controlled_group_phases_out(&op["args"], frame)
                        && visit("ControlledGroupPhasesOut")
                }
                Some("CreateTokens") => {
                    let tokens = &op["args"]["tokens"];
                    creature_tokens_without_haste(tokens) && visit("SummoningSickness")
                        || creates_group_with_infect(tokens) && visit("InfectTokenGroup")
                }
                Some("ChangeCombat") => {
                    op["args"]["change"]["Reselect"]["players_only"] == false
                        && visit("AttackReselection")
                }
                Some("Move") => unattachable_auras(&op["args"]).is_some_and(&mut visit),
                Some("CreateContinuousEffect") => {
                    grants_graveyard_mechanic(op, frame, "JumpStart")
                        && visit("JumpStartFromYourGraveyard")
                        || op["args"]["duration"] == "EndOfTurn"
                            && grants_graveyard_mechanic(op, frame, "Escape")
                            && visit("EscapeFromYourGraveyardThisTurn")
                        || op["args"]["affected"]["Objects"]["kind"] == "One"
                            && grants_keyword(&op["args"]["changes"], "Hexproof")
                            && grants_keyword(&op["args"]["changes"], "Indestructible")
                            && visit("HexproofAndIndestructible")
                        || lifelink_notes(&op["args"], frame, &mut visit)
                        || keeps_abilities_when_animated(&op["args"])
                            && visit("AnimationKeepsAbilities")
                        || op["args"]["affected"]["Objects"].is_object()
                            && (replacement_notes(&op["args"]["changes"], &mut visit)
                                || makes_artifact_creature(&op["args"]["changes"])
                                    && visit("CreatureEquipment"))
                }
                Some("ReflexiveTrigger") => {
                    effects(&op["args"]["body"]).take(16).any(|op_inside| {
                        grants_graveyard_mechanic(op_inside, &op["args"], "JumpStart")
                    }) && visit("JumpStartFromYourGraveyard")
                }
                _ => false,
            }
        },
    )
}

fn makes_artifact_creature(changes: &Value) -> bool {
    let adds = |kind| {
        changes.as_array().is_some_and(|changes| {
            changes.len() <= 32
                && changes
                    .iter()
                    .any(|change| change["kind"] == "AddCardType" && change["args"] == kind)
        })
    };
    adds("Artifact") && adds("Creature")
}

fn exiles_one_card_face_down(movement: &Value) -> bool {
    let objects = &movement["objects"];
    let amount = &objects["args"]["count"];
    movement["destination"]["Exile"]["visibility"] == "FaceDown"
        && objects["kind"] == "Take"
        && amount["kind"] == "Constant"
        && amount["args"] == 1
        && objects["args"]["from"] == "Top"
        && super::receipts::own_zone(
            &objects["args"]["objects"]["args"]["scope"]["Players"],
            "Library",
        )
}

fn grants_source_prowess(args: &Value) -> bool {
    let affected = &args["affected"]["Objects"];
    affected["kind"] == "One"
        && affected["args"]["kind"] == "Source"
        && grants_keyword(&args["changes"], "Prowess")
}

fn grants_sliver_regeneration(changes: &Value) -> bool {
    let Some([change]) = changes.as_array().map(Vec::as_slice) else {
        return false;
    };
    if change["kind"] != "GrantAbility" {
        return false;
    }
    let grant = unscoped(&change["args"]);
    let args = &grant["args"];
    grant["kind"] == "Activated"
        && effects(&args["body"]).take(16).any(|op| {
            let objects = &op["args"]["objects"];
            op["kind"] == "CreateRegenerationShields"
                && objects["kind"] == "One"
                && target_spec(&objects["args"], &args["announcement"]).is_some_and(|target| {
                    filter_proves(
                        &target["filter"],
                        &|term| term["kind"] == "Subtype" && term["args"] == "Sliver",
                        0,
                        &mut 256,
                    )
                })
        })
}

pub(super) fn regenerates_source(ability: &Value) -> bool {
    let ability = unscoped(ability);
    ability["kind"] == "Activated"
        && effects(&ability["args"]["body"]).take(16).any(|op| {
            let objects = &op["args"]["objects"];
            op["kind"] == "CreateRegenerationShields"
                && objects["kind"] == "One"
                && objects["args"]["kind"] == "Source"
        })
}

fn replacement_notes(changes: &Value, visit: &mut impl FnMut(&'static str) -> bool) -> bool {
    let Some(changes) = changes.as_array().filter(|items| items.len() <= 32) else {
        return false;
    };
    let replaces_card_types = changes.iter().any(|change| {
        change["kind"] == "SetCardTypes"
            && change["args"]
                .as_array()
                .is_some_and(|types| !types.is_empty())
    });
    if replaces_card_types && visit("ReplaceCardTypes") {
        return true;
    }
    if !changes.iter().any(|change| {
        change["kind"] == "CreatureTypes"
            && change["args"]["mode"] == "Set"
            && change["args"]["types"]["Listed"]
                .as_array()
                .is_some_and(|types| !types.is_empty())
    }) {
        return false;
    }
    let mut colors = changes.iter().filter(|change| change["kind"] == "Colors");
    let color = match (colors.next(), colors.next()) {
        (Some(change), None) if change["args"]["mode"] == "Set" => {
            change["args"]["colors"].as_array()
        }
        _ => None,
    };
    let renames = changes.iter().any(|change| {
        change["kind"] == "SetName" && change["args"].as_str().is_some_and(|name| !name.is_empty())
    });
    color.is_some_and(Vec::is_empty) && visit("RemoveColorsReplaceCreatureTypes")
        || renames && visit("ReplaceNamesAndCreatureTypes")
        || replaces_card_types && visit("ReplaceCardAndCreatureTypes")
        || color.is_some_and(|colors| !colors.is_empty())
            && (visit("ReplaceColorsAndCreatureTypes")
                || replaces_card_types
                    && (visit("ReplaceColorsAndTypes")
                        || renames && visit("ReplaceColorsTypesAndNames")))
}

fn keeps_abilities_when_animated(args: &Value) -> bool {
    args["affected"]["Objects"]["kind"] == "One"
        && args["changes"].as_array().is_some_and(|changes| {
            changes.len() <= 16
                && changes
                    .iter()
                    .any(|change| change["kind"] == "AddCardType" && change["args"] == "Creature")
                && changes.iter().all(|change| {
                    matches!(
                        change["kind"].as_str(),
                        Some("AddCardType" | "SetBasePowerToughness" | "AddSubtype")
                    )
                })
        })
}

fn lifelink_notes(
    args: &Value,
    frame: &Value,
    visit: &mut impl FnMut(&'static str) -> bool,
) -> bool {
    if !grants_keyword(&args["changes"], "Lifelink") {
        return false;
    }
    let affected = &args["affected"]["Objects"];
    let object = &affected["args"];
    let controlled =
        |filter: &Value| filter["kind"] == "ControlledBy" && is_controller(&filter["args"]);
    match affected["kind"].as_str() {
        Some("One") if object["kind"] == "Source" => visit("OwnSourceLifelink"),
        Some("One") => target_spec(object, &frame["announcement"]).is_some_and(|target| {
            filter_proves(&target["filter"], &controlled, 0, &mut 256)
                && visit("OwnCreatureLifelink")
        }),
        Some("Zone") if filter_proves(&object["filter"], &controlled, 0, &mut 256) => {
            visit("OwnGroupLifelink")
                || filter_proves(
                    &object["filter"],
                    &|filter| filter["kind"] == "Subtype" && filter["args"] == "Sliver",
                    0,
                    &mut 256,
                ) && visit("OwnSliverLifelink")
        }
        _ => false,
    }
}

fn unattachable_auras(movement: &Value) -> Option<&'static str> {
    let objects = &movement["objects"];
    let args = &objects["args"];
    let scope = &args["scope"]["Players"];
    let arrival = &movement["destination"]["Battlefield"];
    if !arrival.is_object()
        || objects["kind"] != "Zone"
        || args["kind"] != "Card"
        || scope["zone"] != "Graveyard"
    {
        return None;
    }
    let filter = &args["filter"];
    let aura_eligible = |leaf: &Value| {
        leaf["kind"] == "CardType" && leaf["args"] == "Enchantment"
            || leaf["kind"] == "Subtype" && leaf["args"] == "Aura"
    };
    if scope["players"]["kind"] == "All" {
        return (aura_eligible(filter)
            || filter["kind"] == "AnyOf"
                && filter["args"]
                    .as_array()
                    .is_some_and(|terms| terms.iter().take(8).any(aura_eligible)))
        .then_some("UnattachableAurasAllGraveyards");
    }
    if !super::receipts::own_zone(scope, "Graveyard") {
        return None;
    }
    if aura_eligible(filter) {
        return Some("UnattachableAuras");
    }
    let creature_filter = |leaf: &Value| leaf["kind"] == "CardType" && leaf["args"] == "Creature";
    let chosen_host = arrival["arrival"]["states"]
        .as_array()
        .is_some_and(|states| {
            states
                .iter()
                .take(8)
                .any(|state| creature_filter(&state["AttachedToChosen"]))
        });
    let restricts_hosts = filter_proves(
        filter,
        &|leaf| {
            let host = &leaf["args"];
            leaf["kind"] == "CouldEnchant"
                && host["kind"] == "Zone"
                && host["args"]["scope"]["Shared"] == "Battlefield"
                && host["args"]["kind"] == "Permanent"
                && creature_filter(&host["args"]["filter"])
        },
        0,
        &mut 256,
    );
    (chosen_host
        && restricts_hosts
        && filter_proves(
            filter,
            &|leaf| leaf["kind"] == "Subtype" && leaf["args"] == "Aura",
            0,
            &mut 256,
        ))
    .then_some("UnattachableCreatureAuras")
}

fn own_graveyard_cards(objects: &Value) -> bool {
    let args = &objects["args"];
    objects["kind"] == "Zone"
        && args["kind"] == "Card"
        && super::receipts::own_zone(&args["scope"]["Players"], "Graveyard")
}

fn grants_mechanic(changes: &Value, mechanic: &str) -> bool {
    changes.as_array().is_some_and(|changes| {
        changes.iter().take(16).any(|change| {
            change["kind"] == "GrantAbility"
                && change["args"]["kind"] == "Mechanic"
                && change["args"]["args"]["mechanic"] == mechanic
        })
    })
}

fn grants_graveyard_mechanic(operation: &Value, frame: &Value, mechanic: &str) -> bool {
    let args = &operation["args"];
    let affected = &args["affected"]["Objects"];
    let objects = &affected["args"]["objects"];
    operation["kind"] == "CreateContinuousEffect"
        && affected["kind"] == "InZone"
        && affected["args"]["zone"] == "Graveyard"
        && objects["kind"] == "One"
        && grants_mechanic(&args["changes"], mechanic)
        && target_spec(&objects["args"], &frame["announcement"]).is_some_and(|target| {
            filter_proves(
                &target["filter"],
                &|leaf| {
                    leaf["kind"] == "InZone"
                        && super::receipts::own_zone(&leaf["args"]["Players"], "Graveyard")
                },
                0,
                &mut 256,
            )
        })
}

fn seek_arrival(step: &Value, previous: &Value) -> bool {
    let Some(choice) = step_operation(previous) else {
        return false;
    };
    let Some(movement) = step_operation(step) else {
        return false;
    };
    let choice_args = &choice["args"];
    let objects = &choice_args["objects"];
    let scope = &objects["args"]["scope"]["Players"];
    let players = &scope["players"];
    let moved = &movement["args"]["objects"];
    choice["kind"] == "ChooseObjects"
        && objects["kind"] == "Zone"
        && objects["args"]["kind"] == "Card"
        && scope["zone"] == "Library"
        && players["kind"] == "One"
        && players["args"] == choice_args["player"]
        && choice_args["count"]["AsManyAsCan"]["kind"] == "Constant"
        && choice_args["count"]["AsManyAsCan"]["args"] == 1
        && matches!(choice_args["constraints"].as_array().map(Vec::as_slice),
            Some([constraint]) if constraint == "AtRandom")
        && movement["kind"] == "Move"
        && moved["kind"] == "Selected"
        && moved["args"] == previous["args"]["result"]
        && movement["args"]["destination"]["OwnersHand"].is_object()
}

fn epic_announcement(args: &Value) -> Option<&'static str> {
    let step = args["body"]["instructions"].as_array()?.last()?;
    let operation = step_operation(step)?;
    if operation["kind"] != "Keyword" || operation["args"]["action"] != "Epic" {
        return None;
    }
    let targets = args["announcement"]["targets"].as_array()?;
    Some(if targets.is_empty() {
        "EpicWithoutTargets"
    } else {
        "EpicWithTargets"
    })
}

pub(super) fn filter_proves(
    filter: &Value,
    leaf: &impl Fn(&Value) -> bool,
    depth: usize,
    remaining: &mut usize,
) -> bool {
    if depth == 8 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    let terms = filter["args"]
        .as_array()
        .filter(|items| !items.is_empty() && items.len() <= 8);
    let meets = |term: &Value| filter_proves(term, leaf, depth + 1, remaining);
    match filter["kind"].as_str() {
        Some("AllOf") => terms.is_some_and(|terms| terms.iter().any(meets)),
        Some("AnyOf") => terms.is_some_and(|terms| terms.iter().all(meets)),
        _ => leaf(filter),
    }
}

fn attaches_equipment(attachments: &Value, announcement: &Value) -> bool {
    let args = &attachments["args"];
    let filter = match attachments["kind"].as_str() {
        Some("Zone") => &args["filter"],
        Some("One" | "Targets") => {
            let object = if attachments["kind"] == "One" {
                args
            } else {
                attachments
            };
            let Some(target) = target_spec(object, announcement) else {
                return false;
            };
            &target["filter"]
        }
        _ => return false,
    };
    filter_proves(
        filter,
        &|term| term["kind"] == "Subtype" && term["args"] == "Equipment",
        0,
        &mut 256,
    )
}

pub(super) fn is_controller(players: &Value) -> bool {
    players["kind"] == "One" && players["args"]["kind"] == "AbilityController"
}

fn phases_out_for_controller(args: &Value, frame: &Value) -> bool {
    let phase = &args["action"]["PhaseOut"];
    if !phase.is_object() || !phase["until"].is_null() || !is_controller(&args["actor"]) {
        return false;
    }
    let object = &phase["object"];
    let controlled = |term: &Value| {
        term["kind"] == "SameObject" && term["args"]["kind"] == "Source"
            || term["kind"] == "ControlledBy" && is_controller(&term["args"])
    };
    match object["kind"].as_str() {
        Some("Source") => true,
        Some("EventSubject") => {
            let event = &frame["event"];
            ["BecomesTarget", "Object"]
                .iter()
                .any(|kind| filter_proves(&event[*kind]["objects"], &controlled, 0, &mut 256))
        }
        Some("Target") => target_spec(object, &frame["announcement"])
            .is_some_and(|target| filter_proves(&target["filter"], &controlled, 0, &mut 256)),
        _ => false,
    }
}

fn controlled_group_phases_out(args: &Value, frame: &Value) -> bool {
    let phase = &args["action"]["PhaseOut"];
    if !phase.is_object() || !phase["until"].is_null() || !is_controller(&args["actor"]) {
        return false;
    }
    let objects = &args["objects"];
    let filter = match objects["kind"].as_str() {
        Some("Targets") => {
            let Some(target) = target_spec(objects, &frame["announcement"]) else {
                return false;
            };
            &target["filter"]
        }
        Some("Zone") => &objects["args"]["filter"],
        _ => return false,
    };
    filter_proves(
        filter,
        &|term| term["kind"] == "ControlledBy" && is_controller(&term["args"]),
        0,
        &mut 256,
    )
}

fn target_player_group_phases_out(args: &Value, frame: &Value, previous: Option<&Value>) -> bool {
    let phase = &args["action"]["PhaseOut"];
    if !phase.is_object() || !phase["until"].is_null() || !is_controller(&args["actor"]) {
        return false;
    }
    let announcement = &frame["announcement"];
    let Some([target]) = announcement["targets"].as_array().map(Vec::as_slice) else {
        return false;
    };
    let count = &target["count"]["Exactly"];
    if target["kind"] != "Player" || count["kind"] != "Constant" || count["args"] != 1 {
        return false;
    }
    let mut objects = &args["objects"];
    if objects["kind"] == "Selected" {
        let Some(previous) = previous else {
            return false;
        };
        let Some(choice) = step_operation(previous) else {
            return false;
        };
        if choice["kind"] != "ChooseObjects"
            || !objects["args"].is_u64()
            || objects["args"] != previous["args"]["result"]
        {
            return false;
        }
        objects = &choice["args"]["objects"];
    }
    objects["kind"] == "Zone"
        && objects["args"]["scope"]["Shared"] == "Battlefield"
        && filter_proves(
            &objects["args"]["filter"],
            &|term| {
                let player = &term["args"]["args"];
                term["kind"] == "ControlledBy"
                    && term["args"]["kind"] == "One"
                    && player["kind"] == "Target"
                    && player["args"]["slot"] == target["slot"]
                    && player["args"]["index"] == 0
            },
            0,
            &mut 256,
        )
}

fn delayed_controlled_group_phases_out(delayed: &Value, frame: &Value) -> bool {
    // A delayed trigger does not announce new targets. Its single phasing
    // instruction can use the target population announced by the enclosing
    // effect. An independently targeted nested ability has its own frame.
    let Some([step]) = delayed["body"]["instructions"]
        .as_array()
        .map(Vec::as_slice)
    else {
        return false;
    };
    step_operation(step).is_some_and(|op| {
        op["kind"] == "KeywordEach"
            && op["args"]["objects"]["kind"] == "Targets"
            && controlled_group_phases_out(&op["args"], frame)
    })
}

fn makes_creature(changes: &Value, copy: bool) -> bool {
    let Some(items) = changes.as_array().filter(|items| items.len() <= 32) else {
        return false;
    };
    let adds_creature = items.iter().any(|change| {
        if copy {
            change["kind"] == "AddCardTypes" && has_creature(&change["args"])
        } else {
            change["kind"] == "AddCardType" && change["args"] == "Creature"
        }
    });
    adds_creature
        && !items.iter().any(|change| {
            change["kind"] == "GrantKeyword" && change["args"]["kind"] == "Haste"
                || change["kind"] == "AddAbilities" && has_haste(&change["args"])
        })
}

fn creature_tokens_without_haste(tokens: &Value) -> bool {
    tokens.as_array().is_some_and(|tokens| {
        !tokens.is_empty()
            && tokens.len() <= 16
            && tokens.iter().all(|token| {
                let recipe = &token["recipe"];
                let defined = &recipe["Defined"];
                if defined.is_object() {
                    return has_creature(&defined["card_types"])
                        && defined["abilities_of"].is_null()
                        && !has_haste(&defined["abilities"]);
                }
                makes_creature(&recipe["Copy"]["exceptions"], true)
            })
    })
}

fn has_creature(types: &Value) -> bool {
    types
        .as_array()
        .is_some_and(|types| types.len() <= 16 && types.iter().any(|ty| ty == "Creature"))
}

fn has_haste(abilities: &Value) -> bool {
    abilities.as_array().is_some_and(|abilities| {
        abilities.len() > 32
            || abilities.iter().any(|ability| {
                ability["kind"] == "Keyword"
                    && (ability["args"]["kind"] == "Haste" || ability["args"] == "Haste")
            })
    })
}

fn grants_keyword(changes: &Value, name: &str) -> bool {
    changes.as_array().is_some_and(|changes| {
        changes
            .iter()
            .take(32)
            .any(|change| change["kind"] == "GrantKeyword" && change["args"]["kind"] == name)
    })
}

fn creates_group_with_infect(tokens: &Value) -> bool {
    let Some([token]) = tokens.as_array().map(Vec::as_slice) else {
        return false;
    };
    let amount = &token["amount"];
    amount["kind"] == "Constant"
        && amount["args"].as_u64().is_some_and(|count| count > 1)
        && token["recipe"]["Defined"]["abilities"]
            .as_array()
            .is_some_and(|abilities| {
                abilities.iter().take(32).any(|ability| {
                    ability["kind"] == "Keyword" && ability["args"]["kind"] == "Infect"
                })
            })
}
