use serde_json::{Value, json};

pub(super) fn unscoped(value: &Value) -> &Value {
    if value["$sym"] == "scope" {
        &value["args"][0]
    } else {
        value
    }
}

fn own_ability(value: &Value) -> &Value {
    let value = unscoped(value);
    if value["kind"] == "Scoped" {
        unscoped(&value["args"]["ability"])
    } else {
        value
    }
}

pub(super) fn visit_nodes<'a>(
    value: &'a Value,
    mut visit: impl FnMut(&'a Value, usize) -> bool,
) -> bool {
    fn walk<'a>(
        value: &'a Value,
        depth: usize,
        remaining: &mut usize,
        visit: &mut impl FnMut(&'a Value, usize) -> bool,
    ) -> bool {
        if *remaining == 0 {
            return false;
        }
        *remaining -= 1;
        if visit(value, *remaining) {
            return true;
        }
        if depth == 80 {
            return false;
        }
        match value {
            Value::Object(fields) => fields
                .values()
                .any(|child| walk(child, depth + 1, remaining, visit)),
            Value::Array(items) => items
                .iter()
                .any(|child| walk(child, depth + 1, remaining, visit)),
            _ => false,
        }
    }
    walk(value, 0, &mut 8192, &mut visit)
}

pub(super) fn cycling_claim(value: &Value) -> Option<Value> {
    let (cost, search) = cycling_operands(value)?;
    (!search.is_null()).then(|| {
        json!({"kind":"List","args":[{"kind":"Cost","args":cost},{"kind":"Filter","args":search}]})
    })
}

fn cycling_operands(value: &Value) -> Option<(&Value, &Value)> {
    let scope = &value["args"];
    if value["kind"] != "Scoped" || scope["zones"] != json!(["Hand"]) {
        return None;
    }
    let activation = unscoped(&scope["ability"]);
    if activation["kind"] != "Activated" {
        return None;
    }
    let search = activation["args"]["origin"]["Cycling"].get("search")?;
    let cost = activation["args"].get("cost")?;
    Some((cost, search))
}

pub(super) fn cycling_pair_claim(line: &Value, last: &Value) -> Vec<Value> {
    let Some([first, _]) = line.as_array().map(Vec::as_slice) else {
        return Vec::new();
    };
    let (Some((first_cost, first_filter)), Some((last_cost, last_filter))) =
        (cycling_operands(first), cycling_operands(last))
    else {
        return Vec::new();
    };
    if first_cost != last_cost
        || first_filter.is_null()
        || last_filter.is_null()
        || first_filter == last_filter
    {
        return Vec::new();
    }
    [[first_filter, last_filter], [last_filter, first_filter]]
        .into_iter()
        .map(|filters| {
            json!({"kind":"List","args":[
                {"kind":"Cost","args":first_cost},
                {"kind":"Filter","args":{"kind":"AnyOf","args":filters}}
            ]})
        })
        .collect()
}

pub(super) fn cycling_grant_claim(ability: &Value) -> Option<Value> {
    let ability = own_ability(ability);
    let args = &ability["args"];
    let objects = &args["affected"]["Objects"];
    if ability["kind"] != "Static"
        || objects["kind"] != "Zone"
        || objects["args"]["kind"] != "Card"
        || !super::receipts::own_zone(&objects["args"]["scope"]["Players"], "Hand")
    {
        return None;
    }
    let [change] = args["changes"].as_array()?.as_slice() else {
        return None;
    };
    if change["kind"] != "GrantAbility" {
        return None;
    }
    let (cost, search) = cycling_operands(&change["args"])?;
    search.is_null().then(|| json!({"kind":"Cost","args":cost}))
}

pub(super) fn cost_symbols(ability: &Value) -> Vec<Value> {
    let ability = own_ability(ability);
    let args = &ability["args"];
    let cost = match ability["kind"].as_str() {
        Some("Activated" | "Mechanic" | "CostBranch") => &args["cost"],
        Some("Casting") if args.get("Permission").is_some() => &args["Permission"]["cost"],
        Some("Casting") => &args["AdditionalCost"]["cost"],
        _ => &Value::Null,
    };
    let mut symbols = Vec::new();
    append_mana_symbols(cost, &mut symbols);
    visit_nodes(&args["body"], |value, _| {
        if value["kind"] == "PayCost" {
            append_mana_symbols(&value["args"]["cost"], &mut symbols);
        }
        symbols.len() >= 32
    });
    symbols
}

fn append_mana_symbols(cost: &Value, symbols: &mut Vec<Value>) {
    for symbol in cost["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .take(32)
        .flat_map(|part| part["Mana"].as_array().into_iter().flatten().take(32))
    {
        if symbols.len() == 32 {
            break;
        }
        if !symbols.contains(symbol) {
            symbols.push(symbol.clone());
        }
    }
}

pub(super) fn equip_payment_claim(ability: &Value) -> Option<Value> {
    let ability = own_ability(ability);
    let args = &ability["args"];
    if ability["kind"] != "Activated"
        || args["origin"] != "Equip"
        || args["limits"]["sorcery"] != true
    {
        return None;
    }
    let cost = args.get("cost")?;
    Some(json!({"kind":"Cost","args":cost}))
}

pub(super) fn unearth_grant_claim(ability: &Value) -> Option<Value> {
    let ability = own_ability(ability);
    let args = &ability["args"];
    let objects = &args["affected"]["Objects"];
    if ability["kind"] != "Static"
        || objects["kind"] != "Zone"
        || objects["args"]["kind"] != "Card"
        || !super::receipts::own_zone(&objects["args"]["scope"]["Players"], "Graveyard")
    {
        return None;
    }
    let [change] = args["changes"].as_array()?.as_slice() else {
        return None;
    };
    let granted = &change["args"];
    let granted = if granted["$sym"] == "holder" {
        granted["args"].get(0)?
    } else {
        unscoped(granted)
    };
    (change["kind"] == "GrantAbility"
        && granted["kind"] == "Mechanic"
        && granted["args"]["mechanic"] == "Unearth"
        && granted["args"]["cost"].is_object())
    .then(|| json!({"kind":"Cost","args":granted["args"]["cost"]}))
}

pub(super) fn ward_grant_claims(ability: &Value) -> Vec<Value> {
    let ability = own_ability(ability);
    let affected = &ability["args"]["affected"]["Objects"];
    let subject = &affected["args"];
    // The equipped noun records a mention until the enclosing frame binds it.
    let subject = if subject["$sym"] == "remember_operand" {
        &subject["args"][0]
    } else {
        subject
    };
    let attached = ability["kind"] == "Static"
        && affected["kind"] == "One"
        && subject["kind"] == "AttachedTo"
        && subject["args"]["kind"] == "Source";
    let mut claims = Vec::new();
    visit_nodes(ability, |node, _| {
        let grant = if node["kind"] == "GrantKeyword" {
            &node["args"]
        } else {
            &node["GrantKeyword"]
        };
        if grant["kind"] == "Ward" {
            for subject in ["Recipient", "Attached"]
                .into_iter()
                .take(if attached { 2 } else { 1 })
            {
                let claim = json!({"kind":"List","args":[{"kind":"Text","args":subject},{"kind":"Cost","args":grant["args"]}]});
                if !claims.contains(&claim) {
                    claims.push(claim);
                }
            }
        }
        false
    });
    claims
}

pub(super) fn awaken_claim(ability: &Value) -> Option<Value> {
    let ability = unscoped(ability);
    let args = &ability["args"];
    if ability["kind"] != "CostBranch" || args["keyword"] != "Awaken" {
        return None;
    }
    let paid = unscoped(&args["paid"]);
    if paid["kind"] != "Spell" {
        return None;
    }
    let op = effects(&paid["args"]["body"]).last()?;
    let actor = &op["args"]["actor"];
    if op["kind"] != "Keyword"
        || actor["kind"] != "One"
        || actor["args"]["kind"] != "AbilityController"
    {
        return None;
    }
    let amount = op["args"]["action"]["Awaken"].get("amount")?;
    Some(
        json!({"kind":"List","args":[{"kind":"Cost","args":args["cost"]},{"kind":"Number","args":amount}]}),
    )
}

pub(super) fn mention(value: &Value) -> Option<&'static str> {
    let args = &value["args"];
    match value["kind"].as_str() {
        Some("Shadow") => Some("Shadow"),
        Some("Skulk") => Some("Skulk"),
        Some("Flanking") => Some("Flanking"),
        Some("Intimidate") => Some("Intimidate"),
        Some("Wither") => Some("Wither"),
        Some("Infect") => Some("Infect"),
        Some("Defender") => Some("Defender"),
        Some("Horsemanship") => Some("Horsemanship"),
        Some("Changeling") => Some("Changeling"),
        Some("Landwalk") if args["kind"] == "Subtype" => match args["args"].as_str()? {
            "Plains" => Some("Plainswalk"),
            "Island" => Some("Islandwalk"),
            "Swamp" => Some("Swampwalk"),
            "Mountain" => Some("Mountainwalk"),
            "Forest" => Some("Forestwalk"),
            _ => None,
        },
        Some("Landwalk")
            if args["kind"] == "Not"
                && args["args"]["kind"] == "Supertype"
                && args["args"]["args"] == "Basic" =>
        {
            Some("Nonbasicwalk")
        }
        Some("Manifest") if args["cloak"] == false => Some("Manifest"),
        Some("Manifest") if args["cloak"] == true => Some("Cloak"),
        Some("ManaSpent") if args["filter"]["snow"] == true => Some("Snow"),
        Some("Seek") if args["top"].is_null() => Some("SeekWholeLibrary"),
        Some("Stickers") if args["measure"] == "UniqueVowels" => Some("StickerVowels"),
        Some("CreateTokens") => attached_role(args),
        Some("SeparatePiles" | "Piles") => Some("EmptyPiles"),
        Some("GameEffect")
            if args["effect"]["kind"] == "Term"
                && args["effect"]["args"]["family"] == "GameEffect"
                && args["effect"]["args"]["form"]
                    == "DefendingPlayersChooseCreaturesToDefendAttackersAtRandom" =>
        {
            Some("EmptyPiles")
        }
        Some("Rule")
            if args["CombatCost"]["action"].get("Attack").is_some()
                && args["CombatCost"]["payer"]["kind"] == "DeclaringPlayer" =>
        {
            Some("AttackDeclarationCost")
        }
        Some("Rule") if args["GoadedBy"]["kind"] == "AbilityController" => Some("Goad"),
        Some("Casting") if flashback_permission(value).is_some() => Some("Flashback"),
        Some("ChooseValue") if args["kind"] == "Parity" => Some("Parity"),
        Some("Compare" | "Counters") if args["comparison"] == "SameParity" => Some("Parity"),
        Some("Static" | "CreateContinuousEffect") if controller_hexproof(args) => {
            Some("PlayerHexproof")
        }
        Some("Mechanic")
            if args["mechanic"] == "BandsWithOthers" && legendary_creatures(&args["filter"]) =>
        {
            Some("LegendaryBands")
        }
        _ if value["Named"] == "PoisonCounter" => Some("PoisonCounter"),
        _ => None,
    }
}

fn attached_role(args: &Value) -> Option<&'static str> {
    let [entry] = args["entry"]["states"].as_array()?.as_slice() else {
        return None;
    };
    entry.get("AttachedTo")?;
    let [token] = args["tokens"].as_array()?.as_slice() else {
        return None;
    };
    let recipe = &token["recipe"]["Defined"];
    // The named rules recipe supplies the role abilities. A custom or copied
    // token can share a name or a subtype without supplying those abilities.
    if token["words"] != "Rules"
        || !matches!(recipe["card_types"].as_array()?.as_slice(), [kind] if kind == "Enchantment")
        || !matches!(recipe["subtypes"].as_array()?.as_slice(),
            [a, b] if (a == "Aura" && b == "Role") || (a == "Role" && b == "Aura"))
    {
        return None;
    }
    match recipe["name"]["Explicit"].as_str()? {
        "Wicked" => Some("WickedRole"),
        "Young Hero" => Some("YoungHeroRole"),
        "Sorcerer" => Some("SorcererRole"),
        "Monster" => Some("MonsterRole"),
        "Royal" => Some("RoyalRole"),
        "Cursed" => Some("CursedRole"),
        "Virtuous" => Some("VirtuousRole"),
        _ => None,
    }
}

pub(super) fn single_connive(value: &Value) -> bool {
    let amount = &value["args"]["action"]["Connive"]["amount"];
    amount.is_null() || (amount["kind"] == "Constant" && amount["args"] == 1)
}

fn controller_hexproof(args: &Value) -> bool {
    let players = &args["affected"]["Players"];
    players["kind"] == "One"
        && players["args"]["kind"] == "AbilityController"
        && args["changes"].as_array().is_some_and(|changes| {
            changes.iter().take(32).any(|change| {
                change["kind"] == "PlayerRule" && change["args"]["GrantKeyword"] == "Hexproof"
            })
        })
}

fn legendary_creatures(filter: &Value) -> bool {
    let is_creature = |v: &Value| v["kind"] == "CardType" && v["args"] == "Creature";
    let is_legendary = |v: &Value| v["kind"] == "Supertype" && v["args"] == "Legendary";
    filter["kind"] == "AllOf"
        && filter["args"]
            .as_array()
            .is_some_and(|items| match items.as_slice() {
                [a, b] => is_creature(a) && is_legendary(b) || is_creature(b) && is_legendary(a),
                _ => false,
            })
}

fn flashback_permission(value: &Value) -> Option<&Value> {
    let permission = &value["args"]["Permission"];
    (value["kind"] == "Casting"
        && permission["origin"] == "Flashback"
        && permission["from"]["Players"]["zone"] == "Graveyard"
        && permission["leave_stack"] == "Exile")
        .then_some(permission)
}

pub(super) fn flashback_at_mana_cost(value: &Value) -> bool {
    flashback_permission(value).is_some_and(|permission| {
        permission["player"]["kind"] == "OwnerOf"
            && permission["player"]["args"]["kind"] == "Source"
            && permission["cost"]["parts"] == json!([{"ManaCostOf":{"kind":"Source"}}])
    })
}

pub(super) fn measurement(value: &Value) -> Option<(&'static str, &Value)> {
    let cost = &value["Keyword"];
    match cost["action"].as_str() {
        Some("CollectEvidence") => return Some(("CollectEvidence", cost.get("amount")?)),
        Some("Blight") => return Some(("Blight", cost.get("amount")?)),
        _ => (),
    }
    let expends = value.get("NthEachTurn")?;
    let event = &expends["event"]["Player"];
    let test = &expends["test"];
    if event["event"] == "Expends"
        && event["players"]["kind"] == "One"
        && event["players"]["args"]["kind"] == "AbilityController"
        && test["kind"] == "Compare"
        && test["args"]["comparison"] == "Equal"
    {
        return Some(("Expend", test["args"].get("value")?));
    }
    None
}

pub(super) fn root_mentions(ability: &Value, mut visit: impl FnMut(&'static str) -> bool) -> bool {
    ability_mentions(ability, 0, &mut 512, &mut visit)
}

fn ability_mentions(
    ability: &Value,
    depth: usize,
    remaining: &mut usize,
    visit: &mut impl FnMut(&'static str) -> bool,
) -> bool {
    if depth == 8 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    let permanent = crate::functions::support::permanent_ability(ability);
    let ability = own_ability(ability);
    let args = &ability["args"];
    if ability["kind"] == "Conditional" {
        return [&args["then"], &args["otherwise"]]
            .into_iter()
            .any(|branch| {
                branch.as_array().is_some_and(|abilities| {
                    abilities
                        .iter()
                        .take(16)
                        .any(|child| ability_mentions(child, depth + 1, remaining, visit))
                })
            });
    }
    let encounter = &args["event"]["Player"];
    if encounter["event"] == "EncountersPhenomenon"
        && encounter["objects"]["kind"] == "SameObject"
        && encounter["objects"]["args"]["kind"] == "Source"
        && visit("EncounteredPhenomenon")
    {
        return true;
    }
    let changes = &args["changes"];
    let filter = &args["affected"]["Objects"]["args"]["filter"];
    if ability["kind"] == "Static" {
        if filter["kind"] == "SameObject"
            && filter["args"]["kind"] == "Source"
            && changes.as_array().is_some_and(|changes| {
                changes.iter().take(32).any(|change| {
                    change["kind"] == "Rule" && change["args"]["Prohibit"] == "Counter"
                })
            })
            && visit("CounterProhibited")
        {
            return true;
        }
        if changes
            .as_array()
            .is_some_and(|changes| changes.iter().take(32).any(replicate_grant))
            && visit("PermanentSpellCopyRule")
        {
            return true;
        }
        if excludes_creature(changes) && visit("NoncreatureForm") {
            return true;
        }
    }
    in_resolution(
        &args["body"],
        args,
        0,
        remaining,
        &mut |step, frame, previous| {
            if let Some(previous) = previous {
                if exiled_creature_returns(step, previous, frame) && visit("ReturnExiledCreature") {
                    return true;
                }
                if casts_created_copy(step, previous) && visit("PermanentSpellCopyRule") {
                    return true;
                }
            }
            let Some(op) = step_operation(step) else {
                return false;
            };
            if op["kind"] == "Keyword"
                && op["args"]["action"]["Prepare"]["object"]["kind"] == "Source"
                && visit("PreparedSource")
            {
                return true;
            }
            if noncreature_form(op) && visit("NoncreatureForm") {
                return true;
            }
            if op["kind"] == "Keyword"
                && op["args"]["action"].get("ExchangeControl").is_some()
                && visit("Indefinite")
            {
                return true;
            }
            // A delayed trigger's move uses the captured references of its creator.
            let move_frame = if frame.get("start").is_some() {
                args
            } else {
                frame
            };
            if battlefield_return(op, move_frame, permanent) && visit("BattlefieldReturn") {
                return true;
            }
            copy_mentions(op, frame, visit)
        },
    )
}

fn copy_mentions(op: &Value, frame: &Value, visit: &mut impl FnMut(&'static str) -> bool) -> bool {
    if op["kind"] != "CopyOnStack" {
        return false;
    }
    let copy = &op["args"];
    let object = &copy["object"];
    let event = &frame["event"];
    if object["kind"] == "EventStackObject" && event.get("SpellOrActivation").is_some() {
        return visit("PermanentSpellCopyRule");
    }
    let filter = if object["kind"] == "EventSubject" {
        let Some(cast) = cast_event(event) else {
            return false;
        };
        &cast["spells"]
    } else if let Some(target) = target_spec(object, &frame["announcement"])
        && target["kind"]["Object"]["kind"] == "Spell"
    {
        &target["filter"]
    } else {
        return false;
    };
    visit("PermanentSpellCopyRule")
        || aura_eligible_copy(filter) && visit("AuraSpellCopyRule")
        || copy["times"]["kind"] == "Constant"
            && copy["times"]["args"] == 1
            && (permanent_spell_filter(filter, 0, &mut 256) && visit("PermanentSpellCopy")
                || super::object_notes::filter_proves(
                    filter,
                    &|leaf| leaf["kind"] == "CardType" && leaf["args"] == "Creature",
                    0,
                    &mut 256,
                ) && visit("CreatureSpellCopy"))
}

fn aura_eligible_copy(filter: &Value) -> bool {
    filter["kind"] == "Subtype" && filter["args"] == "Aura"
        || filter["kind"] == "AllOf"
            && filter["args"].as_array().is_some_and(|terms| {
                matches!(terms.as_slice(), [_, _])
                    && terms
                        .iter()
                        .any(|term| term["kind"] == "OnStack" && term["args"] == "SingleTarget")
                    && terms.iter().any(|term| {
                        let objects = &term["args"]["args"];
                        term["kind"] == "TargetsAny"
                            && term["args"]["kind"] == "Objects"
                            && objects["kind"] == "Zone"
                            && objects["args"]["scope"]["Shared"] == "Battlefield"
                            && objects["args"]["kind"] == "Permanent"
                            && is_creature_filter(&objects["args"]["filter"], 0, &mut 32)
                    })
            })
}

/// The trigger watches this source's cast and copies that exact stack object.
/// The caller supplies the separate proof that the source is a permanent spell.
pub(super) fn copies_source_spell(ability: &Value) -> bool {
    let ability = own_ability(ability);
    let frame = &ability["args"];
    if ability["kind"] != "Triggered"
        || !cast_event(&frame["event"]).is_some_and(|cast| {
            cast["spells"]["kind"] == "SameObject" && cast["spells"]["args"]["kind"] == "Source"
        })
    {
        return false;
    }
    in_resolution(&frame["body"], frame, 0, &mut 128, &mut |step, scope, _| {
        std::ptr::eq(scope, frame)
            && step_operation(step).is_some_and(|op| {
                let copy = &op["args"];
                op["kind"] == "CopyOnStack"
                    && matches!(
                        copy["object"]["kind"].as_str(),
                        Some("Source" | "EventSubject")
                    )
                    && copy["times"]["kind"] == "Constant"
                    && copy["times"]["args"] == 1
            })
    })
}

fn replicate_grant(change: &Value) -> bool {
    if change["kind"] != "PlayerRule" {
        return false;
    }
    let terms = &change["args"]["SpellsHaveAbilities"]["abilities"];
    terms["kind"] == "List"
        && terms["args"].as_array().is_some_and(|items| {
            items.iter().take(32).any(|item| {
                item["kind"] == "Term"
                    && item["args"]["family"] == "SpellEffect"
                    && item["args"]["form"] == "Replicate"
            })
        })
}

fn cast_event(mut event: &Value) -> Option<&Value> {
    for _ in 0..12 {
        if event["Spell"]["event"] == "Cast" {
            return Some(&event["Spell"]);
        }
        event = if let Some(wrapper) = event.get("NthEachTurn").or_else(|| event.get("Filtered")) {
            wrapper.get("event")?
        } else {
            event.get("FirstEachTurn")?
        };
    }
    None
}

pub(super) fn in_resolution(
    program: &Value,
    frame: &Value,
    depth: usize,
    remaining: &mut usize,
    visit: &mut impl FnMut(&Value, &Value, Option<&Value>) -> bool,
) -> bool {
    if depth == 12 || *remaining == 0 {
        return false;
    }
    let Some(steps) = program["instructions"].as_array() else {
        return false;
    };
    for (index, step) in steps.iter().enumerate() {
        if *remaining == 0 {
            break;
        }
        *remaining -= 1;
        let args = &step["args"];
        let previous = index.checked_sub(1).map(|i| &steps[i]);
        let found = visit(step, frame, previous)
            || match step["kind"].as_str() {
                Some("Effect") => {
                    let operation = &args["operation"];
                    let op_args = &operation["args"];
                    match operation["kind"].as_str() {
                        Some("CreateTrigger" | "CreateDelayedTrigger") => {
                            in_resolution(&op_args["body"], op_args, depth + 1, remaining, visit)
                        }
                        Some("PlayersDo") => op_args["body"].as_array().is_some_and(|body| {
                            body.iter().take(32).any(|step| {
                                step["kind"] == "Program"
                                    && in_resolution(
                                        &step["args"],
                                        frame,
                                        depth + 1,
                                        remaining,
                                        visit,
                                    )
                            })
                        }),
                        _ => false,
                    }
                }
                Some("If") => [&args["then"], &args["otherwise"]]
                    .into_iter()
                    .any(|body| in_resolution(body, frame, depth + 1, remaining, visit)),
                Some("Optional" | "ForEachPlayer" | "ForEachObject" | "RepeatableWithLimit") => {
                    in_resolution(&args["body"], frame, depth + 1, remaining, visit)
                }
                Some("Group" | "Repeatable") => {
                    in_resolution(args, frame, depth + 1, remaining, visit)
                }
                _ => false,
            };
        if found {
            return true;
        }
    }
    false
}

fn excludes_creature(changes: &Value) -> bool {
    changes.as_array().is_some_and(|items| {
        items.iter().take(32).any(|change| {
            change["kind"] == "SetCardTypes"
                && change["args"].as_array().is_some_and(|types| {
                    !types.is_empty()
                        && types.len() <= 16
                        && types.iter().all(|kind| kind != "Creature")
                })
        })
    })
}

fn noncreature_form(operation: &Value) -> bool {
    let args = &operation["args"];
    match operation["kind"].as_str() {
        Some("Move") => args["destination"]["Battlefield"]["arrival"]["states"]
            .as_array()
            .is_some_and(|states| {
                states
                    .iter()
                    .take(32)
                    .any(|state| excludes_creature(&state["Changes"]))
            }),
        Some("ModifyEvent") => excludes_creature(&args["change"]["Entry"]["CopyOf"]["exceptions"]),
        _ => false,
    }
}

fn battlefield_return(operation: &Value, frame: &Value, permanent: bool) -> bool {
    let args = &operation["args"];
    if operation["kind"] != "Move" || args["destination"].get("OwnersHand").is_none() {
        return false;
    }
    let set = &args["objects"];
    let object = if set["kind"] == "One" {
        &set["args"]
    } else {
        set
    };
    if let Some(target) = target_spec(object, &frame["announcement"]) {
        let domain = &target["kind"]["Object"];
        return domain["kind"] == "Permanent" && domain["zones"] == json!(["Battlefield"]);
    }
    if object["kind"] == "Source" {
        return permanent;
    }
    let event = &frame["event"];
    object["kind"] == "EventSubject" && combat_events(event, 0, &mut 32)
        || object["kind"] == "BlockPair"
            && object["args"] == "Attacker"
            && event["Pair"]["event"] == "Blocks"
}

pub(super) fn step_operation(step: &Value) -> Option<&Value> {
    if step["kind"] == "Effect" {
        step["args"].get("operation")
    } else {
        None
    }
}

fn casts_created_copy(step: &Value, previous: &Value) -> bool {
    let Some(copy) = step_operation(previous) else {
        return false;
    };
    if copy["kind"] != "CopyCards" {
        return false;
    }
    let cast = if step["kind"] == "Optional" {
        let Some([cast]) = step["args"]["body"]["instructions"]
            .as_array()
            .map(Vec::as_slice)
        else {
            return false;
        };
        cast
    } else if let Some(payment) = step_operation(step).filter(|op| op["kind"] == "PayCost") {
        let args = &payment["args"];
        let Some([part]) = args["cost"]["parts"].as_array().map(Vec::as_slice) else {
            return false;
        };
        let action = &part["PlayerAction"];
        let Some([cast]) = action["body"]["instructions"].as_array().map(Vec::as_slice) else {
            return false;
        };
        if action["player"] != args["player"]
            || !step_operation(cast).is_some_and(|play| {
                let actor = &play["args"]["actor"];
                actor["kind"] == "One" && actor["args"] == action["player"]
            })
        {
            return false;
        }
        cast
    } else {
        step
    };
    let Some(play) = step_operation(cast) else {
        return false;
    };
    let cards = &play["args"]["cards"];
    play["kind"] == "PlayCards"
        && play["args"]["action"] == "CastSpell"
        && cards["kind"] == "Copies"
        && cards["args"].is_u64()
        && cards["args"] == previous["args"]["result"]
}

fn exiled_creature_returns(step: &Value, previous: &Value, frame: &Value) -> bool {
    let (Some(returns), Some(moves)) = (step_operation(step), step_operation(previous)) else {
        return false;
    };
    let moved = &moves["args"];
    let receipt = &returns["args"]["objects"];
    if returns["kind"] != "ReturnAfter"
        || moves["kind"] != "Move"
        || moved["destination"].get("Exile").is_none()
        || moved["objects"]["kind"] != "One"
        || receipt["kind"] != "Transition"
        || receipt["args"]["phase"] != "After"
        || !receipt["args"]["result"].is_u64()
        || receipt["args"]["result"] != previous["args"]["result"]
    {
        return false;
    }
    // CR 610.3c returns the creature to the battlefield under its owner's control.
    let object = &moved["objects"]["args"];
    if let Some(target) = target_spec(object, &frame["announcement"]) {
        let domain = &target["kind"]["Object"];
        return domain["kind"] == "Permanent"
            && domain["zones"]
                .as_array()
                .is_some_and(|zones| matches!(zones.as_slice(), [zone] if zone == "Battlefield"))
            && is_creature_filter(&target["filter"], 0, &mut 32);
    }
    let damage = &frame["event"]["Damage"];
    (object["kind"] == "EventSubject" || object["kind"] == "Damage" && object["args"] == "Source")
        && damage["source_kind"] == "Permanent"
        && damage["subject"] == "Source"
        && is_creature_filter(&damage["sources"], 0, &mut 32)
}

fn is_creature_filter(filter: &Value, depth: usize, remaining: &mut usize) -> bool {
    if depth == 8 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    let args = &filter["args"];
    match filter["kind"].as_str() {
        Some("CardType") => args == "Creature",
        Some("AllOf" | "AnyOf") => args.as_array().is_some_and(|items| {
            if items.is_empty() || items.len() > 16 {
                return false;
            }
            let mut proven = items
                .iter()
                .map(|item| is_creature_filter(item, depth + 1, remaining));
            if filter["kind"] == "AllOf" {
                proven.any(|is_creature| is_creature)
            } else {
                proven.all(|is_creature| is_creature)
            }
        }),
        _ => false,
    }
}

fn combat_events(event: &Value, depth: usize, remaining: &mut usize) -> bool {
    if depth == 8 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    matches!(
        event["Object"]["event"].as_str(),
        Some("Attacks" | "Blocks" | "BecomesBlocked")
    ) || event["AnyOf"].as_array().is_some_and(|items| {
        !items.is_empty()
            && items.len() <= 8
            && items
                .iter()
                .all(|item| combat_events(item, depth + 1, remaining))
    })
}

pub(super) fn effects(program: &Value) -> impl Iterator<Item = &Value> {
    program["instructions"]
        .as_array()
        .into_iter()
        .flatten()
        .take(64)
        .filter(|step| step["kind"] == "Effect")
        .map(|step| &step["args"]["operation"])
}

pub(super) fn target_spec<'a>(object: &'a Value, announcement: &'a Value) -> Option<&'a Value> {
    let slot = match object["kind"].as_str() {
        Some("Target") if object["args"]["index"] == 0 => &object["args"]["slot"],
        Some("Targets") => &object["args"],
        _ => return None,
    };
    let target = if slot["$sym"] == "target" {
        slot["args"].get(0)?
    } else {
        let slot = slot.as_u64()?;
        announcement["targets"]
            .as_array()?
            .iter()
            .take(64)
            .find(|target| target["slot"] == slot)?
    };
    Some(target)
}

fn permanent_spell_filter(filter: &Value, depth: usize, remaining: &mut usize) -> bool {
    if depth == 12 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    match filter["kind"].as_str() {
        Some("CardType") => matches!(
            filter["args"].as_str(),
            Some("Artifact" | "Battle" | "Creature" | "Enchantment" | "Land" | "Planeswalker")
        ),
        Some("Subtype") => matches!(filter["args"].as_str(), Some("Equipment" | "Aura")),
        Some("AllOf" | "AnyOf") => filter["args"].as_array().is_some_and(|items| {
            if items.is_empty() || items.len() > 32 {
                return false;
            }
            if filter["kind"] == "AllOf" {
                items
                    .iter()
                    .any(|item| permanent_spell_filter(item, depth + 1, remaining))
            } else {
                items
                    .iter()
                    .all(|item| permanent_spell_filter(item, depth + 1, remaining))
            }
        }),
        _ => false,
    }
}
