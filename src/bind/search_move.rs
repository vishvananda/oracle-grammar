//! Scalar focus from proved search and random library-selection movements.
use super::{Scope, Step, at_path, transitions};
use crate::model::ir::{
    Count, EventZone, Expr, MovementDestination, ObjectFilter, ObjectKind, ObjectSet, Operation,
    PlayerRef, PlayerSet, SelectionConstraint, ZoneScope,
};
use serde_json::{Value, json};

fn single(count: &Count) -> bool {
    matches!(
        count,
        Count::UpTo(Expr::Constant(1))
            | Count::Exactly(Expr::Constant(1))
            | Count::AsManyAsCan(Expr::Constant(1))
    )
}

fn producer(operation: Operation, seek: bool) -> bool {
    match operation {
        Operation::Search { choices, .. } if !seek => {
            choices.len() == 1 && single(&choices[0].count)
        }
        Operation::ChooseObjects {
            player: PlayerRef::AbilityController,
            objects:
                ObjectSet::Zone {
                    scope:
                        ZoneScope::Players {
                            zone: EventZone::Library,
                            players: PlayerSet::One(PlayerRef::AbilityController),
                        },
                    kind: ObjectKind::Card,
                    filter,
                },
            count,
            constraints,
        } if seek => {
            single(&count)
                && matches!(constraints.as_slice(), [SelectionConstraint::AtRandom])
                && library_filter(&filter)
        }
        _ => false,
    }
}

// Library cards have owners but no controllers. A following control qualifier
// belongs to the battlefield population inside a comparison, not its card.
fn library_filter(filter: &ObjectFilter) -> bool {
    match filter {
        ObjectFilter::ControlledBy(_) => false,
        ObjectFilter::AllOf(filters) | ObjectFilter::AnyOf(filters) => {
            filters.iter().all(library_filter)
        }
        ObjectFilter::Not(filter) => library_filter(filter),
        _ => true,
    }
}

pub(super) fn marker(
    root: &mut Value,
    scope: &mut Scope,
    path: &[Step],
    name: &str,
    introduced: &[super::references::TargetIntroduction],
) -> Result<bool, String> {
    let seek = name == "seek_move_result";
    if !seek && name != "search_move_result" {
        return Ok(false);
    }
    if path.len() > 128 {
        return Err("search move receipt depth limit".into());
    }
    let mut work = 32768;
    let mut bytes = 1048576;
    if !bounded(root, 0, &mut work, &mut bytes) {
        return Err("search move work limit".into());
    }
    let wrapped = at_path(root, path).ok_or("search move marker path")?["args"]
        .get(0)
        .ok_or("search move needs an operation")?
        .clone();
    let mut plain = super::references::bound_reference(&wrapped, introduced, scope.target_base)?;
    super::filters::strip(&mut plain);
    let operation: Operation =
        serde_json::from_value(plain).map_err(|why| format!("search move operation: {why}"))?;
    let Operation::Move {
        objects: ObjectSet::Selected(selected),
        destination,
        ..
    } = operation
    else {
        return Err("search/seek move needs Selected objects".into());
    };
    if !matches!(
        (&destination, seek),
        (MovementDestination::OwnersHand(_), true)
            | (MovementDestination::Battlefield { .. }, false)
    ) {
        return Err("search/seek move destination disagrees with its receipt".into());
    }
    let (depth, index) = path.iter().enumerate().rev().find_map(|(depth, step)| {
        if let Step::Index(index) = step
            && matches!(depth.checked_sub(1).and_then(|n| path.get(n)), Some(Step::Key(k)) if k == "instructions") {
            Some((depth, *index))
        } else { None }
    }).ok_or("search move needs instruction sequence")?;
    let instructions = at_path(root, &path[..depth])
        .and_then(|v| v.as_array())
        .ok_or("search move instruction sequence")?;
    if instructions.len() > 256 {
        return Err("search move instruction limit".into());
    }
    let candidate = instructions[..index].iter().rev().find(|instruction| {
        instruction["kind"] == "Effect"
            && instruction["args"]["result"].as_u64() == Some(selected.0.into())
    });
    let scalar = match candidate {
        Some(instruction) => {
            let mut plain = super::references::bound_reference(
                &instruction["args"]["operation"],
                introduced,
                scope.target_base,
            )?;
            super::filters::strip(&mut plain);
            serde_json::from_value(plain).is_ok_and(|operation| producer(operation, seek))
        }
        None => false,
    };
    if seek && !scalar {
        return Err("seek move requires one random card from the controller's library".into());
    }
    // Shared found-actions also serve plural searches. They retain their exact
    // old operation, without manufacturing a scalar referent.
    if scalar
        && (seek
            || instructions.get(index + 1).is_some_and(|next| {
                next["kind"] == "Effect" && next["args"]["operation"]["kind"] == "Shuffle"
            }))
    {
        let result = transitions::effect_result(root, path)?;
        let reference = json!({"kind":"Result","args":{
            "result":result,"phase":"After","index":0}});
        scope.objects.push(reference.clone());
        scope.named_object = Some(reference);
        scope.others.clear();
        scope.pronoun = None;
    }
    Ok(true)
}

fn bounded(value: &Value, depth: usize, work: &mut usize, bytes: &mut usize) -> bool {
    if depth > 128 || *work == 0 {
        return false;
    }
    *work -= 1;
    match value {
        Value::String(text) => charge(bytes, text.len()),
        Value::Array(values) => values.iter().all(|v| bounded(v, depth + 1, work, bytes)),
        Value::Object(values) => values
            .iter()
            .all(|(k, v)| charge(bytes, k.len()) && bounded(v, depth + 1, work, bytes)),
        _ => true,
    }
}

fn charge(bytes: &mut usize, amount: usize) -> bool {
    match bytes.checked_sub(amount) {
        Some(left) => {
            *bytes = left;
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;

    fn engine() -> &'static Engine {
        crate::test_support::engine()
    }

    #[test]
    fn seek_receipts_keep_followups_off_the_source_and_previous_target() {
        let parsed = engine()
            .parse(
                "Seek Study",
                "Tap target creature. Seek a land card and put it onto the battlefield tapped.",
            )
            .unwrap();
        assert_eq!(parsed.readings.len(), 1);
        let steps = &parsed.readings[0][0]["args"]["body"]["instructions"];
        assert_eq!(
            steps[3]["args"]["operation"]["args"]["objects"],
            json!({"kind":"One","args":{"kind":"Result","args":{
            "result":steps[2]["args"]["result"],"phase":"After","index":0}}})
        );
        let parsed = engine().parse("Seek Study", "When this creature enters, seek a creature card and put a +1/+1 counter on this creature.").unwrap();
        assert_eq!(parsed.readings.len(), 1);
        assert_eq!(
            parsed.readings[0][0]["args"]["body"]["instructions"][2]["args"]["operation"]["args"]["placements"]
                [0]["recipients"]["args"],
            json!({"kind":"One","args":{"kind":"Source"}})
        );
    }

    #[test]
    fn seek_comparison_qualifiers_belong_to_the_counted_battlefield_population() {
        let parsed = engine()
            .parse(
                "Seek Study",
                "Seek a permanent card with mana value equal to the number of lands you control.",
            )
            .unwrap();
        assert_eq!(parsed.readings.len(), 1);
        let filter = &parsed.readings[0][0]["args"]["body"]["instructions"][0]["args"]["operation"]
            ["args"]["objects"]["args"]["filter"];
        assert_eq!(filter["args"].as_array().unwrap().len(), 2);
        assert_eq!(
            filter["args"][1]["args"]["test"]["args"]["value"]["args"]["args"]["filter"]["args"][1]
                ["kind"],
            "ControlledBy"
        );
        assert!(
            engine()
                .parse("Seek Study", "Seek a land card you control.")
                .is_err()
        );
    }

    fn example(count: u64) -> (Value, Vec<Step>) {
        let fixtures: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/search-move-receipt.json"
        ))
        .unwrap();
        let frame = fixtures
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == "Fabled Passage")
            .unwrap();
        let mut root = frame["expected"][0]["args"]["body"]["instructions"][0]["args"].clone();
        root["instructions"][0]["args"]["result"] = json!(19);
        root["instructions"][0]["args"]["operation"]["args"]["choices"][0]["count"]["UpTo"]["args"] =
            json!(count);
        root["instructions"][1]["args"]["result"] = json!(27);
        root["instructions"][1]["args"]["operation"]["args"]["objects"]["args"] = json!(19);
        let operation = root["instructions"][1]["args"]["operation"].take();
        root["instructions"][1]["args"]["operation"] =
            json!({"$sym":"search_move_result","args":[operation]});
        (
            root,
            vec![
                Step::Key("instructions".into()),
                Step::Index(1),
                Step::Key("args".into()),
                Step::Key("operation".into()),
            ],
        )
    }
    #[test]
    fn actual_move_after_address_survives_shuffle() {
        let (mut root, path) = example(1);
        let mut scope = Scope::default();
        assert!(marker(&mut root, &mut scope, &path, "search_move_result", &[]).unwrap());
        assert_eq!(
            scope.object(),
            Some(json!({"kind":"Result","args":{"result":27,"phase":"After","index":0}}))
        );
        assert_eq!(
            root["instructions"][1]["args"]["operation"]["args"][0]["kind"],
            "Move"
        );
    }
    #[test]
    fn plural_search_does_not_guess_first_card() {
        let (mut root, path) = example(2);
        let mut scope = Scope::default();
        marker(&mut root, &mut scope, &path, "search_move_result", &[]).unwrap();
        assert!(scope.objects.is_empty());
    }
    #[test]
    fn mismatched_search_or_missing_shuffle_has_no_focus() {
        for mismatch in [true, false] {
            let (mut root, path) = example(1);
            if mismatch {
                root["instructions"][0]["args"]["result"] = json!(18);
            } else {
                root["instructions"].as_array_mut().unwrap().remove(2);
            }
            let mut scope = Scope::default();
            marker(&mut root, &mut scope, &path, "search_move_result", &[]).unwrap();
            assert!(scope.objects.is_empty());
        }
    }
    #[test]
    fn raw_strings_and_keys_are_bounded_before_cloning() {
        for key in [true, false] {
            let (mut root, path) = example(1);
            if key {
                root.as_object_mut()
                    .unwrap()
                    .insert("x".repeat(1048577), Value::Null);
            } else {
                root["large"] = json!("x".repeat(1048577));
            }
            assert!(
                marker(
                    &mut root,
                    &mut Scope::default(),
                    &path,
                    "search_move_result",
                    &[]
                )
                .is_err()
            );
        }
    }
    #[test]
    fn assigned_target_player_preview_does_not_consume_introductions() {
        let (mut root, path) = example(1);
        let target = json!({"kind":"Target","args":{"slot":{
            "$sym":"target","args":["player"],"at":4},"index":0}});
        root["instructions"][0]["args"]["operation"]["args"]["searcher"] = target.clone();
        root["instructions"][0]["args"]["operation"]["args"]["library_owner"] = target;
        let original = root["instructions"][0].clone();
        let mut scope = Scope {
            target_base: 3,
            ..Scope::default()
        };
        let introduced = vec![((4, json!("player").to_string()), 1)];
        marker(
            &mut root,
            &mut scope,
            &path,
            "search_move_result",
            &introduced,
        )
        .unwrap();
        assert_eq!(scope.object().unwrap()["args"]["result"], 27);
        assert_eq!(root["instructions"][0], original);
    }

    #[test]
    fn typed_preview_ignores_context_metadata_without_mutating_the_source() {
        let (mut root, path) = example(1);
        root["instructions"][0]["args"]["operation"]["args"]["choices"][0]["filter"] = json!({"$sym":"in_filter","args":[{"kind":"SharesNameWith",
                "args":{"$sym":"pump_name_ref","args":[{"kind":"Source"}]}}]});
        let original = root["instructions"][0].clone();
        let mut scope = Scope::default();
        marker(&mut root, &mut scope, &path, "search_move_result", &[]).unwrap();
        assert_eq!(scope.object().unwrap()["args"]["result"], 27);
        assert_eq!(root["instructions"][0], original);
    }
}
