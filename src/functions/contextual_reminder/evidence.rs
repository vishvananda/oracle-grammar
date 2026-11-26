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
    let scope = &value["args"];
    if value["kind"] != "Scoped" || scope["zones"] != json!(["Hand"]) {
        return None;
    }
    let activation = unscoped(&scope["ability"]);
    if activation["kind"] != "Activated" {
        return None;
    }
    let search = activation["args"]["origin"]["Cycling"].get("search")?;
    if search.is_null() {
        return None;
    }
    let cost = activation["args"].get("cost")?;
    Some(
        json!({"kind":"List","args":[{"kind":"Cost","args":cost},{"kind":"Filter","args":search}]}),
    )
}

pub(super) fn cost_symbols(ability: &Value) -> impl Iterator<Item = &Value> {
    let ability = own_ability(ability);
    let args = &ability["args"];
    let cost = match ability["kind"].as_str() {
        Some("Activated" | "Mechanic" | "CostBranch") => &args["cost"],
        Some("Casting") if args.get("Permission").is_some() => &args["Permission"]["cost"],
        Some("Casting") => &args["AdditionalCost"]["cost"],
        _ => &Value::Null,
    };
    cost["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .take(32)
        .flat_map(|part| part["Mana"].as_array().into_iter().flatten().take(32))
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

pub(super) fn ward_grant_claims(ability: &Value) -> Vec<Value> {
    let ability = own_ability(ability);
    let affected = &ability["args"]["affected"]["Objects"];
    let subject = &affected["args"];
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
        Some("PhaseOut") => Some("PhaseOut"),
        Some("Rule") if args["GoadedBy"]["kind"] == "AbilityController" => Some("Goad"),
        Some("Casting") if flashback_permission(value).is_some() => Some("Flashback"),
        Some("ChooseValue") if args["kind"] == "Parity" => Some("Parity"),
        Some("Compare" | "Counters") if args["comparison"] == "SameParity" => Some("Parity"),
        _ if value["Named"] == "PoisonCounter" => Some("PoisonCounter"),
        _ => None,
    }
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
    let ability = own_ability(ability);
    let args = &ability["args"];
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
    if ability["kind"] == "Static"
        && filter["kind"] == "SameObject"
        && filter["args"]["kind"] == "Source"
        && changes.as_array().is_some_and(|changes| {
            changes
                .iter()
                .take(32)
                .any(|change| change["Rule"]["Prohibit"] == "Counter")
        })
        && visit("CounterProhibited")
    {
        return true;
    }
    let cast = cast_event(&args["event"]);
    in_resolution(&args["body"], 0, &mut 512, &mut |op| {
        if op["kind"] == "Keyword"
            && op["args"]["action"]["Prepare"]["object"]["kind"] == "Source"
            && visit("PreparedSource")
        {
            return true;
        }
        if op["kind"] != "CopyOnStack" {
            return false;
        }
        let copy = &op["args"];
        let object = &copy["object"];
        let filter = if object["kind"] == "EventSubject" {
            let Some(cast) = cast else {
                return false;
            };
            &cast["spells"]
        } else if let Some(target) = spell_target(object, &args["announcement"]) {
            &target["filter"]
        } else {
            return false;
        };
        visit("PermanentSpellCopyRule")
            || copy["times"]["kind"] == "Constant"
                && copy["times"]["args"] == 1
                && permanent_spell_filter(filter, 0, &mut 256)
                && visit("PermanentSpellCopy")
    })
}

fn cast_event(mut event: &Value) -> Option<&Value> {
    for _ in 0..12 {
        if event["Spell"]["event"] == "Cast" {
            return Some(&event["Spell"]);
        }
        event = if let Some(nth) = event.get("NthEachTurn") {
            nth.get("event")?
        } else {
            event.get("FirstEachTurn")?
        };
    }
    None
}

fn in_resolution(
    program: &Value,
    depth: usize,
    remaining: &mut usize,
    visit: &mut impl FnMut(&Value) -> bool,
) -> bool {
    if depth == 12 || *remaining == 0 {
        return false;
    }
    let Some(steps) = program["instructions"].as_array() else {
        return false;
    };
    for step in steps {
        if *remaining == 0 {
            break;
        }
        *remaining -= 1;
        let args = &step["args"];
        let found = match step["kind"].as_str() {
            Some("Effect") => visit(&args["operation"]),
            Some("If") => [&args["then"], &args["otherwise"]]
                .into_iter()
                .any(|body| in_resolution(body, depth + 1, remaining, visit)),
            Some("Optional" | "ForEachPlayer" | "ForEachObject" | "RepeatableWithLimit") => {
                in_resolution(&args["body"], depth + 1, remaining, visit)
            }
            Some("Group" | "Repeatable") => in_resolution(args, depth + 1, remaining, visit),
            _ => false,
        };
        if found {
            return true;
        }
    }
    false
}

fn effects(program: &Value) -> impl Iterator<Item = &Value> {
    program["instructions"]
        .as_array()
        .into_iter()
        .flatten()
        .take(64)
        .filter(|step| step["kind"] == "Effect")
        .map(|step| &step["args"]["operation"])
}

fn spell_target<'a>(object: &'a Value, announcement: &'a Value) -> Option<&'a Value> {
    if object["kind"] != "Target" || object["args"]["index"] != 0 {
        return None;
    }
    let slot = &object["args"]["slot"];
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
    let domain = &target["kind"]["Object"];
    (domain["kind"] == "Spell").then_some(target)
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
