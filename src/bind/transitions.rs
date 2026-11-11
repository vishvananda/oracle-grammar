//! A zone change makes a new object. Later symbolic references follow that
//! object's result, without rewriting literal references in mined declarations.
use super::{Step, at_path, events::Frame, symbol_name};
use serde_json::{Value, json};

pub(super) struct Introductions<'a> {
    pub(super) slots: &'a [(Vec<Step>, usize)],
    pub(super) base: usize,
    pub(super) keys: &'a [super::references::TargetIntroduction],
}

pub(super) fn reference(
    name: &str,
    root: &Value,
    path: &[Step],
    introductions: &Introductions<'_>,
    frame: &Frame,
    mut reference: Value,
    counter_choice: Option<u64>,
) -> Result<Value, String> {
    if let Some(current) = super::move_after::reference(
        root,
        path,
        introductions.keys,
        introductions.base,
        &reference,
    )? {
        return Ok(current);
    }
    if name == "group_reference" {
        return group_reference(root, path, introductions, reference);
    }
    let counter = if matches!(name, "counter_reference" | "counter_reference_single")
        && reference["kind"] == "Affected"
    {
        reference["args"].as_u64()
    } else if reference["kind"] == "Query" && reference["args"]["objects"]["kind"] == "Affected" {
        reference["args"]["objects"]["args"].as_u64()
    } else {
        None
    };
    if let Some(result) = counter.filter(|result| Some(*result) == counter_choice) {
        counter_focus(
            root,
            path,
            result,
            !matches!(name, "counter_reference" | "counter_reference_single"),
        )?;
    }
    if !matches!(
        name,
        "qualified_object"
            | "other_reference"
            | "another_target_reference"
            | "exiled_card_reference"
            | "it"
            | "current_it"
            | "self"
            | "named_source"
            | "that_object"
            | "named_object"
            | "current_named_object"
            | "defined_it"
            | "defined_object"
            | "defined_named_object"
    ) {
        return Ok(reference);
    }
    // A created trigger/replacement keeps its ability holder, even when its
    // creator's matched object departed. Its new event cannot bind that old role.
    if matches!(name, "self" | "named_source") && new_event_scope(root, path) {
        return Ok(if reference["kind"] == "EventSubject" {
            json!({"kind":"Source"})
        } else {
            reference
        });
    }
    let current = matches!(name, "current_it" | "current_named_object");
    if !current && information(root, path) {
        if matches!(path.last(), Some(Step::Key(key)) if key == "source")
            && get(root, &path[..path.len().saturating_sub(2)])
                .is_some_and(|operation| operation["kind"] == "DealDamage")
        {
            return frame.damage_source(reference);
        }
        return Ok(reference);
    }
    if (current
        || path
            .iter()
            .any(|step| matches!(step, Step::Key(key) if key == "instructions")))
        && !new_event_scope(root, path)
    {
        if moving_object(root, path) {
            reference = frame.returning_aura(reference)?;
        }
        reference = frame.reference(reference, current)?;
    }
    // The target filter is evaluated at announcement, even when its words
    // follow an earlier effect. Its current-card view precedes all those moves.
    if announcement(root, path) {
        return Ok(reference);
    }
    if !matches!(name, "self" | "named_source")
        && let Some(after) =
            super::payment_move_after::reference(root, path, introductions, &reference)?
    {
        return Ok(after);
    }
    for (depth, step) in path.iter().enumerate() {
        let Step::Index(index) = step else {
            continue;
        };
        if !matches!(depth.checked_sub(1).and_then(|d| path.get(d)),
            Some(Step::Key(key)) if key == "instructions")
        {
            continue;
        }
        let Some(instructions) = get(root, &path[..depth]).and_then(Value::as_array) else {
            continue;
        };
        for (before, instruction) in instructions.iter().take(*index).enumerate() {
            let operation = &instruction["args"]["operation"];
            if instruction["kind"] != "Effect"
                || operation["kind"] != "Move"
                || !public_destination(&operation["args"]["destination"])
            {
                continue;
            }
            let mut prefix = path[..depth].to_vec();
            prefix.push(Step::Index(before));
            prefix
                .extend(["args", "operation", "args", "objects"].map(|key| Step::Key(key.into())));
            let mut objects = &operation["args"]["objects"];
            loop {
                if matches!(
                    symbol_name(objects),
                    Some("zone_members" | "exile_followup")
                ) {
                    objects = &objects["args"][0];
                    prefix.extend([Step::Key("args".into()), Step::Index(0)]);
                } else if objects["kind"] == "InZone" {
                    objects = &objects["args"]["objects"];
                    prefix.extend(["args", "objects"].map(|key| Step::Key(key.into())));
                } else {
                    break;
                }
            }
            let projected = if objects["kind"] == "Targets"
                && super::movement::zone(&operation["args"]["destination"]) == Some("Exile")
                && !new_event_scope(root, path)
                && super::optional_exile_target::implicit_action(root, path)?
            {
                super::optional_exile_target::reference(root, objects, introductions)?
            } else {
                None
            };
            if objects["kind"] != "One" && projected.is_none() {
                continue;
            }
            let Some(result) = instruction["args"]["result"].as_u64() else {
                continue;
            };
            let mut moved = projected.unwrap_or_else(|| objects["args"].clone());
            // Target specifications move into the announcement only after all
            // references bind. Resolve their still-present slot markers here.
            if objects["kind"] == "One" {
                prefix.push(Step::Key("args".into()));
                for (introduced, slot) in introductions.slots {
                    if let Some(relative) = introduced.strip_prefix(prefix.as_slice())
                        && let Some(value) = at_path(&mut moved, relative)
                    {
                        *value = json!(introductions.base + slot);
                    }
                }
            }
            if moved == reference {
                reference = json!({"kind":"Result", "args":{
                    "result":result, "phase":"After", "index":0,
                }});
            }
        }
    }
    Ok(reference)
}

/// A whole announced or matched group follows its actual public move result.
/// Private destinations do not establish this public tracking contract.
fn group_reference(
    root: &Value,
    path: &[Step],
    introductions: &Introductions<'_>,
    mut group: Value,
) -> Result<Value, String> {
    if announcement(root, path) {
        return Ok(group);
    }
    let origin = group["args"].as_u64().and_then(|slot| {
        introductions
            .slots
            .iter()
            .find(|(_, local)| (introductions.base + local) as u64 == slot)
            .map(|(path, _)| path)
    });
    let mut zone = None;
    for (depth, step) in path.iter().enumerate() {
        let Step::Index(index) = step else { continue };
        if !matches!(depth.checked_sub(1).and_then(|d| path.get(d)),
            Some(Step::Key(key)) if key == "instructions")
        {
            continue;
        }
        let Some(items) = get(root, &path[..depth]).and_then(Value::as_array) else {
            continue;
        };
        let start = origin
            .and_then(|origin| origin.strip_prefix(&path[..depth]))
            .and_then(|suffix| match suffix.first() {
                Some(Step::Index(index)) => Some(*index),
                _ => None,
            })
            .unwrap_or(0);
        for (before, instruction) in items.iter().take(*index).enumerate().skip(start) {
            if instruction["kind"] != "Effect" {
                continue;
            }
            let operation = &instruction["args"]["operation"];
            let kind = operation["kind"].as_str().unwrap_or_default();
            if matches!(
                kind,
                "CreateTokens"
                    | "Search"
                    | "ChooseObjects"
                    | "NoteObjects"
                    | "Conjure"
                    | "DraftFromSpellbook"
                    | "Manifest"
                    | "Destroy"
                    | "Sacrifice"
            ) || kind == "PutCounters" && group["kind"] == "EventGroup"
            {
                return Err("a later object action replaces the plural group's focus".into());
            }
            let Some(relative) = population_path(operation) else {
                continue;
            };
            let mut prefix = path[..depth].to_vec();
            prefix.push(Step::Index(before));
            prefix.extend(["args", "operation"].map(|key| Step::Key(key.into())));
            prefix.extend(relative.clone());
            let mut moved = get(operation, &relative)
                .ok_or("object population path")?
                .clone();
            for (introduced, slot) in introductions.slots {
                if let Some(relative) = introduced.strip_prefix(prefix.as_slice())
                    && let Some(value) = at_path(&mut moved, relative)
                {
                    *value = json!(introductions.base + slot);
                }
            }
            if group_base(&moved) != group_base(&group) {
                return Err("a later object population replaces the plural group's focus".into());
            }
            if kind != "Move" {
                continue;
            }
            let destination = public_zone(&operation["args"]["destination"])
                .ok_or("a plural reference cannot track this move destination")?;
            let result = instruction["args"]["result"]
                .as_u64()
                .ok_or("a group move needs its actual result")?;
            group = json!({"kind":"Transition","args":{"result":result,"phase":"After"}});
            zone = Some(destination);
        }
    }
    if moving_object(root, path)
        && let Some(zone) = zone
    {
        group = json!({"kind":"InZone","args":{"objects":group,"zone":zone}});
    }
    Ok(group)
}

fn group_base(mut objects: &Value) -> &Value {
    while objects["kind"] == "InZone" {
        objects = &objects["args"]["objects"];
    }
    objects
}

fn population_path(operation: &Value) -> Option<Vec<Step>> {
    let keys = match operation["kind"].as_str()? {
        "Move" | "SetTapped" => vec![Step::Key("args".into()), Step::Key("objects".into())],
        "CreateContinuousEffect" if operation["args"]["affected"].get("Objects").is_some() => {
            ["args", "affected", "Objects"]
                .map(|key| Step::Key(key.into()))
                .to_vec()
        }
        "PutCounters"
            if operation["args"]["placements"].as_array()?.len() == 1
                && operation["args"]["placements"][0]["recipients"]["kind"] == "Objects" =>
        {
            vec![
                Step::Key("args".into()),
                Step::Key("placements".into()),
                Step::Index(0),
                Step::Key("recipients".into()),
                Step::Key("args".into()),
            ]
        }
        _ => return None,
    };
    Some(keys)
}

fn public_zone(destination: &Value) -> Option<&'static str> {
    super::movement::zone(destination)
        .filter(|zone| matches!(*zone, "Exile" | "Graveyard" | "Battlefield"))
}

/// A movement's operand can name the Aura's graveyard incarnation. Source,
/// owner/controller queries and unrelated actions still name their original role.
fn moving_object(root: &Value, path: &[Step]) -> bool {
    (0..path.len()).any(|depth| {
        get(root, &path[..depth]).is_some_and(|v| v["kind"] == "Move")
            && matches!(path.get(depth), Some(Step::Key(key)) if key == "args")
            && matches!(path.get(depth + 1), Some(Step::Key(key)) if key == "objects")
    })
}

pub(super) fn announcement(root: &Value, path: &[Step]) -> bool {
    path.iter()
        .any(|step| matches!(step, Step::Key(key) if key == "announcement"))
        || (0..path.len())
            .any(|depth| get(root, &path[..depth]).and_then(symbol_name) == Some("target"))
}

/// Counter references survive non-object actions, but a later creation, search,
/// move or choice must not silently keep the old bare pronoun's focus.
fn counter_focus(
    root: &Value,
    path: &[Step],
    result: u64,
    follow_move: bool,
) -> Result<(), String> {
    for (depth, step) in path.iter().enumerate() {
        let Step::Index(index) = step else { continue };
        if !matches!(depth.checked_sub(1).and_then(|i| path.get(i)), Some(Step::Key(key)) if key == "instructions")
        {
            continue;
        }
        let Some(items) = get(root, &path[..depth]).and_then(Value::as_array) else {
            continue;
        };
        let mut seen = false;
        for item in items.iter().take(*index) {
            if item["kind"] != "Effect" {
                continue;
            }
            if item["args"]["result"].as_u64() == Some(result) {
                seen = true;
                continue;
            }
            if seen
                && matches!(
                    item["args"]["operation"]["kind"].as_str(),
                    Some("CreateTokens" | "Search" | "ChooseObjects" | "Move")
                )
                && !(follow_move && item["args"]["operation"]["kind"] == "Move")
            {
                return Err("a later object action supersedes the bare counter reference".into());
            }
        }
    }
    Ok(())
}

pub(super) fn effect_result(root: &Value, path: &[Step]) -> Result<u64, String> {
    let mut value = root;
    let mut result = None;
    for step in path {
        if value["kind"] == "Effect" {
            result = value["args"]["result"].as_u64();
        }
        value = match step {
            Step::Key(key) => value.get(key),
            Step::Index(index) => value.get(*index),
        }
        .ok_or("counter effect path")?;
    }
    result.ok_or_else(|| "counter choice needs a bound effect result".into())
}

/// These bodies install a new event; lexical delayed triggers keep the creator.
fn new_event_scope(root: &Value, path: &[Step]) -> bool {
    path.iter().enumerate().any(|(depth, step)| {
        matches!(step, Step::Key(key) if key == "body")
            && depth > 0
            // An independently bound created trigger already has its own
            // event frame. Only a descendant's new event blocks that frame.
            && !(depth == 1 && root["kind"] == "CreateTrigger")
            && get(root, &path[..depth - 1]).is_some_and(|node| {
                matches!(
                    node["kind"].as_str(),
                    Some("CreateTrigger" | "CreateReplacement")
                )
            })
    })
}

/// Queries retain the object whose information they request (CR 608.2h),
/// including its last-known controller or characteristics. Target filters are
/// announced before instructions resolve. Neither follows an action's result.
fn information(root: &Value, path: &[Step]) -> bool {
    for (depth, step) in path.iter().enumerate() {
        // The shared battlefield query replaces ObjectSet::Battlefield's direct
        // filter payload. That new record field does not make an exclusion an
        // information read; it still follows the event or earlier public move.
        if matches!(step, Step::Key(key) if key == "filter")
            && depth > 0
            && get(root, &path[..depth - 1]).is_some_and(|objects| {
                objects["kind"] == "Zone"
                    && objects["args"]["scope"]["Shared"] == "Battlefield"
                    && objects["args"]["kind"] == "Permanent"
            })
        {
            continue;
        }
        if matches!(step, Step::Key(key) if matches!(key.as_str(),
            "amount" | "value" | "left" | "right" | "condition" | "intervening_if"
                | "filter" | "recipe" | "source" | "announcement" | "duration"))
        {
            return true;
        }
        if get(root, &path[..depth]).is_some_and(|value| {
            // A scalar counter read keeps its informational object. Identity
            // predicates inside a recipient population still follow movement.
            (value["kind"] == "CountCounters"
                && value["args"]["information"] == "CurrentOrLastKnown"
                && value["args"]["recipients"]["kind"] == "Objects"
                && value["args"]["recipients"]["args"]["kind"] == "One"
                && path[depth..]
                    .iter()
                    .map(|step| match step {
                        Step::Key(key) => key.as_str(),
                        Step::Index(_) => "",
                    })
                    .eq(["args", "recipients", "args", "args"]))
                || matches!(
                    value["kind"].as_str(),
                    Some(
                        "ObjectProperty"
                            | "ControllerOf"
                            | "OwnerOf"
                            | "CasterOf"
                            | "AttachedTo"
                            | "MadeBy"
                            | "ExiledWith"
                            | "SharesNameWith"
                            | "SharesColorWith"
                            | "Shares"
                            | "History"
                    )
                )
        }) {
            return true;
        }
    }
    false
}

/// Other parts of an effect can find an object it moved to a public zone
/// (CR 400.7j). Hand and library moves do not establish that new referent.
fn public_destination(destination: &Value) -> bool {
    public_zone(destination).is_some()
}

fn get<'a>(mut value: &'a Value, path: &[Step]) -> Option<&'a Value> {
    for step in path {
        value = match step {
            Step::Key(key) => value.get(key)?,
            Step::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}

#[cfg(test)]
mod event_scope_tests {
    use super::*;

    #[test]
    fn a_created_trigger_uses_its_own_frame_but_keeps_descendant_events_separate() {
        let body = [
            Step::Key("args".into()),
            Step::Key("body".into()),
            Step::Key("instructions".into()),
        ];
        let created = json!({"kind":"CreateTrigger","args":{"body":{"instructions":[]}}});
        assert!(!new_event_scope(&created, &body));
        let mut outer = json!({"kind":"Triggered","args":{"body":{"instructions":[
            {"kind":"Effect","args":{"operation":created}}
        ]}}});
        let mut nested = body.to_vec();
        nested.extend([
            Step::Index(0),
            Step::Key("args".into()),
            Step::Key("operation".into()),
            Step::Key("args".into()),
            Step::Key("body".into()),
        ]);
        assert!(new_event_scope(&outer, &nested));
        outer["kind"] = "CreateTrigger".into();
        assert!(new_event_scope(&outer, &nested));
        for kind in ["CreateDelayedTrigger", "CreateReplacement"] {
            let root = json!({"kind":kind,"args":{"body":{"instructions":[]}}});
            assert_eq!(new_event_scope(&root, &body), kind == "CreateReplacement");
        }
    }
}

#[cfg(test)]
mod type_information_tests {
    use super::*;

    #[test]
    fn creature_type_relation_is_information_but_same_object_is_identity() {
        for (kind, property, expected) in [
            ("Shares", "CreatureTypes", true),
            ("Shares", "CardTypes", true),
            ("Shares", "Colors", true),
            ("Shares", "Owners", true),
            ("SameObject", "CreatureTypes", false),
        ] {
            let root = json!({"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},
                "kind":"Permanent","filter":{"kind":kind,"args":{
                    "property":property,"with":{"kind":"EventSubject"}}}}});
            let path = ["args", "filter", "args", "with"].map(|s| Step::Key(s.into()));
            assert_eq!(information(&root, &path), expected, "{kind}/{property}");
        }
    }

    #[test]
    fn whole_native_type_information_and_current_card_controls() {
        let engine = crate::engine::Engine::load().unwrap();
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../tests/fixtures/death-type-information.json"
        ))
        .unwrap();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let mut actual = engine
                .parse(name, case["text"].as_str().unwrap())
                .unwrap()
                .readings;
            let mut expected: Vec<_> = case["expected_readings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| crate::engine::canonical_abilities(v).unwrap())
                .collect();
            actual.sort_by_key(Value::to_string);
            expected.sort_by_key(Value::to_string);
            assert_eq!(actual, expected, "{name}");
        }
    }
    #[test]
    fn ordinary_type_relation_and_query_choices_compose_without_learned_rules() {
        let engine = crate::engine::Engine::load().unwrap();
        let off = engine.rule_set(|name| name.starts_with("m_") || name.starts_with("t_"));
        let family: Value =
            serde_json::from_str(include_str!("../../families/death-type-information.json"))
                .unwrap();
        for case in family["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let actual = engine
                .parse_without(name, case["text"].as_str().unwrap(), &off)
                .unwrap()
                .readings;
            assert_eq!(
                actual,
                vec![crate::engine::canonical_abilities(&case["expected"]).unwrap()],
                "{name}"
            );
        }
    }
    #[test]
    fn scalar_counter_information_keeps_before_but_current_and_query_identity_do_not() {
        let root = json!({"kind":"CountCounters","args":{
            "recipients":{"kind":"Objects","args":{"kind":"One","args":{"$sym":"it","args":[]}}},
            "kind":{"Named":"OilCounter"},"information":"CurrentOrLastKnown"}});
        let path = ["args", "recipients", "args", "args"].map(|key| Step::Key(key.into()));
        assert!(information(&root, &path));
        let frame = Frame::of_ability(&json!({"kind":"Triggered","args":{"event":{
            "ZoneChange":{"objects":{"kind":"CardType","args":"Creature"},
            "from":"Battlefield","to":"Graveyard"}
        }}}));
        let intro = Introductions {
            slots: &[],
            base: 0,
            keys: &[],
        };
        let before = json!({"kind":"EventSubject"});
        assert_eq!(
            reference("it", &root, &path, &intro, &frame, before.clone(), None).unwrap(),
            before
        );
        let current = reference(
            "current_it",
            &root,
            &path,
            &intro,
            &frame,
            before.clone(),
            None,
        )
        .unwrap();
        assert_ne!(current, before);
        let query = json!({"kind":"CountCounters","args":{
            "recipients":{"kind":"Objects","args":{"kind":"Zone","args":{"scope":{"Shared":"Battlefield"},
                "kind":"Permanent","filter":{"kind":"Not","args":{"kind":"SameObject",
                    "args":{"$sym":"it","args":[]}}}}}},
            "kind":{"Named":"OilCounter"},"information":"CurrentOrLastKnown"}});
        let query = json!({"kind":"Triggered","args":{"body":{"instructions":[
            {"kind":"Effect","args":{"operation":{"kind":"ConjureCopy","args":{"count":query}}}}
        ]}}});
        let mut qp = vec![
            Step::Key("args".into()),
            Step::Key("body".into()),
            Step::Key("instructions".into()),
            Step::Index(0),
        ];
        qp.extend(
            [
                "args",
                "operation",
                "args",
                "count",
                "args",
                "recipients",
                "args",
                "args",
                "filter",
                "args",
                "args",
            ]
            .map(|key| Step::Key(key.into())),
        );
        assert!(!information(&query, &qp));
        assert_eq!(
            reference("it", &query, &qp, &intro, &frame, before.clone(), None).unwrap(),
            current
        );
        assert_ne!(
            reference("it", &query, &qp, &intro, &frame, before.clone(), None).unwrap(),
            before
        );
    }
}
